import { describe, expect, it } from "vitest";
import { outlinePseudocode } from "../../src/pseudo/outline.js";

describe("outlinePseudocode", () => {
  it("marks pseudo variables assigned more than once as mutable state candidates", () => {
    const outline = outlinePseudocode(`
program Validate {
  purpose { "Validate a value." }
  inputs { value: Int }
  outputs { result: Bool }
  algorithm {
    subaction ValidateValue {
      set is_valid to false
      if value > 0 {
        set is_valid to true
      }
      return is_valid
    }
    subaction BuildValue {
      set next_value to value + 1
      return next_value
    }
  }
}
`);

    expect(outline.mutable_state_candidates).toEqual(["is_valid"]);
  });
});
