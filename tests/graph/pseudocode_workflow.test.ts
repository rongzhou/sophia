import { describe, expect, it } from "vitest";
import { createTempDir } from "../helpers/sophia_workspace.js";
import {
  assertPseudocodeNodeCanImplement,
  createPseudocodeCheckNode,
} from "../../src/graph/pseudocode_workflow.js";
import { GraphStore } from "../../src/graph/store.js";

describe("pseudocode implementation gate", () => {
  it("requires a recorded PseudocodeCheckNode before implementation", async () => {
    const store = await tempStore();
    const pseudo = await pseudoNode(store);

    await expect(assertPseudocodeNodeCanImplement(store, pseudo)).rejects.toThrow(
      `run graph pseudo-check ${pseudo.id} first`,
    );
  });

  it("rejects implementation when the latest pseudocode check failed", async () => {
    const store = await tempStore();
    const pseudo = await pseudoNode(store);
    await createPseudocodeCheckNode({
      store,
      pseudoNode: pseudo,
      pseudocode: `program Demo {
  purpose { "too vague" }
  inputs { none }
  outputs { result := "numbers" }
  algorithm {
    repeat several times {
      do the calculation
    }
  }
}
`,
    });

    await expect(assertPseudocodeNodeCanImplement(store, pseudo)).rejects.toThrow(
      "run graph revise-design",
    );
  });

  it("allows implementation when the latest pseudocode check passed", async () => {
    const store = await tempStore();
    const pseudo = await pseudoNode(store);
    const check = await createPseudocodeCheckNode({
      store,
      pseudoNode: pseudo,
      pseudocode: passingPseudocode,
    });

    await expect(assertPseudocodeNodeCanImplement(store, pseudo)).resolves.toMatchObject({
      node: { id: check.node.id },
      result: { ok: true },
    });
  });
});

async function tempStore(): Promise<GraphStore> {
  return new GraphStore(await createTempDir("sophia-pseudo-gate-"));
}

async function pseudoNode(store: GraphStore) {
  const node = await store.createNode({
    type: "PseudocodeNode",
    createdFrom: null,
    action_used: "add_pseudo",
    summary: "Pseudo",
    artifacts: ["content.pseudo"],
  });
  await store.writeArtifact(node, "content.pseudo", passingPseudocode);
  return node;
}

const passingPseudocode = `program Demo {
  purpose { "Return a fixed label." }
  inputs { none }
  outputs { result := "text label" }
  algorithm {
    return ready
  }
}
`;
