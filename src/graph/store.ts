import { mkdir, readFile, readdir, writeFile } from "node:fs/promises";
import path from "node:path";
import { z } from "zod";
import { ensureDir, graphNodesPath, graphPath } from "../workspace/fs_layout.js";
import { withFileLock } from "../util/fs.js";
import { isSafeRelativeArtifactPath } from "../workspace/sophia_paths.js";
import type { GraphEdge } from "./edges.js";
import { GraphEdgeSchema } from "./edges.js";
import type { GraphNode, GraphNodeType, NodeStatus } from "./nodes.js";
import { GraphNodeSchema, nodeDirectoryName } from "./nodes.js";

const GraphEdgesSchema = z.array(GraphEdgeSchema);

interface CreateNodeInput {
  type: GraphNodeType;
  createdFrom: string | null;
  action_used: string;
  summary: string;
  status?: NodeStatus;
  artifacts?: string[];
  goal?: string | undefined;
  tags?: string[];
  model?: string;
  promptArtifact?: string;
  responseArtifact?: string;
}

export class GraphStore {
  constructor(
    private readonly root: string,
    private readonly graphRelativePath?: string,
  ) {}

  async init(): Promise<void> {
    await ensureDir(graphNodesPath(this.root, this.graphRelativePath));
    await writeJsonIfMissing(path.join(this.graphDir(), "edges.json"), []);
    await writeJsonIfMissing(path.join(this.graphDir(), "index.json"), { next_id: 1 });
    await this.reconcileIndex();
  }

  async createNode(input: CreateNodeInput): Promise<GraphNode> {
    await this.init();
    const id = await this.allocateNodeId();
    const node: GraphNode = {
      id,
      type: input.type,
      status: input.status ?? "active",
      created_from: input.createdFrom,
      action_used: input.action_used,
      goal: input.goal,
      version: 1,
      artifacts: input.artifacts ?? [],
      summary: input.summary,
      score: {},
      tags: input.tags ?? [],
      model: input.model,
      prompt_artifact: input.promptArtifact,
      response_artifact: input.responseArtifact,
    };
    const parsed = GraphNodeSchema.parse(node);
    const dir = this.nodeDir(parsed);
    await mkdir(dir, { recursive: true });
    await writeFile(path.join(dir, "node.json"), `${JSON.stringify(parsed, null, 2)}\n`, "utf8");
    return parsed;
  }

  async readNode(id: string): Promise<GraphNode> {
    const dir = await this.findNodeDir(id);
    const content = await readFile(path.join(dir, "node.json"), "utf8");
    return GraphNodeSchema.parse(JSON.parse(content));
  }

  async listNodes(): Promise<GraphNode[]> {
    await this.init();
    const nodesRoot = this.nodesDir();
    const entries = await readdir(nodesRoot, { withFileTypes: true }).catch(() => []);
    const nodes = await Promise.all(
      entries
        .filter((entry) => entry.isDirectory())
        .map(async (entry) => {
          const content = await readFile(path.join(nodesRoot, entry.name, "node.json"), "utf8");
          return GraphNodeSchema.parse(JSON.parse(content));
        }),
    );
    return nodes.sort((left, right) => left.id.localeCompare(right.id));
  }

  async updateNode(node: GraphNode): Promise<GraphNode> {
    const parsed = GraphNodeSchema.parse(node);
    const dir = await this.findNodeDir(parsed.id);
    await writeFile(path.join(dir, "node.json"), `${JSON.stringify(parsed, null, 2)}\n`, "utf8");
    return parsed;
  }

  async writeArtifact(node: GraphNode, relativePath: string, content: string): Promise<void> {
    assertSafeRelativePath(relativePath);
    const artifactPath = path.join(this.nodeDir(node), relativePath);
    await mkdir(path.dirname(artifactPath), { recursive: true });
    await writeFile(artifactPath, content, "utf8");
  }

  async readArtifact(node: GraphNode, relativePath: string): Promise<string> {
    assertSafeRelativePath(relativePath);
    return readFile(path.join(this.nodeDir(node), relativePath), "utf8");
  }

  async readArtifactJson<T>(node: GraphNode, relativePath: string): Promise<T> {
    return JSON.parse(await this.readArtifact(node, relativePath)) as T;
  }

  async appendEdge(edge: GraphEdge): Promise<void> {
    const parsed = GraphEdgeSchema.parse(edge);
    await this.init();
    await this.withFileLock("edges.lock", async () => {
      const edgesPath = path.join(this.graphDir(), "edges.json");
      const edges = await this.readEdges();
      edges.push(parsed);
      await writeFile(edgesPath, `${JSON.stringify(edges, null, 2)}\n`, "utf8");
    });
  }

  async listEdges(): Promise<GraphEdge[]> {
    await this.init();
    return this.readEdges();
  }

  nodeDir(node: Pick<GraphNode, "id" | "type">): string {
    return path.join(this.nodesDir(), nodeDirectoryName(node));
  }

  private async readEdges(): Promise<GraphEdge[]> {
    const edgesPath = path.join(this.graphDir(), "edges.json");
    const content = await readFile(edgesPath, "utf8").catch(() => "[]");
    return GraphEdgesSchema.parse(JSON.parse(content));
  }

  private async allocateNodeId(): Promise<string> {
    return this.withFileLock("index.lock", async () => {
      await this.reconcileIndex();
      const indexPath = path.join(this.graphDir(), "index.json");
      const index = JSON.parse(await readFile(indexPath, "utf8")) as { next_id?: unknown };
      const nextId = typeof index.next_id === "number" ? index.next_id : 1;
      await writeFile(indexPath, `${JSON.stringify({ next_id: nextId + 1 }, null, 2)}\n`, "utf8");
      return `N${String(nextId).padStart(4, "0")}`;
    });
  }

  private async findNodeDir(id: string): Promise<string> {
    const nodesRoot = this.nodesDir();
    const entries = await readdir(nodesRoot, { withFileTypes: true });
    const match = entries.find((entry) => entry.isDirectory() && entry.name.startsWith(`${id}.`));
    if (!match) {
      throw new Error(`Graph node not found: ${id}`);
    }
    return path.join(nodesRoot, match.name);
  }

  private async reconcileIndex(): Promise<void> {
    const indexPath = path.join(this.graphDir(), "index.json");
    const current = JSON.parse(await readFile(indexPath, "utf8")) as { next_id?: unknown };
    const currentNextId = typeof current.next_id === "number" ? current.next_id : 1;
    const maxId = await this.maxExistingNodeNumber();
    const nextId = Math.max(currentNextId, maxId + 1);
    if (nextId !== currentNextId) {
      await writeFile(indexPath, `${JSON.stringify({ next_id: nextId }, null, 2)}\n`, "utf8");
    }
  }

  private async maxExistingNodeNumber(): Promise<number> {
    const nodesRoot = this.nodesDir();
    const entries = await readdir(nodesRoot, { withFileTypes: true }).catch(() => []);
    return entries.reduce((max, entry) => {
      if (!entry.isDirectory()) return max;
      const match = /^N(\d{4,})\./.exec(entry.name);
      if (!match) return max;
      return Math.max(max, Number(match[1]));
    }, 0);
  }

  private async withFileLock<T>(lockFileName: string, operation: () => Promise<T>): Promise<T> {
    const lockPath = path.join(this.graphDir(), lockFileName);
    return withFileLock({
      lockPath,
      attempts: 50,
      retryMs: 20,
      operation,
      errorLabel: `graph lock ${lockFileName}`,
    });
  }

  graphDir(): string {
    return graphPath(this.root, this.graphRelativePath);
  }

  private nodesDir(): string {
    return graphNodesPath(this.root, this.graphRelativePath);
  }
}

function assertSafeRelativePath(relativePath: string): void {
  if (!isSafeRelativeArtifactPath(relativePath)) {
    throw new Error(`Unsafe artifact path: ${relativePath}`);
  }
}

async function writeJsonIfMissing(file: string, value: unknown): Promise<void> {
  await readFile(file, "utf8").catch(async () => {
    await mkdir(path.dirname(file), { recursive: true });
    await writeFile(file, `${JSON.stringify(value, null, 2)}\n`, "utf8");
  });
}
