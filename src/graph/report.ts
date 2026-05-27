import type { CheckResult, Diagnostic } from "../lang/diagnostics.js";
import type { GraphEdge } from "./edges.js";
import type { GraphNode } from "./nodes.js";
import type { GraphStore } from "./store.js";
import { countBy } from "../util/strings.js";

export async function buildGraphReport(store: GraphStore, nodes: GraphNode[], edges: GraphEdge[]) {
  const nodesByType = countBy(nodes, (node) => node.type);
  const nodesByStatus = countBy(nodes, (node) => node.status);
  const diagnosticCounts: Record<string, number> = {};
  const codeNodes = [];

  for (const codeNode of nodes.filter((node) => node.type === "CodeNode")) {
    const childEdges = edges.filter((edge) => edge.from === codeNode.id);
    const checkNodes = await resultSummaries(
      store,
      childEdges,
      "checks",
      "CheckResultNode",
      diagnosticCounts,
    );
    const auditNodes = await resultSummaries(
      store,
      childEdges,
      "audits",
      "AuditNode",
      diagnosticCounts,
    );
    const diffNodes = await resultSummaries(
      store,
      childEdges,
      "diffs",
      "ArtifactDiffNode",
      diagnosticCounts,
    );
    codeNodes.push({
      id: codeNode.id,
      status: codeNode.status,
      action_used: codeNode.action_used,
      created_from: codeNode.created_from,
      model: codeNode.model,
      checks: checkNodes,
      audits: auditNodes,
      diffs: diffNodes,
    });
  }

  const experiments = await buildExperimentSummaries(
    store,
    nodes,
    edges,
    codeNodes,
    diagnosticCounts,
  );

  return {
    nodes_by_type: nodesByType,
    nodes_by_status: nodesByStatus,
    metrics: buildReportMetrics(nodes, edges, codeNodes),
    experiments,
    llm_failed_nodes: nodes.filter((node) => node.type === "RawLlmNode" && node.status === "failed")
      .length,
    diagnostic_counts: Object.fromEntries(
      Object.entries(diagnosticCounts).sort(([left], [right]) => left.localeCompare(right)),
    ),
    code_nodes: codeNodes,
  };
}

type ResultSummary = {
  id: string;
  ok: boolean;
  diagnostics: number;
  errors: number;
  warnings: number;
  codes: Record<string, number>;
};

async function buildExperimentSummaries(
  store: GraphStore,
  nodes: GraphNode[],
  edges: GraphEdge[],
  codeNodes: Array<{
    id: string;
    action_used: string;
    created_from: string | null;
    checks: ResultSummary[];
    audits: ResultSummary[];
    diffs: Array<{ ok: boolean }>;
  }>,
  diagnosticCounts: Record<string, number>,
) {
  const nodesById = new Map(nodes.map((node) => [node.id, node]));
  return Promise.all(
    nodes
      .filter((node) => node.type === "PseudocodeNode")
      .map(async (pseudoNode) => {
        const pseudoCheckNodes = await resultSummaries(
          store,
          edges.filter((edge) => edge.from === pseudoNode.id),
          "checks",
          "PseudocodeCheckNode",
          diagnosticCounts,
        );
        const relatedCodeNodes = codeNodes.filter(
          (codeNode) => nearestPseudocodeAncestor(nodesById, codeNode.id) === pseudoNode.id,
        );
        const implementationNodes = relatedCodeNodes.filter(
          (node) => node.action_used === "implement_design",
        );
        const repairNodes = relatedCodeNodes.filter((node) => node.action_used === "repair_code");
        const latestById =
          [...relatedCodeNodes].sort((left, right) => right.id.localeCompare(left.id))[0] ?? null;
        const implementationPassed = implementationNodes.filter(
          (node) => node.checks.at(-1)?.ok === true,
        ).length;

        return {
          pseudocode_node: pseudoNode.id,
          fixture: extractFixtureFromSummary(pseudoNode.summary),
          goal: pseudoNode.goal ?? null,
          pseudocode_checks: pseudoCheckNodes.length,
          latest_pseudocode_check_ok: pseudoCheckNodes.at(-1)?.ok ?? null,
          pseudocode_diagnostic_types: countResultCodes(pseudoCheckNodes),
          implementation_attempts: implementationNodes.length,
          implementation_passed: implementationPassed,
          implementation_success_rate:
            implementationNodes.length === 0
              ? null
              : implementationPassed / implementationNodes.length,
          repair_attempts: repairNodes.length,
          repaired_passed: repairNodes.filter((node) => node.checks.at(-1)?.ok === true).length,
          audited_passed: relatedCodeNodes.filter((node) => node.audits.at(-1)?.ok === true).length,
          materialize_ready: relatedCodeNodes.filter(
            (node) =>
              node.checks.at(-1)?.ok === true &&
              node.audits.at(-1)?.ok === true &&
              (node.action_used !== "repair_code" || node.diffs.at(-1)?.ok === true),
          ).length,
          checker_error_types: countResultCodes(relatedCodeNodes.flatMap((node) => node.checks)),
          audit_risk_types: countResultCodes(relatedCodeNodes.flatMap((node) => node.audits)),
          latest_code_node: latestById?.id ?? null,
          latest_check_ok: latestById?.checks.at(-1)?.ok ?? null,
          latest_audit_ok: latestById?.audits.at(-1)?.ok ?? null,
        };
      }),
  );
}

function nearestPseudocodeAncestor(
  nodesById: Map<string, GraphNode>,
  nodeId: string,
): string | null {
  let current = nodesById.get(nodeId);
  const visited = new Set<string>();
  while (current?.created_from) {
    if (visited.has(current.id)) return null;
    visited.add(current.id);
    current = nodesById.get(current.created_from);
    if (current?.type === "PseudocodeNode") return current.id;
  }
  return null;
}

function extractFixtureFromSummary(summary: string): string | null {
  return /Pseudocode from ([^.\n]+\.pseudo)/.exec(summary)?.[1] ?? null;
}

function countResultCodes(
  results: Array<{ codes?: Record<string, number> }>,
): Record<string, number> {
  const counts: Record<string, number> = {};
  for (const result of results) {
    for (const [code, count] of Object.entries(result.codes ?? {})) {
      counts[code] = (counts[code] ?? 0) + count;
    }
  }
  return Object.fromEntries(
    Object.entries(counts).sort(([left], [right]) => left.localeCompare(right)),
  );
}

function buildReportMetrics(
  nodes: GraphNode[],
  edges: GraphEdge[],
  codeNodes: Array<{
    id: string;
    action_used: string;
    status: string;
    checks: Array<{ ok: boolean }>;
    audits: Array<{ ok: boolean }>;
    diffs: Array<{ ok: boolean }>;
  }>,
) {
  const nodesById = new Map(nodes.map((node) => [node.id, node]));
  const selectedCodeNodeIds = new Set([
    ...edges
      .filter((edge) => nodes.some((node) => node.id === edge.to && node.type === "SelectionNode"))
      .filter((edge) => nodesById.get(edge.from)?.type === "CodeNode")
      .map((edge) => edge.from),
  ]);
  const materializedCodeNodeIds = new Set([
    ...edges
      .filter((edge) =>
        nodes.some((node) => node.id === edge.to && node.type === "MaterializeNode"),
      )
      .filter((edge) => nodesById.get(edge.from)?.type === "CodeNode")
      .map((edge) => edge.from),
  ]);

  return {
    code_nodes_total: codeNodes.length,
    decision_nodes_total: nodes.filter((node) => node.type === "DecisionNode").length,
    llm_decision_nodes: nodes.filter(
      (node) => node.type === "DecisionNode" && node.action_used === "llm_decide",
    ).length,
    repaired_code_nodes: codeNodes.filter((node) => node.action_used === "repair_code").length,
    selected_code_nodes: selectedCodeNodeIds.size,
    materialized_code_nodes: materializedCodeNodeIds.size,
    checked_code_nodes: codeNodes.filter((node) => node.checks.length > 0).length,
    latest_check_passed: codeNodes.filter((node) => node.checks.at(-1)?.ok === true).length,
    latest_check_failed: codeNodes.filter((node) => node.checks.at(-1)?.ok === false).length,
    audited_code_nodes: codeNodes.filter((node) => node.audits.length > 0).length,
    latest_audit_passed: codeNodes.filter((node) => node.audits.at(-1)?.ok === true).length,
    latest_audit_failed: codeNodes.filter((node) => node.audits.at(-1)?.ok === false).length,
    repaired_code_nodes_with_diff: codeNodes.filter(
      (node) => node.action_used === "repair_code" && node.diffs.length > 0,
    ).length,
    latest_diff_failed: codeNodes.filter((node) => node.diffs.at(-1)?.ok === false).length,
    raw_llm_failed_nodes: nodes.filter(
      (node) => node.type === "RawLlmNode" && node.status === "failed",
    ).length,
  };
}

async function resultSummaries(
  store: GraphStore,
  edges: GraphEdge[],
  edgeType: string,
  nodeType: GraphNode["type"],
  diagnosticCounts: Record<string, number>,
): Promise<ResultSummary[]> {
  const resultNodes = await Promise.all(
    edges.filter((edge) => edge.type === edgeType).map(async (edge) => store.readNode(edge.to)),
  );
  return Promise.all(
    resultNodes
      .filter((node) => node.type === nodeType)
      .sort((left, right) => left.id.localeCompare(right.id))
      .map(async (node) => {
        const result = await store.readArtifactJson<CheckResult>(node, "result.json");
        for (const diagnostic of result.diagnostics) {
          diagnosticCounts[diagnostic.code] = (diagnosticCounts[diagnostic.code] ?? 0) + 1;
        }
        return {
          id: node.id,
          ok: result.ok,
          diagnostics: result.diagnostics.length,
          errors: result.diagnostics.filter((diagnostic) => diagnostic.severity === "error").length,
          warnings: result.diagnostics.filter((diagnostic) => diagnostic.severity === "warning")
            .length,
          codes: countBy(result.diagnostics, (diagnostic) => diagnostic.code),
        };
      }),
  );
}
