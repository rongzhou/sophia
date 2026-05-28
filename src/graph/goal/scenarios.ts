import { readFile, readdir, stat } from "node:fs/promises";
import path from "node:path";
import { z } from "zod";
import { buildGoalContext } from "./context.js";
import {
  acceptChangeRequest,
  acceptObjectiveDecomposition,
  activateMilestone,
  createAcceptanceNode,
  createChangeRequestNode,
  createImpactAnalysisNode,
  createObjectiveNode,
  decomposeObjective,
  invalidateDecomposition,
  redecomposeObjective,
  type ImpactAnalysisPayloadInput,
} from "./workflow.js";
import type { GraphNode } from "../core/nodes.js";
import type { GraphStore } from "../core/store.js";

const RefSchema = z.string().regex(/^[a-z][a-z0-9_]*$/);
const StringArraySchema = z.array(z.string().min(1)).default([]);

const ScenarioObjectiveInputSchema = z.object({
  ref: RefSchema,
  status: z.enum(["open", "active", "satisfied", "superseded", "abandoned"]),
  title: z.string().min(1),
  description: z.string().min(1),
  constraints: StringArraySchema,
  acceptance: StringArraySchema,
});

const ScenarioMilestoneInputSchema = z.object({
  ref: RefSchema,
  status: z.enum(["planned", "active", "accepted", "rejected", "superseded"]),
  name: z.string().min(1),
  scope: StringArraySchema,
  out_of_scope: StringArraySchema,
  acceptance: StringArraySchema,
});

const ScenarioDecomposeStepSchema = z.object({
  type: z.literal("decompose"),
  decomposition_id: z.string().min(1),
  objectives: z.array(ScenarioObjectiveInputSchema).min(1),
  milestones: z.array(ScenarioMilestoneInputSchema).default([]),
});

const ScenarioRedecomposeStepSchema = ScenarioDecomposeStepSchema.extend({
  type: z.literal("redecompose"),
  previous_decomposition_id: z.string().min(1),
  reason: z.string().min(1),
});

const ScenarioStepSchema = z.discriminatedUnion("type", [
  ScenarioDecomposeStepSchema,
  z.object({
    type: z.literal("accept_decomposition"),
    decomposition_id: z.string().min(1),
  }),
  z.object({
    type: z.literal("invalidate_decomposition"),
    decomposition_id: z.string().min(1),
    reason: z.string().min(1),
  }),
  ScenarioRedecomposeStepSchema,
  z.object({
    type: z.literal("activate_milestone"),
    milestone_ref: RefSchema,
  }),
  z.object({
    type: z.literal("record_change"),
    ref: RefSchema,
    source_ref: RefSchema,
    kind: z.enum(["new_requirement", "correction", "preference", "rejection", "constraint_change"]),
    request: z.string().min(1),
    applies_to: StringArraySchema,
    priority: z.enum(["must", "should", "could"]),
  }),
  z.object({
    type: z.literal("analyze_change"),
    ref: RefSchema,
    change_ref: RefSchema,
    payload: z.record(z.string(), z.unknown()),
  }),
  z.object({
    type: z.literal("accept_change"),
    change_ref: RefSchema,
    impact_ref: RefSchema,
  }),
  z.object({
    type: z.literal("record_acceptance"),
    source_ref: RefSchema,
    target_ref: RefSchema,
    decision: z.enum(["accepted", "rejected", "accepted_with_changes"]),
    notes: z.string().default(""),
  }),
]);

export const GoalGraphScenarioSchema = z.object({
  id: z.string().regex(/^[a-z0-9_]+$/),
  title: z.string().min(1),
  purpose: z.string().min(1),
  root_objective: z.object({
    title: z.string().min(1),
    description: z.string().min(1),
    constraints: StringArraySchema,
    acceptance: StringArraySchema,
  }),
  steps: z.array(ScenarioStepSchema).min(1),
  records: z.object({
    prompt: z.string().min(1),
    llm_response: z.unknown(),
    final_verification: z.object({
      check_ok: z.boolean(),
      audit_ok: z.boolean(),
      verify_ok: z.boolean(),
      notes: StringArraySchema,
    }),
  }),
});

export type GoalGraphScenario = z.infer<typeof GoalGraphScenarioSchema>;

export interface MaterializedGoalGraphScenario {
  scenario_id: string;
  title: string;
  purpose: string;
  refs: Record<string, string>;
  graph: {
    nodes: GraphNode[];
    edges: Awaited<ReturnType<GraphStore["listEdges"]>>;
  };
  records: GoalGraphScenario["records"];
  active_context: Awaited<ReturnType<typeof buildGoalContext>>;
}

export async function loadGoalGraphScenario(filePath: string): Promise<GoalGraphScenario> {
  return GoalGraphScenarioSchema.parse(JSON.parse(await readFile(filePath, "utf8")));
}

export async function loadGoalGraphScenarioSuite(root: string): Promise<GoalGraphScenario[]> {
  const files = await findScenarioFiles(root);
  const scenarios = await Promise.all(files.map(loadGoalGraphScenario));
  return scenarios.sort((left, right) => left.id.localeCompare(right.id));
}

export async function materializeGoalGraphScenario(options: {
  store: GraphStore;
  scenario: GoalGraphScenario;
}): Promise<MaterializedGoalGraphScenario> {
  const refs = new Map<string, GraphNode>();
  const root = await createObjectiveNode({
    store: options.store,
    payload: {
      origin: "human",
      authority: "authoritative",
      status: "open",
      title: options.scenario.root_objective.title,
      description: options.scenario.root_objective.description,
      constraints: options.scenario.root_objective.constraints,
      acceptance: options.scenario.root_objective.acceptance,
      parent_objective: null,
    },
  });
  refs.set("root", root);

  for (const step of options.scenario.steps) {
    switch (step.type) {
      case "decompose": {
        const result = await decomposeObjective({
          store: options.store,
          parentObjective: root,
          decompositionId: step.decomposition_id,
          objectives: step.objectives.map(({ ref: _ref, ...objective }) => objective),
          milestones: step.milestones.map(({ ref: _ref, ...milestone }) => milestone),
        });
        recordCreatedRefs(refs, step.objectives, result.objective_nodes);
        recordCreatedRefs(refs, step.milestones, result.milestone_nodes);
        break;
      }
      case "accept_decomposition":
        await acceptObjectiveDecomposition({
          store: options.store,
          parentObjective: root,
          decompositionId: step.decomposition_id,
        });
        await refreshKnownRefs(options.store, refs);
        break;
      case "invalidate_decomposition":
        await invalidateDecomposition({
          store: options.store,
          parentObjective: root,
          decompositionId: step.decomposition_id,
          reason: step.reason,
        });
        await refreshKnownRefs(options.store, refs);
        break;
      case "redecompose": {
        const result = await redecomposeObjective({
          store: options.store,
          parentObjective: root,
          previousDecompositionId: step.previous_decomposition_id,
          decompositionId: step.decomposition_id,
          reason: step.reason,
          objectives: step.objectives.map(({ ref: _ref, ...objective }) => objective),
          milestones: step.milestones.map(({ ref: _ref, ...milestone }) => milestone),
        });
        await refreshKnownRefs(options.store, refs);
        recordCreatedRefs(refs, step.objectives, result.objective_nodes);
        recordCreatedRefs(refs, step.milestones, result.milestone_nodes);
        break;
      }
      case "activate_milestone": {
        const milestone = await activateMilestone({
          store: options.store,
          milestoneNode: requireRef(refs, step.milestone_ref),
        });
        refs.set(step.milestone_ref, milestone);
        break;
      }
      case "record_change": {
        const change = await createChangeRequestNode({
          store: options.store,
          createdFrom: requireRef(refs, step.source_ref),
          payload: {
            origin: "human",
            authority: "authoritative",
            status: "proposed",
            kind: step.kind,
            request: step.request,
            applies_to: step.applies_to,
            priority: step.priority,
          },
        });
        refs.set(step.ref, change);
        break;
      }
      case "analyze_change": {
        const change = requireRef(refs, step.change_ref);
        const impact = await createImpactAnalysisNode({
          store: options.store,
          createdFrom: change,
          payload: resolveImpactPayload(step.payload, refs, change.id),
        });
        refs.set(step.ref, impact);
        break;
      }
      case "accept_change": {
        const result = await acceptChangeRequest({
          store: options.store,
          changeRequestNode: requireRef(refs, step.change_ref),
          impactAnalysisNode: requireRef(refs, step.impact_ref),
        });
        refs.set(step.change_ref, result.change_request);
        refs.set(step.impact_ref, result.impact_analysis);
        break;
      }
      case "record_acceptance": {
        const acceptance = await createAcceptanceNode({
          store: options.store,
          createdFrom: requireRef(refs, step.source_ref),
          payload: {
            origin: "human",
            authority: "authoritative",
            status: "recorded",
            target: requireRef(refs, step.target_ref).id,
            decision: step.decision,
            notes: step.notes,
            creates_change_request: null,
          },
        });
        refs.set(`${step.target_ref}_acceptance`, acceptance);
        break;
      }
    }
  }

  return {
    scenario_id: options.scenario.id,
    title: options.scenario.title,
    purpose: options.scenario.purpose,
    refs: Object.fromEntries([...refs.entries()].map(([key, node]) => [key, node.id])),
    graph: {
      nodes: await options.store.listNodes(),
      edges: await options.store.listEdges(),
    },
    records: options.scenario.records,
    active_context: await buildGoalContext(options.store),
  };
}

async function findScenarioFiles(root: string): Promise<string[]> {
  const info = await stat(root);
  if (info.isFile()) return [root];
  const entries = await readdir(root, { withFileTypes: true });
  const nested = await Promise.all(
    entries.map(async (entry) => {
      const entryPath = path.join(root, entry.name);
      if (entry.isDirectory()) return findScenarioFiles(entryPath);
      return entry.name === "scenario.json" ? [entryPath] : [];
    }),
  );
  return nested.flat().sort();
}

function recordCreatedRefs(
  refs: Map<string, GraphNode>,
  inputs: Array<{ ref: string }>,
  nodes: GraphNode[],
): void {
  if (inputs.length !== nodes.length) {
    throw new Error(
      `Scenario ref count ${inputs.length} does not match created nodes ${nodes.length}.`,
    );
  }
  for (const [index, input] of inputs.entries()) {
    refs.set(input.ref, nodes[index]!);
  }
}

async function refreshKnownRefs(store: GraphStore, refs: Map<string, GraphNode>): Promise<void> {
  for (const [key, node] of refs) {
    refs.set(key, await store.readNode(node.id));
  }
}

function requireRef(refs: Map<string, GraphNode>, ref: string): GraphNode {
  const node = refs.get(ref);
  if (!node) {
    throw new Error(`Unknown scenario ref: ${ref}.`);
  }
  return node;
}

function resolveImpactPayload(
  payload: Record<string, unknown>,
  refs: Map<string, GraphNode>,
  changeRequestId: string,
): ImpactAnalysisPayloadInput {
  return {
    origin: "ai",
    authority: "proposed",
    status: "proposed",
    change_request: changeRequestId,
    affected_objectives: resolveRefArray(payload.affected_objectives, refs, "affected_objectives"),
    affected_milestones: resolveRefArray(payload.affected_milestones, refs, "affected_milestones"),
    affected_artifacts: stringArray(payload.affected_artifacts, "affected_artifacts"),
    preserved_constraints: stringArray(payload.preserved_constraints, "preserved_constraints"),
    possibly_invalidated_acceptance: stringArray(
      payload.possibly_invalidated_acceptance,
      "possibly_invalidated_acceptance",
    ),
    recommended_action: payload.recommended_action,
    risk: payload.risk,
    ...(payload.blast_radius ? { blast_radius: payload.blast_radius } : {}),
    affected_systems: stringArray(payload.affected_systems, "affected_systems"),
    unknowns: stringArray(payload.unknowns, "unknowns"),
    ...(payload.recommended_strategy ? { recommended_strategy: payload.recommended_strategy } : {}),
    ...(payload.first_slice ? { first_slice: payload.first_slice } : {}),
    regression_constraints: stringArray(payload.regression_constraints, "regression_constraints"),
  } as ImpactAnalysisPayloadInput;
}

function resolveRefArray(
  value: unknown,
  refs: Map<string, GraphNode>,
  fieldName: string,
): string[] {
  return stringArray(value, fieldName).map((ref) => requireRef(refs, ref).id);
}

function stringArray(value: unknown, fieldName: string): string[] {
  if (!Array.isArray(value) || value.some((item) => typeof item !== "string")) {
    throw new Error(`${fieldName} must be a string array.`);
  }
  return value;
}
