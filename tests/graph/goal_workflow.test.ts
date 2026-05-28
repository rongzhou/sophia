import { describe, expect, it } from "vitest";
import { createTempDir } from "../helpers/sophia_workspace.js";
import {
  acceptChangeRequest,
  acceptMilestone,
  acceptObjectiveDecomposition,
  activateMilestone,
  createAcceptanceNode,
  createChangeRequestNode,
  createImpactAnalysisNode,
  createMilestoneNode,
  createObjectiveNode,
  decomposeObjective,
  readMilestonePayload,
  readObjectivePayload,
  redecomposeObjective,
} from "../../src/graph/goal/workflow.js";
import { GraphStore } from "../../src/graph/core/store.js";

describe("goal workflow nodes", () => {
  it("creates human objective nodes with payload artifacts and graph edges", async () => {
    const store = await tempStore();
    const objective = await createObjectiveNode({
      store,
      payload: {
        origin: "human",
        authority: "authoritative",
        status: "open",
        title: "Build a playable prototype",
        description: "Create the first playable slice.",
        constraints: ["Keep behavior deterministic"],
        acceptance: ["Player can complete one level"],
        parent_objective: null,
      },
    });
    const milestone = await createMilestoneNode({
      store,
      createdFrom: objective,
      payload: {
        origin: "ai",
        authority: "proposed",
        status: "planned",
        name: "vertical_slice",
        scope: ["movement", "collision"],
        out_of_scope: ["sound"],
        acceptance: ["Movement works in one test level"],
      },
    });

    expect(objective.type).toBe("ObjectiveNode");
    expect(milestone.type).toBe("MilestoneNode");
    expect(objective.artifacts).toEqual(["payload.json"]);
    expect(await store.readArtifactJson(objective, "payload.json")).toMatchObject({
      origin: "human",
      authority: "authoritative",
      title: "Build a playable prototype",
    });
    expect(await store.listEdges()).toEqual([
      { from: objective.id, to: milestone.id, type: "defines_milestone" },
    ]);
  });

  it("rejects authoritative AI-derived objective and milestone nodes", async () => {
    const store = await tempStore();

    await expect(
      createObjectiveNode({
        store,
        payload: {
          origin: "ai",
          authority: "authoritative",
          status: "open",
          title: "AI split",
          description: "A proposed split.",
          constraints: [],
          acceptance: [],
          parent_objective: null,
        },
      }),
    ).rejects.toThrow("AI-derived goal nodes cannot be authoritative");

    await expect(
      createMilestoneNode({
        store,
        payload: {
          origin: "ai",
          authority: "authoritative",
          status: "planned",
          name: "ai_stage",
          scope: [],
          out_of_scope: [],
          acceptance: [],
        },
      }),
    ).rejects.toThrow("AI-derived goal nodes cannot be authoritative");
  });

  it("creates change request, impact analysis, and acceptance nodes with constrained origins", async () => {
    const store = await tempStore();
    const objective = await createObjectiveNode({
      store,
      payload: {
        origin: "human",
        authority: "authoritative",
        status: "open",
        title: "Todo workflow",
        description: "Build todo creation and listing.",
        constraints: [],
        acceptance: ["Title behavior remains stable"],
        parent_objective: null,
      },
    });
    const change = await createChangeRequestNode({
      store,
      createdFrom: objective,
      payload: {
        origin: "human",
        authority: "authoritative",
        status: "proposed",
        kind: "new_requirement",
        request: "Add priority labels.",
        applies_to: ["todo item"],
        priority: "must",
      },
    });
    const impact = await createImpactAnalysisNode({
      store,
      createdFrom: change,
      payload: {
        origin: "ai",
        authority: "proposed",
        status: "proposed",
        change_request: change.id,
        affected_objectives: [objective.id],
        affected_milestones: [],
        affected_artifacts: ["TodoItem"],
        preserved_constraints: ["Title behavior remains stable"],
        possibly_invalidated_acceptance: [],
        recommended_action: "decompose_objective",
        risk: "medium",
        affected_systems: ["data_model"],
        unknowns: [],
        regression_constraints: ["Existing title-only todos still list"],
      },
    });
    const acceptance = await createAcceptanceNode({
      store,
      createdFrom: impact,
      payload: {
        origin: "human",
        authority: "authoritative",
        status: "recorded",
        target: impact.id,
        decision: "accepted",
        notes: "Proceed with the priority split.",
        creates_change_request: null,
      },
    });

    expect(change.type).toBe("ChangeRequestNode");
    expect(impact.type).toBe("ImpactAnalysisNode");
    expect(acceptance.type).toBe("AcceptanceNode");
    expect(await store.readArtifactJson(change, "payload.json")).toMatchObject({
      origin: "human",
      status: "proposed",
    });
    expect(await store.readArtifactJson(impact, "payload.json")).toMatchObject({
      origin: "ai",
      authority: "proposed",
      change_request: change.id,
    });
    expect(await store.listEdges()).toEqual([
      { from: objective.id, to: change.id, type: "requests_change" },
      { from: change.id, to: impact.id, type: "analyzes_change" },
      { from: impact.id, to: acceptance.id, type: "records_acceptance" },
    ]);
  });

  it("requires impact analysis to be created from a ChangeRequestNode", async () => {
    const store = await tempStore();
    const objective = await createObjectiveNode({
      store,
      payload: {
        origin: "human",
        authority: "authoritative",
        status: "open",
        title: "Root",
        description: "Root objective.",
        constraints: [],
        acceptance: [],
        parent_objective: null,
      },
    });

    await expect(
      createImpactAnalysisNode({
        store,
        createdFrom: objective,
        payload: {
          origin: "ai",
          authority: "proposed",
          status: "proposed",
          change_request: objective.id,
          affected_objectives: [],
          affected_milestones: [],
          affected_artifacts: [],
          preserved_constraints: [],
          possibly_invalidated_acceptance: [],
          recommended_action: "defer_change",
          risk: "low",
          affected_systems: [],
          unknowns: [],
          regression_constraints: [],
        },
      }),
    ).rejects.toThrow("Expected ChangeRequestNode");
  });

  it("decomposes, accepts, invalidates, and redecomposes objectives without deleting history", async () => {
    const store = await tempStore();
    const parent = await createObjectiveNode({
      store,
      payload: {
        origin: "human",
        authority: "authoritative",
        status: "open",
        title: "Todo objective",
        description: "Build todo creation, listing, and priority.",
        constraints: [],
        acceptance: [],
        parent_objective: null,
      },
    });

    const first = await decomposeObjective({
      store,
      parentObjective: parent,
      decompositionId: "D0001",
      objectives: [
        {
          status: "open",
          title: "Render priority",
          description: "Treat priority as a display detail.",
          constraints: [],
          acceptance: [],
        },
      ],
    });
    await acceptObjectiveDecomposition({
      store,
      parentObjective: parent,
      decompositionId: "D0001",
    });
    const acceptedFirstPayload = await readObjectivePayload(store, first.objective_nodes[0]!);
    expect(acceptedFirstPayload).toMatchObject({
      authority: "derived",
      decomposition_id: "D0001",
      decomposition_status: "accepted",
    });

    const second = await redecomposeObjective({
      store,
      parentObjective: parent,
      previousDecompositionId: "D0001",
      decompositionId: "D0002",
      reason: "Priority must be a data field, not only display.",
      objectives: [
        {
          status: "open",
          title: "Model priority",
          description: "Represent priority in the todo data model.",
          constraints: [],
          acceptance: [],
        },
        {
          status: "open",
          title: "Validate priority",
          description: "Restrict priority to allowed values.",
          constraints: [],
          acceptance: [],
        },
      ],
    });

    const invalidatedFirstPayload = await readObjectivePayload(store, first.objective_nodes[0]!);
    const secondPayload = await readObjectivePayload(store, second.objective_nodes[0]!);
    expect(invalidatedFirstPayload).toMatchObject({
      status: "superseded",
      decomposition_status: "invalidated",
    });
    expect(secondPayload).toMatchObject({
      authority: "proposed",
      decomposition_id: "D0002",
      decomposition_status: "proposed",
      parent_objective: parent.id,
    });
    expect(await store.readNode(first.objective_nodes[0]!.id)).toMatchObject({
      status: "superseded",
    });
    expect(await store.readNode(second.objective_nodes[0]!.id)).toMatchObject({
      status: "active",
    });
    expect(await store.listNodes()).toHaveLength(4);
  });

  it("accepts and activates milestones only after proposal authority is removed", async () => {
    const store = await tempStore();
    const parent = await createObjectiveNode({
      store,
      payload: {
        origin: "human",
        authority: "authoritative",
        status: "open",
        title: "Game objective",
        description: "Build a small movement slice.",
        constraints: [],
        acceptance: [],
        parent_objective: null,
      },
    });
    const milestone = await createMilestoneNode({
      store,
      createdFrom: parent,
      payload: {
        origin: "ai",
        authority: "proposed",
        status: "planned",
        name: "movement_slice",
        scope: ["walk"],
        out_of_scope: ["combat"],
        acceptance: [],
        parent_objective: parent.id,
        decomposition_id: "D0001",
        decomposition_status: "proposed",
      },
    });

    await expect(activateMilestone({ store, milestoneNode: milestone })).rejects.toThrow(
      "must be accepted before activation",
    );
    const accepted = await acceptMilestone({ store, milestoneNode: milestone });
    expect(await readMilestonePayload(store, accepted)).toMatchObject({
      authority: "derived",
      status: "accepted",
      decomposition_status: "accepted",
    });
    const activated = await activateMilestone({ store, milestoneNode: accepted });
    expect(await readMilestonePayload(store, activated)).toMatchObject({
      status: "active",
    });
  });

  it("accepts change requests only with matching impact analysis", async () => {
    const store = await tempStore();
    const objective = await createObjectiveNode({
      store,
      payload: {
        origin: "human",
        authority: "authoritative",
        status: "open",
        title: "Mount objective",
        description: "Model player movement.",
        constraints: [],
        acceptance: [],
        parent_objective: null,
      },
    });
    const change = await createChangeRequestNode({
      store,
      createdFrom: objective,
      payload: {
        origin: "human",
        authority: "authoritative",
        status: "proposed",
        kind: "new_requirement",
        request: "Player can mount.",
        applies_to: ["player"],
        priority: "must",
      },
    });
    const impact = await createImpactAnalysisNode({
      store,
      createdFrom: change,
      payload: {
        origin: "ai",
        authority: "proposed",
        status: "proposed",
        change_request: change.id,
        affected_objectives: [objective.id],
        affected_milestones: [],
        affected_artifacts: ["Player"],
        preserved_constraints: [],
        possibly_invalidated_acceptance: [],
        recommended_action: "plan_vertical_slice",
        risk: "high",
        blast_radius: "subsystem",
        affected_systems: ["movement", "state"],
        unknowns: [],
        recommended_strategy: "vertical_slice",
        regression_constraints: ["Unmounted movement still works"],
      },
    });

    const accepted = await acceptChangeRequest({
      store,
      changeRequestNode: change,
      impactAnalysisNode: impact,
    });

    expect(await store.readArtifactJson(accepted.change_request, "payload.json")).toMatchObject({
      status: "accepted",
    });
    expect(await store.readArtifactJson(accepted.impact_analysis, "payload.json")).toMatchObject({
      status: "accepted",
    });
  });
});

async function tempStore(): Promise<GraphStore> {
  return new GraphStore(await createTempDir("sophia-goal-workflow-"));
}
