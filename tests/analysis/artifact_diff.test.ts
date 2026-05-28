import { describe, expect, it } from "vitest";
import { diffSophiaArtifacts } from "../../src/analysis/artifact_diff.js";

describe("diffSophiaArtifacts", () => {
  it("blocks removed files while reporting added, changed, and unchanged files", () => {
    const result = diffSophiaArtifacts({
      before: {
        "domains/Demo/domain.sophia": "domain Demo {\n}\n",
        "domains/Demo/actions/A.sophia": "action A {\n}\n",
        "domains/Demo/actions/Removed.sophia": "action Removed {\n}\n",
      },
      after: {
        "domains/Demo/domain.sophia": "domain Demo {\n}\n",
        "domains/Demo/actions/A.sophia": "action A {\n  body { return unit }\n}\n",
        "domains/Demo/actions/Added.sophia": "action Added {\n}\n",
      },
    });

    expect(result.ok).toBe(false);
    expect(result.files.added).toEqual(["domains/Demo/actions/Added.sophia"]);
    expect(result.files.removed).toEqual(["domains/Demo/actions/Removed.sophia"]);
    expect(result.files.changed).toEqual(["domains/Demo/actions/A.sophia"]);
    expect(result.files.unchanged).toEqual(["domains/Demo/domain.sophia"]);
    expect(result.diagnostics.map((diagnostic) => diagnostic.code)).toContain("DIFF-FILE-001");
  });

  it("blocks repairs that remove capabilities, actions, or effects", () => {
    const result = diffSophiaArtifacts({
      before: {
        "domains/Demo/capabilities/ConsoleCapability.sophia": `
capability ConsoleCapability {
  allow { Console.Write }
}
`,
        "domains/Demo/actions/Demo.sophia": `
action Demo {
  capability: ConsoleCapability
  effects { Console.Write }
  body { print "hello" }
}
`,
      },
      after: {
        "domains/Demo/actions/Demo.sophia": `
action Demo {
  body { return unit }
}
`,
      },
    });

    expect(result.diagnostics.map((diagnostic) => diagnostic.code)).toEqual(
      expect.arrayContaining(["DIFF-CAPABILITY-001", "DIFF-EFFECT-001"]),
    );
    expect(result.diagnostics.every((diagnostic) => diagnostic.severity === "error")).toBe(true);
  });

  it("allows small repairs that preserve files, declarations, and effects", () => {
    const result = diffSophiaArtifacts({
      before: {
        "domains/Demo/capabilities/ConsoleCapability.sophia": `
capability ConsoleCapability {
  allow { Console.Write }
}
`,
        "domains/Demo/actions/Demo.sophia": `
action Demo {
  capability: ConsoleCapability
  effects { Console.Write }
  body {
    var message = "hello"
    print message
  }
}
`,
      },
      after: {
        "domains/Demo/capabilities/ConsoleCapability.sophia": `
capability ConsoleCapability {
  allow { Console.Write }
}
`,
        "domains/Demo/actions/Demo.sophia": `
action Demo {
  capability: ConsoleCapability
  effects { Console.Write }
  body {
    let message = "hello"
    print message
  }
}
`,
      },
    });

    expect(result.ok).toBe(true);
    expect(result.diagnostics).toEqual([]);
  });

  it("detects removed parameterized action effects", () => {
    const result = diffSophiaArtifacts({
      before: {
        "domains/Demo/actions/Load.sophia": `
action Load {
  input { id: Text }
  output { item: Optional<Item> }
  effects { DB.Read("Item") }
  body { return none }
}
`,
      },
      after: {
        "domains/Demo/actions/Load.sophia": `
action Load {
  input { id: Text }
  output { item: Optional<Item> }
  body { return none }
}
`,
      },
    });

    expect(result.diagnostics).toEqual(
      expect.arrayContaining([
        expect.objectContaining({
          code: "DIFF-EFFECT-001",
          location: 'DB.Read("Item")',
        }),
      ]),
    );
  });

  it("does not treat generic type names as effects", () => {
    const result = diffSophiaArtifacts({
      before: {
        "domains/Demo/actions/Load.sophia": `
action Load {
  output { item: Optional<Item> }
  body { return none }
}
`,
      },
      after: {
        "domains/Demo/actions/Load.sophia": `
action Load {
  output { item: Text }
  body { return "missing" }
}
`,
      },
    });

    expect(result.diagnostics.map((diagnostic) => diagnostic.code)).not.toContain(
      "DIFF-EFFECT-001",
    );
  });
});
