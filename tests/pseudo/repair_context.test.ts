import { describe, expect, it } from "vitest";
import { checkPseudocode } from "../../src/pseudo/check.js";
import { buildPseudoRepairContext } from "../../src/pseudo/repair_context.js";
import { buildReviseDesignPrompt } from "../../src/llm/tasks/revise_design.js";

describe("buildPseudoRepairContext", () => {
  it("summarizes pseudocode diagnostics without inventing implementation", () => {
    const pseudocode = `
program Demo {
  purpose { "Build values." }
  outputs { values := "list" }
  algorithm {
    repeat several times {
      do the calculation
    }
  }
}
`;
    const checkResult = checkPseudocode(pseudocode);
    const context = buildPseudoRepairContext({ pseudocode, checkResult });

    expect(context.missing_sections).toContain("inputs");
    expect(context.weak_checks).toEqual(
      expect.arrayContaining(["loop_details_explicit", "no_vague_steps"]),
    );
    expect(context.diagnostic_summary.map((item) => item.code)).toEqual(
      expect.arrayContaining(["PSEUDO-SECTION-001", "PSEUDO-LOOP-001"]),
    );
    expect(JSON.stringify(context)).not.toContain("let mutable");
    expect(JSON.stringify(context)).not.toContain("domain Demo");
  });

  it("builds a revision prompt that asks for pseudo only", () => {
    const pseudocode = `
program Demo {
  purpose { "Build values." }
  outputs { values := "list" }
  algorithm { repeat several times { do the calculation } }
}
`;
    const prompt = buildReviseDesignPrompt(pseudocode, checkPseudocode(pseudocode));

    expect(prompt).toContain("revising algorithm pseudocode");
    expect(prompt).toContain("not writing program code");
    expect(prompt).toContain("Do not invent missing business logic");
    expect(prompt).toContain("type annotations");
    expect(prompt).toContain("left followed by right");
    expect(prompt).toContain("Natural console wording");
    expect(prompt).toContain("needs_clarification");
    expect(prompt).toContain("Return schema");
    expect(prompt).not.toContain("Sophia");
    expect(prompt).not.toContain("Sophia-Core");
    expect(prompt).not.toContain("Console.Write");
    expect(prompt).not.toContain("implementation_hints");
    expect(prompt).not.toMatch(/make (the )?tests pass/i);
  });
});
