import { mkdir, writeFile } from "node:fs/promises";
import path from "node:path";
import type { Command } from "commander";
import { readSophiaFilesFromDomains } from "./cli_utils.js";
import { verifyCandidateTypeScriptBuild } from "../backend/candidate_verify.js";
import { checkSophiaFiles } from "../lang/checker.js";
import { pathExists } from "../util/fs.js";
import { isSupportedSophiaFilePath } from "../workspace/sophia_paths.js";
import {
  assertCodeNodeCanMaterialize,
  assertCodeNodeSelectedForMaterialize,
  readCodeNodeFiles,
} from "../graph/code_workflow.js";
import type { GraphNode } from "../graph/nodes.js";
import { GraphStore } from "../graph/store.js";

export function registerGraphMaterializeCommands(graph: Command): void {
  graph
    .command("materialize")
    .argument("<code-node>")
    .option("--force", "Overwrite existing materialized files")
    .description("Write a checked and audited CodeNode into domains/")
    .action(async (codeNodeId: string, options: { force?: boolean }) => {
      const store = new GraphStore(process.cwd());
      const codeNode = await store.readNode(codeNodeId);
      if (codeNode.type !== "CodeNode") {
        throw new Error(`Expected CodeNode, got ${codeNode.type}.`);
      }
      await assertCodeNodeCanMaterialize(store, codeNode);
      await assertCodeNodeSelectedForMaterialize(store, codeNode);

      const files = await readCodeNodeFiles(store, codeNode);
      const fileEntries = Object.entries(files).sort(([left], [right]) =>
        left.localeCompare(right),
      );
      for (const [filePath] of fileEntries) {
        if (!isSupportedSophiaFilePath(filePath)) {
          throw new Error(`Refusing to materialize unsafe or unsupported file path: ${filePath}`);
        }
        if (!options.force) {
          const target = path.join(process.cwd(), filePath);
          if (await pathExists(target)) {
            throw new Error(`Refusing to overwrite existing file without --force: ${filePath}`);
          }
        }
      }

      const candidateDomains = {
        ...(await readSophiaFilesFromDomains(process.cwd())),
        ...files,
      };
      const preflight = checkSophiaFiles(candidateDomains);
      if (!preflight.ok) {
        await recordMaterializeFailure({
          store,
          codeNode,
          summary: `Materialize preflight failed for ${codeNode.id}.`,
          payload: { ok: false, code_node: codeNode.id, check: preflight },
        });
        throw new Error(
          `Materialize preflight failed: ${JSON.stringify(preflight.diagnostics, null, 2)}`,
        );
      }
      const buildPreflight = await verifyCandidateTypeScriptBuild(candidateDomains);
      if (!buildPreflight.ok) {
        await recordMaterializeFailure({
          store,
          codeNode,
          summary: `Materialize build preflight failed for ${codeNode.id}.`,
          payload: { ok: false, code_node: codeNode.id, check: preflight, build: buildPreflight },
        });
        throw new Error(
          `Materialize build preflight failed: ${JSON.stringify(buildPreflight, null, 2)}`,
        );
      }

      for (const [filePath, content] of fileEntries) {
        const target = path.join(process.cwd(), filePath);
        await mkdir(path.dirname(target), { recursive: true });
        await writeFile(target, content, "utf8");
      }

      const result = checkSophiaFiles(await readSophiaFilesFromDomains(process.cwd()));
      if (!result.ok) {
        throw new Error(
          `Materialized domains failed deterministic check: ${JSON.stringify(result.diagnostics, null, 2)}`,
        );
      }
      const materializeNode = await store.createNode({
        type: "MaterializeNode",
        createdFrom: codeNode.id,
        action_used: "materialize_code",
        ...(codeNode.goal ? { goal: codeNode.goal } : {}),
        summary: `Materialized ${codeNode.id} into domains/.`,
        artifacts: ["result.json"],
        tags: ["materialize"],
      });
      const materializeResult = {
        ok: true,
        code_node: codeNode.id,
        files: Object.keys(files).sort(),
        check: result,
        build: buildPreflight,
      };
      await store.writeArtifact(
        materializeNode,
        "result.json",
        `${JSON.stringify(materializeResult, null, 2)}\n`,
      );
      await store.appendEdge({ from: codeNode.id, to: materializeNode.id, type: "materializes" });
      console.log(JSON.stringify({ node: materializeNode, ...materializeResult }, null, 2));
    });
}

async function recordMaterializeFailure(options: {
  store: GraphStore;
  codeNode: GraphNode;
  summary: string;
  payload: Record<string, unknown>;
}): Promise<GraphNode> {
  const failedNode = await options.store.createNode({
    type: "MaterializeNode",
    status: "failed",
    createdFrom: options.codeNode.id,
    action_used: "materialize_code",
    ...(options.codeNode.goal ? { goal: options.codeNode.goal } : {}),
    summary: options.summary,
    artifacts: ["result.json"],
    tags: ["materialize", "failed"],
  });
  await options.store.writeArtifact(
    failedNode,
    "result.json",
    `${JSON.stringify(options.payload, null, 2)}\n`,
  );
  await options.store.appendEdge({
    from: options.codeNode.id,
    to: failedNode.id,
    type: "materializes",
  });
  return failedNode;
}
