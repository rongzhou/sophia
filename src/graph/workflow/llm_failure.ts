import { isLlmCallError } from "../../llm/errors.js";
import type { GraphEdgeType } from "../core/nodes.js";
import { createRawLlmFailureNode } from "./llm_node.js";
import type { GraphNode, NodeAction } from "../core/nodes.js";
import type { GraphStore } from "../core/store.js";

export async function captureRawLlmFailure<T>(options: {
  store: GraphStore;
  createdFrom: GraphNode;
  actionUsed: NodeAction;
  edgeType: GraphEdgeType;
  tags: string[];
  model: string;
  goal?: string | undefined;
  call: () => Promise<T>;
}): Promise<T> {
  try {
    return await options.call();
  } catch (error) {
    if (isLlmCallError(error)) {
      await createRawLlmFailureNode({
        store: options.store,
        createdFrom: options.createdFrom,
        actionUsed: options.actionUsed,
        edgeType: options.edgeType,
        tags: options.tags,
        model: options.model,
        error,
        goal: options.goal,
      });
    }
    throw error;
  }
}
