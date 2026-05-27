import { describe, expect, it } from "vitest";
import { verifyCandidateTypeScriptBuild } from "../../src/backend/candidate_verify.js";

describe("verifyCandidateTypeScriptBuild", () => {
  it("builds and typechecks candidate files before materialization", async () => {
    const result = await verifyCandidateTypeScriptBuild({
      "domains/Demo/domain.sophia": "domain Demo { }\n",
      "domains/Demo/capabilities/PureCapability.sophia":
        "capability PureCapability { allow { } }\n",
      "domains/Demo/actions/Countdown.sophia": `
action Countdown {
  capability: PureCapability
  output { numbers: List<Int> }
  effects { }
  body {
    let mutable numbers = []
    let mutable current = 5
    repeat 5 times {
      set numbers = numbers + [current]
      set current = current - 1
    }
    return numbers
  }
}
`,
    });

    expect(result.ok).toBe(true);
    expect(result.build.ok).toBe(true);
    expect(result.typecheck?.ok).toBe(true);
  });
});
