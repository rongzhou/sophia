import { stripQuotedText } from "./braces.js";
import { parseActions, type SophiaFileSet } from "./check_model.js";
import type { CheckerContext } from "./checker_context.js";
import { error } from "./diagnostics.js";

interface ActionGraphNode {
  name: string;
  path: string;
  calls: Set<string>;
}

export function checkActionCallGraph(context: CheckerContext, files: SophiaFileSet): void {
  const graph = buildActionCallGraph(context, files);
  const reported = new Set<string>();

  for (const node of graph.values()) {
    findCycles(node.name, graph, [], new Set(), (cycle) => {
      const key = canonicalCycleKey(cycle);
      if (reported.has(key)) return;
      reported.add(key);
      context.diagnostics.push(
        error(
          "CHECK-ACTION-CALL-007",
          node.path,
          `Recursive action call cycle is not supported in v0: ${cycle.join(" -> ")}.`,
          "Break the cycle by making one action consume an explicit input value instead of calling back into the caller.",
        ),
      );
    });
  }
}

function buildActionCallGraph(
  context: CheckerContext,
  files: SophiaFileSet,
): Map<string, ActionGraphNode> {
  const graph = new Map<string, ActionGraphNode>();
  for (const [filePath, content] of Object.entries(files)) {
    if (!filePath.endsWith(".sophia")) continue;
    for (const action of parseActions(content)) {
      const calls = new Set<string>();
      for (const calledAction of collectActionCalls(action.body, context)) {
        if (calledAction !== action.name) calls.add(calledAction);
      }
      graph.set(action.name, { name: action.name, path: filePath, calls });
    }
  }
  return graph;
}

function collectActionCalls(body: string, context: CheckerContext): string[] {
  const calls: string[] = [];
  for (const match of stripQuotedText(body).matchAll(/\b([A-Z][A-Za-z0-9]*)\s*\{/g)) {
    const name = match[1];
    if (!name || context.entityTypes.has(name) || !context.actionTypes.has(name)) continue;
    calls.push(name);
  }
  return calls;
}

function findCycles(
  current: string,
  graph: Map<string, ActionGraphNode>,
  path: string[],
  visiting: Set<string>,
  onCycle: (cycle: string[]) => void,
): void {
  if (visiting.has(current)) {
    const start = path.indexOf(current);
    if (start >= 0) onCycle([...path.slice(start), current]);
    return;
  }

  const node = graph.get(current);
  if (!node) return;

  visiting.add(current);
  path.push(current);
  for (const next of node.calls) {
    findCycles(next, graph, path, visiting, onCycle);
  }
  path.pop();
  visiting.delete(current);
}

function canonicalCycleKey(cycle: string[]): string {
  const uniqueCycle = cycle.slice(0, -1);
  const rotations = uniqueCycle.map((_, index) => [
    ...uniqueCycle.slice(index),
    ...uniqueCycle.slice(0, index),
  ]);
  return rotations.map((rotation) => rotation.join("\0")).sort()[0] ?? uniqueCycle.join("\0");
}
