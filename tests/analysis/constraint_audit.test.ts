import { describe, expect, it } from "vitest";
import { auditConstraints } from "../../src/analysis/constraint_audit.js";
import { samplePseudocodeJson } from "../helpers/sophia_workspace.js";

describe("auditConstraints", () => {
  it("rejects hardcoded expected lists when forbidden", () => {
    const result = auditConstraints({
      pseudocode: samplePseudocodeJson({
        constraints: ["Do not hardcode the full list."],
        expected: { result: "[4, 8, 15, 16, 23, 42]" },
      }),
      files: {
        "domains/demo/actions/demo.sophia": `body { return [4,8,15,16,23,42] }`,
      },
    });

    expect(result.ok).toBe(false);
    expect(result.diagnostics.map((diagnostic) => diagnostic.code)).toContain("AUDIT-HARDCODE-001");
  });

  it("accepts the repaired rabbit loop shape", () => {
    const result = auditConstraints({
      pseudocode: samplePseudocodeJson({
        algorithm: ["repeat 8 times: set next to previous + current"],
        forbidden: [
          "Do not use storage.",
          "Do not use time.",
          "Do not use network.",
          "Do not use randomness.",
        ],
        constraints: ["Do not hardcode the full list."],
      }),
      files: {
        "domains/rabbit/actions/rabbit.sophia": `
body {
  repeat 8 times {
    let mutable next = previous + current
    print next
  }
}
`,
      },
    });

    expect(result.ok).toBe(true);
  });

  it("warns when a bounded repeat count is not preserved", () => {
    const result = auditConstraints({
      pseudocode: samplePseudocodeJson({ algorithm: ["repeat 5 times: print current"] }),
      files: {
        "domains/demo/actions/demo.sophia": `body { repeat 4 times { print current } }`,
      },
    });

    expect(result.ok).toBe(true);
    expect(result.diagnostics.map((diagnostic) => diagnostic.code)).toContain("AUDIT-LOOP-001");
  });

  it("rejects hardcoded scalar direct returns when forbidden", () => {
    const result = auditConstraints({
      pseudocode: samplePseudocodeJson({
        expected: { result: "15" },
        forbidden: ["Do not hardcode the result as a direct return."],
      }),
      files: {
        "domains/sum/actions/sum.sophia": `body { return 15 }`,
      },
    });

    expect(result.ok).toBe(false);
    expect(result.diagnostics.map((diagnostic) => diagnostic.code)).toContain("AUDIT-HARDCODE-002");
  });

  it("rejects print usage when pseudocode forbids printing", () => {
    const result = auditConstraints({
      pseudocode: samplePseudocodeJson({ forbidden: ["Do not print."] }),
      files: {
        "domains/demo/actions/demo.sophia": `
action Demo {
  effects { Console.Write }
  body {
    print "hidden side effect"
  }
}
`,
      },
    });

    expect(result.ok).toBe(false);
    expect(result.diagnostics.map((diagnostic) => diagnostic.code)).toContain(
      "AUDIT-FORBIDDEN-001",
    );
  });
});
