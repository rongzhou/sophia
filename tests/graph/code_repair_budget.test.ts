import { describe, expect, it } from "vitest";
import { createTempDir } from "../helpers/sophia_workspace.js";
import {
  assertCodeRepairBudgetAvailable,
  countCodeRepairAttemptsForCodeNode,
} from "../../src/graph/llm_node_workflow.js";
import type { GraphNode } from "../../src/graph/nodes.js";
import { GraphStore } from "../../src/graph/store.js";

describe("code repair budget", () => {
  it("counts repair CodeNodes derived from checks of the same CodeNode", async () => {
    const store = await tempStore();
    const code = await codeNode(store);
    const check = await checkNode(store, code);
    await repairNode(store, check);
    await repairNode(store, check);

    await expect(countCodeRepairAttemptsForCodeNode(store, code)).resolves.toBe(2);
  });

  it("rejects repair when the CodeNode budget is exhausted", async () => {
    const store = await tempStore();
    const code = await codeNode(store);
    const check = await checkNode(store, code);
    await repairNode(store, check);

    await expect(
      assertCodeRepairBudgetAvailable({ store, codeNode: code, maxRepairs: 1 }),
    ).rejects.toThrow("repair budget exhausted");
  });

  it("does not count repairs from another CodeNode branch", async () => {
    const store = await tempStore();
    const code = await codeNode(store);
    const otherCode = await codeNode(store);
    const otherCheck = await checkNode(store, otherCode);
    await repairNode(store, otherCheck);

    await expect(
      assertCodeRepairBudgetAvailable({ store, codeNode: code, maxRepairs: 1 }),
    ).resolves.toBe(0);
  });
});

async function tempStore(): Promise<GraphStore> {
  return new GraphStore(await createTempDir("sophia-repair-budget-"));
}

async function codeNode(store: GraphStore): Promise<GraphNode> {
  return store.createNode({
    type: "CodeNode",
    createdFrom: null,
    action_used: "implement_design",
    summary: "Code",
  });
}

async function checkNode(store: GraphStore, code: GraphNode): Promise<GraphNode> {
  const node = await store.createNode({
    type: "CheckResultNode",
    status: "failed",
    createdFrom: code.id,
    action_used: "check_code",
    summary: "failed",
    artifacts: ["result.json"],
  });
  await store.writeArtifact(
    node,
    "result.json",
    `${JSON.stringify({ ok: false, diagnostics: [{ code: "CHECK-BODY-004", severity: "error", problem: "bad" }] }, null, 2)}\n`,
  );
  await store.appendEdge({ from: code.id, to: node.id, type: "checks" });
  return node;
}

async function repairNode(store: GraphStore, check: GraphNode): Promise<GraphNode> {
  const node = await store.createNode({
    type: "CodeNode",
    createdFrom: check.id,
    action_used: "repair_code",
    summary: "repaired",
  });
  await store.appendEdge({ from: check.id, to: node.id, type: "repairs" });
  return node;
}
