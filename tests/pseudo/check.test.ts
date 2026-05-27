import { describe, expect, it } from "vitest";
import { readFileSync } from "node:fs";
import { checkPseudocode } from "../../src/pseudo/check.js";

describe("checkPseudocode", () => {
  it("accepts structured JSON algorithm pseudocode", () => {
    const result = checkPseudocode(
      JSON.stringify({
        purpose: "Return whether a count is positive.",
        inputs: [{ name: "count", meaning: "integer count" }],
        outputs: [{ name: "result", meaning: "true when count is positive, false otherwise" }],
        algorithm: [
          "If count is greater than zero, return true.",
          "Otherwise, return false.",
        ],
      }),
    );

    expect(result.ok).toBe(true);
  });

  it("accepts explicit rabbit pseudocode", () => {
    const result = checkPseudocode(`
program Rabbit {
  purpose { "Compute rabbit numbers." }
  inputs { none }
  outputs { numbers := "list of values" }
  effects { "print values" }
  algorithm {
    create empty list numbers
    set previous to 1
    set current to 1
    repeat 8 times {
      set next to previous + current
      print next
      append next to numbers
      set previous to current
      set current to next
    }
    return numbers
  }
  expected { result := "[1, 1, 2]" }
}
`);
    expect(result.ok).toBe(true);
  });

  it("rejects vague loop counts", () => {
    const result = checkPseudocode(`
program Rabbit {
  purpose { "Compute rabbit numbers." }
  inputs { none }
  outputs { numbers := "list of values" }
  algorithm {
    repeat several times {
      do the calculation
    }
    return numbers
  }
}
`);
    expect(result.ok).toBe(false);
    expect(result.diagnostics.map((diagnostic) => diagnostic.code)).toContain("PSEUDO-LOOP-001");
  });

  it("accepts natural branch wording without treating it as a syntax problem", () => {
    const result = checkPseudocode(`
program Label {
  purpose { "Return a label." }
  inputs { count: Int }
  outputs { result: Text }
  effects { "print labels" }
  algorithm {
    if count == 0 then
      print "zero"
      return "zero"
    else if count > 0 then
      return "positive"
    else
      return "negative"
    end if
  }
}
`);

    expect(result.ok).toBe(true);
    expect(result.diagnostics.map((diagnostic) => diagnostic.code)).not.toContain(
      "PSEUDO-BRANCH-001",
    );
  });

  it("warns about direct list emptiness checks without rejecting pseudocode", () => {
    const result = checkPseudocode(`
program FilterValues {
  purpose { "Build a filtered list." }
  inputs { first: Int }
  outputs { result: List<Int> }
  algorithm {
    set result to empty List<Int>
    if result is empty {
      print "empty"
    }
    return result
  }
  effects { "print when list is empty" }
}
`);

    expect(result.ok).toBe(true);
    expect(result.diagnostics.map((diagnostic) => diagnostic.code)).toContain("PSEUDO-LIST-001");
    expect(
      result.diagnostics.find((diagnostic) => diagnostic.code === "PSEUDO-LIST-001")?.severity,
    ).toBe("warning");
  });

  it("warns about increment shorthand without rejecting pseudocode", () => {
    const result = checkPseudocode(`
program Counter {
  purpose { "Count positives." }
  inputs { first: Int }
  outputs { result: Int }
  algorithm {
    set result to 0
    if first > 0 {
      increment result
    }
    return result
  }
}
`);

    expect(result.ok).toBe(true);
    expect(result.diagnostics.map((diagnostic) => diagnostic.code)).toContain("PSEUDO-STATE-001");
    expect(
      result.diagnostics.find((diagnostic) => diagnostic.code === "PSEUDO-STATE-001")?.severity,
    ).toBe("warning");
  });

  it("warns about explicit text conversion for console printing without rejecting pseudocode", () => {
    const result = checkPseudocode(`
program PrintSquares {
  purpose { "Print generated numbers." }
  inputs { none }
  outputs { result: List<Int> }
  algorithm {
    set result to empty List<Int>
    set current to 1
    repeat 5 times {
      set square to current * current
      set square_text to convert square to Text
      print square as text
      append square to result
      set current to current + 1
    }
    return result
  }
  effects { "print generated squares" }
}
`);

    expect(result.ok).toBe(true);
    expect(result.diagnostics.map((diagnostic) => diagnostic.code)).toEqual(
      expect.arrayContaining(["PSEUDO-TEXT-001", "PSEUDO-TEXT-002"]),
    );
    expect(result.diagnostics.every((diagnostic) => diagnostic.severity === "warning")).toBe(true);
  });

  it("rejects else-nested input chains when building list results", () => {
    const result = checkPseudocode(`
program RunningTotals {
  purpose { "Build running totals for positive inputs." }
  inputs { first: Int, second: Int, third: Int }
  outputs { result: List<Int> }
  algorithm {
    set result to empty list
    set running_total to 0
    if first > 0 {
      set running_total to running_total + first
      append running_total to result
    } else {
      if second > 0 {
        set running_total to running_total + second
        append running_total to result
      } else {
        if third > 0 {
          set running_total to running_total + third
          append running_total to result
        }
      }
    }
    return result
  }
}
`);

    expect(result.ok).toBe(false);
    expect(result.diagnostics.map((diagnostic) => diagnostic.code)).toContain("PSEUDO-BRANCH-002");
  });

  it("allows independent input branches when building list results", () => {
    const result = checkPseudocode(`
program RunningTotals {
  purpose { "Build running totals for positive inputs." }
  inputs { first: Int, second: Int, third: Int }
  outputs { result: List<Int> }
  algorithm {
    set result to empty list
    set running_total to 0
    if first > 0 {
      set running_total to running_total + first
      append running_total to result
    }
    if second > 0 {
      set running_total to running_total + second
      append running_total to result
    }
    if third > 0 {
      set running_total to running_total + third
      append running_total to result
    }
    return result
  }
}
`);

    expect(result.ok).toBe(true);
  });

  it("warns about multiple pseudocode outputs for the v0 single-output action model", () => {
    const result = checkPseudocode(`
program MultiOutput {
  purpose { "Return a value." }
  inputs { value: Int }
  outputs { result: Int, helper: Bool }
  algorithm {
    return value
  }
}
`);

    expect(result.ok).toBe(true);
    expect(result.diagnostics.map((diagnostic) => diagnostic.code)).toContain("PSEUDO-OUTPUT-001");
    expect(
      result.diagnostics.find((diagnostic) => diagnostic.code === "PSEUDO-OUTPUT-001")?.severity,
    ).toBe("warning");
  });

  it("warns about numeric 0/1 flags used as Bool-like conditions", () => {
    const result = checkPseudocode(`
program NumericFlag {
  purpose { "Use a flag." }
  inputs { value: Int }
  outputs { result: Int }
  algorithm {
    set is_valid to 0
    if value > 0 {
      set is_valid to 1
    }
    if is_valid {
      return value
    } else {
      return 0
    }
  }
}
`);

    expect(result.ok).toBe(true);
    expect(result.diagnostics.map((diagnostic) => diagnostic.code)).toContain("PSEUDO-BOOL-001");
    expect(
      result.diagnostics.find((diagnostic) => diagnostic.code === "PSEUDO-BOOL-001")?.severity,
    ).toBe("warning");
  });

  it("warns about implementation hints without rejecting pseudocode", () => {
    const result = checkPseudocode(`
program Flow {
  purpose { "Use public names." }
  implementation_hints {
    program: Flow
    domain: FlowDomain
    main_action: Flow
    capability: FlowCapability
  }
  inputs { value: Int }
  outputs { result: Int }
  algorithm { return value }
}
`);

    expect(result.ok).toBe(true);
    expect(result.diagnostics.map((diagnostic) => diagnostic.code)).toContain("PSEUDO-HINT-001");
    expect(
      result.diagnostics.find((diagnostic) => diagnostic.code === "PSEUDO-HINT-001")?.severity,
    ).toBe("warning");
  });

  it("warns about implementation hints even when keys were historically recognized", () => {
    const result = checkPseudocode(`
program Flow {
  purpose { "Use public names." }
  implementation_hints {
    program: Flow
    domain: FlowDomain
    action: Flow
    capability: FlowCapability
  }
  inputs { value: Int }
  outputs { result: Int }
  algorithm { return value }
}
`);

    expect(result.ok).toBe(true);
    expect(result.diagnostics.map((diagnostic) => diagnostic.code)).toContain("PSEUDO-HINT-001");
  });

  it("warns about main_flow that only delegates to a wrapper subaction", () => {
    const result = checkPseudocode(`
program ProcessItem {
  purpose { "Process an item through helper stages." }
  inputs { value: Int }
  outputs { result: Int }
  algorithm {
    subaction ValidateItem {
      return whether value is positive
    }
    subaction OrchestrateItem {
      set is_valid to output of ValidateItem using value
      if is_valid {
        set result to value
      } else {
        set result to 0
      }
      return result
    }
    main_flow ProcessItem {
      set result to output of OrchestrateItem using value
      return result
    }
  }
}
`);

    expect(result.ok).toBe(true);
    expect(result.diagnostics.map((diagnostic) => diagnostic.code)).toContain("PSEUDO-FLOW-001");
    expect(
      result.diagnostics.find((diagnostic) => diagnostic.code === "PSEUDO-FLOW-001")?.severity,
    ).toBe("warning");
  });

  it("accepts main_flow that performs orchestration directly", () => {
    const result = checkPseudocode(`
program ProcessItem {
  purpose { "Process an item through helper stages." }
  inputs { value: Int }
  outputs { result: Int }
  algorithm {
    subaction ValidateItem {
      return whether value is positive
    }
    main_flow ProcessItem {
      set is_valid to output of ValidateItem using value
      if is_valid {
        set result to value
      } else {
        set result to 0
      }
      return result
    }
  }
}
`);

    expect(result.ok).toBe(true);
  });

  it("accepts the action pipeline fixture as structured pseudocode", () => {
    const fixture = readFileSync("fixtures/account/process_deposit_pipeline.pseudo", "utf8");
    const result = checkPseudocode(fixture);

    expect(result.ok).toBe(true);
    expect(result.diagnostics.map((diagnostic) => diagnostic.code)).toEqual(["PSEUDO-HINT-001"]);
    expect(result.checks.has_expected).toBe(true);
  });
});
