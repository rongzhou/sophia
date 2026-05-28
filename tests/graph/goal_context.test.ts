import { describe, expect, it } from "vitest";
import { buildGoalContext } from "../../src/graph/goal/context.js";
import {
  acceptChangeRequest,
  acceptMilestone,
  acceptObjectiveDecomposition,
  activateMilestone,
  createChangeRequestNode,
  createImpactAnalysisNode,
  createObjectiveNode,
  decomposeObjective,
  redecomposeObjective,
} from "../../src/graph/goal/workflow.js";
import { GraphStore } from "../../src/graph/core/store.js";
import { createTempDir } from "../helpers/sophia_workspace.js";

describe("buildGoalContext", () => {
  it("includes authoritative roots and accepted decomposition while excluding invalidated branches", async () => {
    const store = await tempStore();
    const root = await createObjectiveNode({
      store,
      payload: {
        origin: "human",
        authority: "authoritative",
        status: "open",
        title: "Todo workflow",
        description: "Build todo creation, listing, and priority.",
        constraints: ["Preserve title behavior"],
        acceptance: ["Title-only todos still list"],
        parent_objective: null,
      },
    });
    const first = await decomposeObjective({
      store,
      parentObjective: root,
      decompositionId: "D0001",
      objectives: [
        {
          status: "open",
          title: "Render priority",
          description: "Wrongly treats priority as display-only.",
          constraints: [],
          acceptance: [],
        },
      ],
    });
    await acceptObjectiveDecomposition({
      store,
      parentObjective: root,
      decompositionId: "D0001",
    });
    const second = await redecomposeObjective({
      store,
      parentObjective: root,
      previousDecompositionId: "D0001",
      decompositionId: "D0002",
      reason: "Priority is data, not only display.",
      objectives: [
        {
          status: "open",
          title: "Model priority",
          description: "Add priority to the todo model.",
          constraints: [],
          acceptance: ["Priority is stored"],
        },
      ],
    });
    await acceptObjectiveDecomposition({
      store,
      parentObjective: root,
      decompositionId: "D0002",
    });

    const context = await buildGoalContext(store);

    expect(context.objectives.map((objective) => objective.node_id)).toEqual([
      root.id,
      second.objective_nodes[0]!.id,
    ]);
    expect(context.excluded.objectives).toContain(first.objective_nodes[0]!.id);
    expect(context.objectives).toEqual(
      expect.arrayContaining([
        expect.objectContaining({
          title: "Model priority",
          authority: "derived",
          decomposition_id: "D0002",
        }),
      ]),
    );
  });

  it("uses the latest active milestone and exposes out-of-scope constraints", async () => {
    const store = await tempStore();
    const root = await createObjectiveNode({
      store,
      payload: {
        origin: "human",
        authority: "authoritative",
        status: "open",
        title: "Movement",
        description: "Build a movement slice.",
        constraints: [],
        acceptance: [],
        parent_objective: null,
      },
    });
    const decomposition = await decomposeObjective({
      store,
      parentObjective: root,
      decompositionId: "D0001",
      objectives: [],
      milestones: [
        {
          status: "planned",
          name: "movement_slice",
          scope: ["walk", "jump"],
          out_of_scope: ["combat", "network"],
          acceptance: ["Player can move"],
        },
      ],
    });

    let context = await buildGoalContext(store);
    expect(context.active_milestone).toBeNull();
    expect(context.excluded.milestones).toContain(decomposition.milestone_nodes[0]!.id);

    const accepted = await acceptMilestone({
      store,
      milestoneNode: decomposition.milestone_nodes[0]!,
    });
    await activateMilestone({ store, milestoneNode: accepted });

    context = await buildGoalContext(store);
    expect(context.active_milestone).toMatchObject({
      node_id: accepted.id,
      authority: "derived",
      status: "active",
      out_of_scope: ["combat", "network"],
    });
    expect(context.out_of_scope).toEqual(["combat", "network"]);
  });

  it("includes only accepted change requests with accepted impact analyses", async () => {
    const store = await tempStore();
    const root = await createObjectiveNode({
      store,
      payload: {
        origin: "human",
        authority: "authoritative",
        status: "open",
        title: "Mount model",
        description: "Model player movement and mount state.",
        constraints: [],
        acceptance: ["Unmounted movement still works"],
        parent_objective: null,
      },
    });
    const acceptedChange = await createChangeRequestNode({
      store,
      createdFrom: root,
      payload: {
        origin: "human",
        authority: "authoritative",
        status: "proposed",
        kind: "new_requirement",
        request: "Player can mount.",
        applies_to: ["player", "mount"],
        priority: "must",
      },
    });
    const acceptedImpact = await createImpactAnalysisNode({
      store,
      createdFrom: acceptedChange,
      payload: {
        origin: "ai",
        authority: "proposed",
        status: "proposed",
        change_request: acceptedChange.id,
        affected_objectives: [root.id],
        affected_milestones: [],
        affected_artifacts: ["Player", "Mount"],
        preserved_constraints: ["Unmounted movement still works"],
        possibly_invalidated_acceptance: [],
        recommended_action: "plan_vertical_slice",
        risk: "high",
        blast_radius: "subsystem",
        affected_systems: ["movement", "state"],
        unknowns: ["collision rules while mounted"],
        recommended_strategy: "vertical_slice",
        first_slice: {
          scope: ["mount", "move", "dismount"],
          out_of_scope: ["combat", "animation"],
          acceptance: ["Mounted player can move"],
        },
        regression_constraints: ["Unmounted movement still works"],
      },
    });
    await acceptChangeRequest({
      store,
      changeRequestNode: acceptedChange,
      impactAnalysisNode: acceptedImpact,
    });
    const deferredChange = await createChangeRequestNode({
      store,
      createdFrom: root,
      payload: {
        origin: "human",
        authority: "authoritative",
        status: "deferred",
        kind: "preference",
        request: "Add mounted combat.",
        applies_to: ["combat"],
        priority: "could",
      },
    });

    const context = await buildGoalContext(store);

    expect(context.accepted_changes).toHaveLength(1);
    expect(context.accepted_changes[0]).toMatchObject({
      node_id: acceptedChange.id,
      request: "Player can mount.",
      impact_analysis: {
        node_id: acceptedImpact.id,
        blast_radius: "subsystem",
        regression_constraints: ["Unmounted movement still works"],
      },
    });
    expect(context.excluded.change_requests).toContain(deferredChange.id);
    expect(context.out_of_scope).toEqual(["animation", "combat"]);
    expect(context.regression_constraints).toEqual(["Unmounted movement still works"]);
  });
});

async function tempStore(): Promise<GraphStore> {
  return new GraphStore(await createTempDir("sophia-goal-context-"));
}
