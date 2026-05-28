import { readFile } from "node:fs/promises";
import path from "node:path";
import { createTempDir } from "../helpers/sophia_workspace.js";
import { describe, expect, it } from "vitest";
import { GraphStore } from "../../src/graph/core/store.js";

describe("GraphStore", () => {
  it("creates nodes, writes artifacts, and appends edges", async () => {
    const root = await createTempDir("sophia-graph-");
    const store = new GraphStore(root);

    const goal = await store.createNode({
      type: "GoalNode",
      createdFrom: null,
      actionUsed: "start",
      goal: "Test goal",
      summary: "Test goal",
      artifacts: ["content.md"],
    });
    await store.writeArtifact(goal, "content.md", "Test goal\n");

    const pseudo = await store.createNode({
      type: "PseudocodeNode",
      createdFrom: goal.id,
      actionUsed: "add_pseudo",
      summary: "Pseudo",
      artifacts: ["content.pseudo"],
    });
    await store.appendEdge({ from: goal.id, to: pseudo.id, type: "designs_solution" });

    await expect(store.readNode(goal.id)).resolves.toMatchObject({ id: goal.id, type: "GoalNode" });
    await expect(store.readArtifact(goal, "content.md")).resolves.toBe("Test goal\n");

    const edges = JSON.parse(
      await readFile(path.join(root, "sophia-runs/graph", "edges.json"), "utf8"),
    ) as unknown;
    expect(edges).toEqual([{ from: goal.id, to: pseudo.id, type: "designs_solution" }]);
  });

  it("rejects unsafe artifact paths", async () => {
    const root = await createTempDir("sophia-graph-");
    const store = new GraphStore(root);
    const node = await store.createNode({
      type: "GoalNode",
      createdFrom: null,
      actionUsed: "start",
      summary: "Test",
    });

    await expect(store.writeArtifact(node, "../escape.txt", "nope")).rejects.toThrow(
      "Unsafe artifact path",
    );
  });

  it("allocates unique node ids for concurrent writers", async () => {
    const root = await createTempDir("sophia-graph-");
    const store = new GraphStore(root);

    const nodes = await Promise.all(
      Array.from({ length: 5 }, (_, index) =>
        store.createNode({
          type: "GoalNode",
          createdFrom: null,
          actionUsed: "start",
          summary: `Goal ${index}`,
        }),
      ),
    );

    expect(new Set(nodes.map((node) => node.id)).size).toBe(5);
    expect(nodes.map((node) => node.id).sort()).toEqual([
      "N0001",
      "N0002",
      "N0003",
      "N0004",
      "N0005",
    ]);
  });

  it("preserves concurrent edge appends", async () => {
    const root = await createTempDir("sophia-graph-");
    const store = new GraphStore(root);
    const nodes = await Promise.all(
      Array.from({ length: 10 }, (_, index) =>
        store.createNode({
          type: "GoalNode",
          createdFrom: null,
          actionUsed: "start",
          summary: `Goal ${index}`,
        }),
      ),
    );

    await Promise.all(
      nodes.slice(1).map((node) =>
        store.appendEdge({
          from: nodes[0]!.id,
          to: node.id,
          type: "designs_solution",
        }),
      ),
    );

    const edges = await store.listEdges();
    expect(edges).toHaveLength(9);
    expect(new Set(edges.map((edge) => edge.to)).size).toBe(9);
  });

  it("supports append-only selection and materialization nodes", async () => {
    const root = await createTempDir("sophia-graph-");
    const store = new GraphStore(root);
    const code = await store.createNode({
      type: "CodeNode",
      createdFrom: null,
      actionUsed: "implement_design",
      summary: "Code",
    });
    const selection = await store.createNode({
      type: "SelectionNode",
      createdFrom: code.id,
      actionUsed: "select_code",
      summary: "Selected",
    });
    const materialized = await store.createNode({
      type: "MaterializeNode",
      createdFrom: code.id,
      actionUsed: "materialize_code",
      summary: "Materialized",
    });

    await expect(store.readNode(selection.id)).resolves.toMatchObject({ type: "SelectionNode" });
    await expect(store.readNode(materialized.id)).resolves.toMatchObject({
      type: "MaterializeNode",
    });
    await expect(store.readNode(code.id)).resolves.toMatchObject({ status: "active" });
  });
});
