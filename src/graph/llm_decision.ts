import { z } from "zod";
import { generateOllamaJson } from "../llm/client.js";
import { renderPromptTemplate } from "../llm/prompt_templates.js";
import { countBy } from "../util/strings.js";
import { buildDecisionActionBaseline } from "./decision_baseline.js";
import { GraphDecisionSchema, type DecisionAction, type GraphDecision } from "./decision_types.js";
import type { GraphEdge } from "./edges.js";
import type { GraphNode } from "./nodes.js";
import type { GraphStore } from "./store.js";
import { buildFocusedGraphContext, summarizeCurrentNode } from "./llm_decision_context.js";
import { buildDecisionScaffold } from "./llm_decision_scaffold.js";

export interface LlmDecisionResult {
  prompt: string;
  rawResponse: string;
  decision: GraphDecision;
  baseline: GraphDecision;
}

const DecisionJsonSchema = GraphDecisionSchema.extend({
  rationale: z.string().optional(),
  self_check: z
    .object({
      selected_action_is_allowed: z.boolean(),
      based_only_on_visible_graph_state: z.boolean(),
      no_pseudocode_or_code_generated: z.boolean(),
      no_hidden_answers_or_fixture_outputs: z.boolean(),
    })
    .strict(),
}).strict();

type DecisionJsonOutput = z.infer<typeof DecisionJsonSchema>;

export async function decideNextActionWithOllama(options: {
  store: GraphStore;
  currentNode: GraphNode;
  model: string;
}): Promise<LlmDecisionResult> {
  const nodes = await options.store.listNodes();
  const edges = await options.store.listEdges();
  const baseline = await buildDecisionActionBaseline(options.store, options.currentNode);
  const prompt = await buildLlmDecisionPrompt({
    store: options.store,
    currentNode: options.currentNode,
    nodes,
    edges,
    baseline,
  });

  const result = await generateOllamaJson({
    model: options.model,
    prompt,
    operation: "LLM decision",
    schema: DecisionJsonSchema,
    temperature: 0.1,
    topP: 0.8,
    validate: (output) => validateLlmDecision(output, baseline, options.currentNode.id),
  });
  return {
    prompt,
    rawResponse: result.rawResponse,
    decision: toGraphDecision(result.output),
    baseline,
  };
}

export async function buildLlmDecisionPrompt(options: {
  store: GraphStore;
  currentNode: GraphNode;
  nodes: GraphNode[];
  edges: GraphEdge[];
  baseline: GraphDecision;
}): Promise<string> {
  const currentSummary = await summarizeCurrentNode(options.store, options.currentNode);
  const focus = await buildFocusedGraphContext({
    store: options.store,
    currentNode: options.currentNode,
    nodes: options.nodes,
    edges: options.edges,
  });
  const decisionScaffold = buildDecisionScaffold(options.baseline);
  const allowedActions = decisionScaffold.map((candidate) => candidate.action);

  return renderPromptTemplate("graph/llm_decision.md", {
    allowed_actions: JSON.stringify(allowedActions),
    decision_scaffold: JSON.stringify(decisionScaffold, null, 2),
    current_node: JSON.stringify(currentSummary, null, 2),
    focused_graph_context: JSON.stringify(
      {
        node_counts: countBy(options.nodes, (node) => node.type),
        ancestry: focus.ancestry,
        adjacent_edges: focus.adjacent_edges,
        child_results: focus.child_results,
      },
      null,
      2,
    ),
    baseline: JSON.stringify(options.baseline, null, 2),
    current_node_id: options.currentNode.id,
  });
}

function validateLlmDecision(
  decision: DecisionJsonOutput,
  baseline: GraphDecision,
  currentNodeId: string,
): DecisionJsonOutput {
  if (decision.current_node !== currentNodeId) {
    throw new Error(`current_node must be ${currentNodeId}.`);
  }
  const allowed = new Set<DecisionAction>(
    baseline.candidate_actions.map((candidate) => candidate.action),
  );
  if (!allowed.has(decision.selected_action)) {
    throw new Error(
      `selected_action ${decision.selected_action} is not in allowed actions: ${[...allowed].join(", ")}.`,
    );
  }
  if (decision.candidate_actions.length === 0) {
    throw new Error("candidate_actions must not be empty.");
  }
  for (const candidate of decision.candidate_actions) {
    if (!allowed.has(candidate.action)) {
      throw new Error(`candidate action ${candidate.action} is not allowed for this node.`);
    }
  }
  if (decision.confidence < 0 || decision.confidence > 1) {
    throw new Error("confidence must be between 0 and 1.");
  }
  for (const [key, value] of Object.entries(decision.self_check)) {
    if (value !== true) {
      throw new Error(`LLM decision self_check failed: ${key}.`);
    }
  }
  return decision;
}

function toGraphDecision(decision: DecisionJsonOutput): GraphDecision {
  return {
    current_node: decision.current_node,
    state_assessment: decision.state_assessment,
    candidate_actions: decision.candidate_actions,
    selected_action: decision.selected_action,
    confidence: decision.confidence,
  };
}
