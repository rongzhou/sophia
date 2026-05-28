import type { Command } from "commander";
import {
  parseEnumOption,
  parseJsonArrayOption,
  parseJsonObjectOption,
  printJson,
  parseStringArrayOption,
} from "./cli_utils.js";
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
  invalidateDecomposition,
  redecomposeObjective,
  type ImpactAnalysisPayloadInput,
  type MilestonePayloadInput,
} from "../graph/goal/workflow.js";
import {
  loadGoalGraphScenario,
  materializeGoalGraphScenario,
} from "../graph/goal/scenarios.js";
import { GraphStore } from "../graph/core/store.js";

export function registerGraphGoalCommands(graph: Command): void {
  graph
    .command("start")
    .argument("<goal>")
    .description("Create a GoalNode")
    .action(async (goal: string) => {
      const store = new GraphStore(process.cwd());
      const node = await store.createNode({
        type: "GoalNode",
        createdFrom: null,
        actionUsed: "start",
        goal,
        summary: goal,
        artifacts: ["content.md"],
        tags: ["goal"],
      });
      await store.writeArtifact(node, "content.md", `${goal}\n`);
      printJson(node);
    });

  const objective = graph.command("objective").description("Manage goal objective nodes");

  objective
    .command("create")
    .requiredOption("--title <title>", "Objective title")
    .requiredOption("--description <description>", "Objective description")
    .option("--constraints <json>", "JSON string array", "[]")
    .option("--acceptance <json>", "JSON string array", "[]")
    .description("Create a human authoritative ObjectiveNode")
    .action(
      async (options: {
        title: string;
        description: string;
        constraints: string;
        acceptance: string;
      }) => {
        const store = new GraphStore(process.cwd());
        const node = await createObjectiveNode({
          store,
          payload: {
            origin: "human",
            authority: "authoritative",
            status: "open",
            title: options.title,
            description: options.description,
            constraints: parseStringArrayOption(options.constraints, "--constraints"),
            acceptance: parseStringArrayOption(options.acceptance, "--acceptance"),
            parent_objective: null,
          },
        });
        printJson(node);
      },
    );

  objective
    .command("decompose")
    .argument("<parent-objective>")
    .requiredOption("--decomposition-id <id>", "Stable decomposition id")
    .requiredOption("--objectives <json>", "JSON array of child objective payloads")
    .option("--milestones <json>", "JSON array of milestone payloads", "[]")
    .description("Create proposed AI child objectives and optional milestones")
    .action(
      async (
        parentObjectiveId: string,
        options: { decompositionId: string; objectives: string; milestones: string },
      ) => {
        const store = new GraphStore(process.cwd());
        const parentObjective = await store.readNode(parentObjectiveId);
        const result = await decomposeObjective({
          store,
          parentObjective,
          decompositionId: options.decompositionId,
          objectives: parseJsonArrayOption(options.objectives, "--objectives"),
          milestones: parseJsonArrayOption(options.milestones, "--milestones"),
        });
        printJson(result);
      },
    );

  objective
    .command("accept-decomposition")
    .argument("<parent-objective>")
    .requiredOption("--decomposition-id <id>", "Decomposition id to accept")
    .description("Accept proposed child objectives for a parent objective")
    .action(async (parentObjectiveId: string, options: { decompositionId: string }) => {
      const store = new GraphStore(process.cwd());
      const parentObjective = await store.readNode(parentObjectiveId);
      const result = await acceptObjectiveDecomposition({
        store,
        parentObjective,
        decompositionId: options.decompositionId,
      });
      printJson(result);
    });

  objective
    .command("invalidate-decomposition")
    .argument("<parent-objective>")
    .requiredOption("--decomposition-id <id>", "Decomposition id to invalidate")
    .requiredOption("--reason <reason>", "Reason for preserving but excluding this split")
    .description("Invalidate a wrong decomposition while preserving its history")
    .action(
      async (parentObjectiveId: string, options: { decompositionId: string; reason: string }) => {
        const store = new GraphStore(process.cwd());
        const parentObjective = await store.readNode(parentObjectiveId);
        const result = await invalidateDecomposition({
          store,
          parentObjective,
          decompositionId: options.decompositionId,
          reason: options.reason,
        });
        printJson(result);
      },
    );

  objective
    .command("redecompose")
    .argument("<parent-objective>")
    .requiredOption("--previous-decomposition-id <id>", "Invalidated decomposition id")
    .requiredOption("--decomposition-id <id>", "Replacement decomposition id")
    .requiredOption("--reason <reason>", "Reason for replacement")
    .requiredOption("--objectives <json>", "JSON array of replacement child objective payloads")
    .option("--milestones <json>", "JSON array of replacement milestone payloads", "[]")
    .description("Invalidate one split and create a replacement split")
    .action(
      async (
        parentObjectiveId: string,
        options: {
          previousDecompositionId: string;
          decompositionId: string;
          reason: string;
          objectives: string;
          milestones: string;
        },
      ) => {
        const store = new GraphStore(process.cwd());
        const parentObjective = await store.readNode(parentObjectiveId);
        const result = await redecomposeObjective({
          store,
          parentObjective,
          previousDecompositionId: options.previousDecompositionId,
          decompositionId: options.decompositionId,
          reason: options.reason,
          objectives: parseJsonArrayOption(options.objectives, "--objectives"),
          milestones: parseJsonArrayOption(options.milestones, "--milestones"),
        });
        printJson(result);
      },
    );

  const milestone = graph.command("milestone").description("Manage goal milestone nodes");

  milestone
    .command("create")
    .argument("<parent-objective>")
    .requiredOption("--name <name>", "Milestone name")
    .option("--scope <json>", "JSON string array", "[]")
    .option("--out-of-scope <json>", "JSON string array", "[]")
    .option("--acceptance <json>", "JSON string array", "[]")
    .description("Create a human authoritative MilestoneNode")
    .action(
      async (
        parentObjectiveId: string,
        options: { name: string; scope: string; outOfScope: string; acceptance: string },
      ) => {
        const store = new GraphStore(process.cwd());
        const parentObjective = await store.readNode(parentObjectiveId);
        const payload: MilestonePayloadInput = {
          origin: "human",
          authority: "authoritative",
          status: "planned",
          name: options.name,
          scope: parseStringArrayOption(options.scope, "--scope"),
          out_of_scope: parseStringArrayOption(options.outOfScope, "--out-of-scope"),
          acceptance: parseStringArrayOption(options.acceptance, "--acceptance"),
          parent_objective: parentObjective.id,
        };
        const node = await createMilestoneNode({
          store,
          createdFrom: parentObjective,
          payload,
        });
        printJson(node);
      },
    );

  milestone
    .command("accept")
    .argument("<milestone-node>")
    .description("Accept a proposed or planned MilestoneNode")
    .action(async (milestoneNodeId: string) => {
      const store = new GraphStore(process.cwd());
      const milestoneNode = await store.readNode(milestoneNodeId);
      printJson(await acceptMilestone({ store, milestoneNode }));
    });

  milestone
    .command("activate")
    .argument("<milestone-node>")
    .description("Set an accepted or planned milestone as active")
    .action(async (milestoneNodeId: string) => {
      const store = new GraphStore(process.cwd());
      const milestoneNode = await store.readNode(milestoneNodeId);
      printJson(await activateMilestone({ store, milestoneNode }));
    });

  const change = graph.command("change").description("Manage human change requests");

  change
    .command("record")
    .argument("<source-node>")
    .requiredOption(
      "--kind <kind>",
      "new_requirement | correction | preference | rejection | constraint_change",
    )
    .requiredOption("--request <request>", "Human change request")
    .option("--applies-to <json>", "JSON string array", "[]")
    .option("--priority <priority>", "must | should | could", "must")
    .description("Record a human authoritative ChangeRequestNode")
    .action(
      async (
        sourceNodeId: string,
        options: { kind: string; request: string; appliesTo: string; priority: string },
      ) => {
        const store = new GraphStore(process.cwd());
        const sourceNode = await store.readNode(sourceNodeId);
        const node = await createChangeRequestNode({
          store,
          createdFrom: sourceNode,
          payload: {
            origin: "human",
            authority: "authoritative",
            status: "proposed",
            kind: parseEnumOption(options.kind, "--kind", [
              "new_requirement",
              "correction",
              "preference",
              "rejection",
              "constraint_change",
            ]),
            request: options.request,
            applies_to: parseStringArrayOption(options.appliesTo, "--applies-to"),
            priority: parseEnumOption(options.priority, "--priority", ["must", "should", "could"]),
          },
        });
        printJson(node);
      },
    );

  change
    .command("analyze")
    .argument("<change-node>")
    .requiredOption("--payload <json>", "Impact analysis fields as JSON object")
    .description("Record an AI proposed ImpactAnalysisNode for a change request")
    .action(async (changeNodeId: string, options: { payload: string }) => {
      const store = new GraphStore(process.cwd());
      const changeNode = await store.readNode(changeNodeId);
      const payload = parseJsonObjectOption(options.payload, "--payload");
      const node = await createImpactAnalysisNode({
        store,
        createdFrom: changeNode,
        payload: {
          origin: "ai",
          authority: "proposed",
          status: "proposed",
          change_request: changeNode.id,
          affected_objectives: [],
          affected_milestones: [],
          affected_artifacts: [],
          preserved_constraints: [],
          possibly_invalidated_acceptance: [],
          affected_systems: [],
          unknowns: [],
          regression_constraints: [],
          ...payload,
        } as unknown as ImpactAnalysisPayloadInput,
      });
      printJson(node);
    });

  change
    .command("accept")
    .argument("<change-node>")
    .argument("<impact-analysis-node>")
    .description("Accept a change request and its matching impact analysis")
    .action(async (changeNodeId: string, impactNodeId: string) => {
      const store = new GraphStore(process.cwd());
      const changeRequestNode = await store.readNode(changeNodeId);
      const impactAnalysisNode = await store.readNode(impactNodeId);
      printJson(await acceptChangeRequest({ store, changeRequestNode, impactAnalysisNode }));
    });

  const acceptance = graph.command("acceptance").description("Record human acceptance decisions");

  acceptance
    .command("record")
    .argument("<source-node>")
    .requiredOption("--target <target-node>", "Accepted or rejected target node")
    .requiredOption("--decision <decision>", "accepted | rejected | accepted_with_changes")
    .option("--notes <notes>", "Human notes", "")
    .option(
      "--creates-change-request <node>",
      "Optional ChangeRequestNode created by this decision",
    )
    .description("Record a human authoritative AcceptanceNode")
    .action(
      async (
        sourceNodeId: string,
        options: {
          target: string;
          decision: string;
          notes: string;
          createsChangeRequest?: string;
        },
      ) => {
        const store = new GraphStore(process.cwd());
        const sourceNode = await store.readNode(sourceNodeId);
        const node = await createAcceptanceNode({
          store,
          createdFrom: sourceNode,
          payload: {
            origin: "human",
            authority: "authoritative",
            status: "recorded",
            target: options.target,
            decision: parseEnumOption(options.decision, "--decision", [
              "accepted",
              "rejected",
              "accepted_with_changes",
            ]),
            notes: options.notes,
            creates_change_request: options.createsChangeRequest ?? null,
          },
        });
        printJson(node);
      },
    );

  const scenario = graph
    .command("scenario")
    .description("Materialize goal-graph validation scenarios");

  scenario
    .command("materialize")
    .argument("<scenario-json>")
    .description("Create graph nodes for a 5.6 manual goal-graph validation scenario")
    .action(async (scenarioPath: string) => {
      const store = new GraphStore(process.cwd());
      const loaded = await loadGoalGraphScenario(scenarioPath);
      const record = await materializeGoalGraphScenario({ store, scenario: loaded });
      printJson(record);
    });
}
