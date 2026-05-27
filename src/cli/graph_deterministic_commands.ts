import type { Command } from "commander";
import { setFailedExitIf } from "./cli_utils.js";
import {
  createArtifactDiffNode,
  createAuditNode,
  createCheckResultNode,
  createSelectionNode,
  inferBeforeCodeNodeForDiff,
  readCodeNodeFiles,
  summarizeResultNode,
} from "../graph/code_workflow.js";
import { buildGraphReport } from "../graph/report.js";
import { GraphStore } from "../graph/store.js";

export function registerGraphDeterministicCommands(graph: Command): void {
  graph
    .command("check")
    .argument("<code-node>")
    .description("Run deterministic checks for a CodeNode")
    .action(async (codeNodeId: string) => {
      const store = new GraphStore(process.cwd());
      const codeNode = await store.readNode(codeNodeId);
      if (codeNode.type !== "CodeNode") {
        throw new Error(`Expected CodeNode, got ${codeNode.type}.`);
      }

      const { result } = await createCheckResultNode({
        store,
        codeNode,
        files: await readCodeNodeFiles(store, codeNode),
      });
      console.log(JSON.stringify(result, null, 2));
      setFailedExitIf(!result.ok);
    });

  graph
    .command("audit")
    .argument("<code-node>")
    .description("Audit a CodeNode against its ancestor .pseudo constraints")
    .action(async (codeNodeId: string) => {
      const store = new GraphStore(process.cwd());
      const codeNode = await store.readNode(codeNodeId);
      if (codeNode.type !== "CodeNode") {
        throw new Error(`Expected CodeNode, got ${codeNode.type}.`);
      }
      const { result } = await createAuditNode({
        store,
        codeNode,
        files: await readCodeNodeFiles(store, codeNode),
      });
      console.log(JSON.stringify(result, null, 2));
      setFailedExitIf(!result.ok);
    });

  graph
    .command("diff")
    .argument("<after-code-node>")
    .argument("[before-code-node]")
    .description("Compare a CodeNode with its repaired ancestor or an explicit before CodeNode")
    .action(async (afterCodeNodeId: string, beforeCodeNodeId?: string) => {
      const store = new GraphStore(process.cwd());
      const afterCodeNode = await store.readNode(afterCodeNodeId);
      if (afterCodeNode.type !== "CodeNode") {
        throw new Error(`Expected after node to be CodeNode, got ${afterCodeNode.type}.`);
      }
      const beforeCodeNode = beforeCodeNodeId
        ? await store.readNode(beforeCodeNodeId)
        : await inferBeforeCodeNodeForDiff(store, afterCodeNode);
      if (beforeCodeNode.type !== "CodeNode") {
        throw new Error(`Expected before node to be CodeNode, got ${beforeCodeNode.type}.`);
      }

      const diff = await createArtifactDiffNode({
        store,
        beforeNode: beforeCodeNode,
        afterNode: afterCodeNode,
        beforeFiles: await readCodeNodeFiles(store, beforeCodeNode),
        afterFiles: await readCodeNodeFiles(store, afterCodeNode),
      });
      console.log(JSON.stringify(diff.result, null, 2));
    });

  graph
    .command("verify")
    .argument("<code-node>")
    .description("Run deterministic check, audit, and repair diff when applicable")
    .action(async (codeNodeId: string) => {
      const store = new GraphStore(process.cwd());
      const codeNode = await store.readNode(codeNodeId);
      if (codeNode.type !== "CodeNode") {
        throw new Error(`Expected CodeNode, got ${codeNode.type}.`);
      }
      const files = await readCodeNodeFiles(store, codeNode);
      const beforeNode =
        codeNode.action_used === "repair_code"
          ? await inferBeforeCodeNodeForDiff(store, codeNode)
          : null;
      const diff = beforeNode
        ? await createArtifactDiffNode({
            store,
            beforeNode,
            afterNode: codeNode,
            beforeFiles: await readCodeNodeFiles(store, beforeNode),
            afterFiles: files,
          })
        : null;
      const check = await createCheckResultNode({ store, codeNode, files });
      const audit = await createAuditNode({ store, codeNode, files });
      const ok = check.result.ok && audit.result.ok && (diff?.result.ok ?? true);
      console.log(
        JSON.stringify(
          {
            ok,
            code_node: codeNode.id,
            diff: diff ? summarizeResultNode(diff.node, diff.result) : null,
            check: summarizeResultNode(check.node, check.result),
            audit: summarizeResultNode(audit.node, audit.result),
          },
          null,
          2,
        ),
      );
      setFailedExitIf(!ok);
    });

  graph
    .command("select")
    .argument("<code-node>")
    .description("Select a CodeNode only after deterministic check and audit pass")
    .action(async (codeNodeId: string) => {
      const store = new GraphStore(process.cwd());
      const codeNode = await store.readNode(codeNodeId);
      if (codeNode.type !== "CodeNode") {
        throw new Error(`Expected CodeNode, got ${codeNode.type}.`);
      }
      const selectionNode = await createSelectionNode({ store, codeNode });
      console.log(JSON.stringify(selectionNode, null, 2));
    });

  graph
    .command("report")
    .description("Summarize graph experiment status and diagnostics")
    .action(async () => {
      const store = new GraphStore(process.cwd());
      const nodes = await store.listNodes();
      const edges = await store.listEdges();
      const report = await buildGraphReport(store, nodes, edges);
      console.log(JSON.stringify(report, null, 2));
    });
}
