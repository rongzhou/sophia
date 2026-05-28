import { describe, expect, it } from "vitest";
import { outlinePseudocode } from "../../src/pseudo/outline.js";
import { samplePseudocodeJson } from "../helpers/sophia_workspace.js";

describe("outlinePseudocode", () => {
  it("marks pseudo variables assigned more than once as mutable state candidates", () => {
    const outline = outlinePseudocode(
      samplePseudocodeJson({
        purpose: "Validate a value.",
        inputs: [{ name: "value", meaning: "integer" }],
        outputs: [{ name: "result", meaning: "boolean" }],
        algorithm: [
          "set is_valid to false",
          "if value > 0",
          "set is_valid to true",
          "return is_valid",
          "set next_value to value + 1",
          "return next_value",
        ],
      }),
    );

    expect(outline.mutable_state_candidates).toEqual(["is_valid"]);
  });
});
