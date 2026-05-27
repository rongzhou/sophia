import { isLlmCallError } from "../llm/errors.js";
import type { GraphEdge } from "./edges.js";
import { createRawLlmFailureNode } from "./llm_node_workflow.js";
import type { GraphNode } from "./nodes.js";
import type { GraphStore } from "./store.js";

export async function captureRawLlmFailure<T>(options: {
  store: GraphStore;
  createdFrom: GraphNode;
  action_used: string;
  edgeType: GraphEdge["type"];
  tags: string[];
  model: string;
  goal?: string;
  call: () => Promise<T>;
}): Promise<T> {
  try {
    return await options.call();
  } catch (error) {
    if (isLlmCallError(error)) {
      await createRawLlmFailureNode({
        store: options.store,
        createdFrom: options.createdFrom,
        action_used: options.action_used,
        edgeType: options.edgeType,
        tags: options.tags,
        model: options.model,
        error,
        ...(options.goal ? { goal: options.goal } : {}),
      });
    }
    throw error;
  }
}
