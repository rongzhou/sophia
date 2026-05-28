import { describe, expect, it } from "vitest";
import { checkSophiaFiles } from "../../src/lang/checker/index.js";

describe("v0.2 regression boundaries", () => {
  it("accepts explicit intent conversion, safe console output, storage write metadata, and error propagation", () => {
    const result = checkSophiaFiles({
      "domains/Demo/capabilities/PureCapability.sophia": `
capability PureCapability {
  allow { }
}
`,
      "domains/Demo/capabilities/ConsoleStorageCapability.sophia": `
capability ConsoleStorageCapability {
  allow {
    Console.Write
    DB.Write("Todos")
  }
}
`,
      "domains/Demo/storages/Todos.sophia": `
storage Todos {
  key: Persisted<Text>
  value: Sanitized<Text>
}
`,
      "domains/Demo/errors/SaveError.sophia": `
error SaveError {
  variant InvalidTitle {
    reason: Text
  }
}
`,
      "domains/Demo/actions/SanitizeTitle.sophia": `
action SanitizeTitle {
  capability: PureCapability
  intent_conversion: true
  input { title: Raw<Text> }
  output { result: Sanitized<Text> }
  effects { }
  body {
    return title
  }
}
`,
      "domains/Demo/actions/RedactToken.sophia": `
action RedactToken {
  capability: PureCapability
  intent_conversion: true
  input { token: Secret<Text> }
  output { result: Redacted<Text> }
  effects { }
  body {
    return token
  }
}
`,
      "domains/Demo/actions/ValidateTitle.sophia": `
action ValidateTitle {
  capability: PureCapability
  input { title: Sanitized<Text> }
  output { result: Sanitized<Text> }
  effects { }
  errors { InvalidTitle }
  body {
    if title == "" {
      raise InvalidTitle { reason = "empty" }
    }
    return title
  }
}
`,
      "domains/Demo/actions/SaveTodoTitle.sophia": `
action SaveTodoTitle {
  capability: ConsoleStorageCapability
  input {
    raw_title: Raw<Text>
    token: Secret<Text>
  }
  output { result: Sanitized<Text> }
  effects {
    Console.Write
    DB.Write("Todos")
  }
  errors { InvalidTitle }
  body {
    let sanitized = SanitizeTitle { title = raw_title }
    let valid = ValidateTitle { title = sanitized }
    let redacted = RedactToken { token = token }
    print valid
    print redacted
    return valid
  }
}
`,
    });

    expect(result.ok).toBe(true);
  });

  it("rejects Raw/Secret external output, DB.Write intent mismatch, and denied effects together", () => {
    const result = checkSophiaFiles({
      "domains/Demo/capabilities/UnsafeCapability.sophia": `
capability UnsafeCapability {
  allow {
    Console.Write
    DB.Write("Todos")
  }
  deny {
    DB.Write("Todos")
  }
}
`,
      "domains/Demo/storages/Todos.sophia": `
storage Todos {
  key: Persisted<Text>
  value: Sanitized<Text>
}
`,
      "domains/Demo/actions/UnsafeSave.sophia": `
action UnsafeSave {
  capability: UnsafeCapability
  input {
    raw_title: Raw<Text>
    token: Secret<Text>
  }
  output { result: Raw<Text> }
  effects {
    Console.Write
    DB.Write("Todos")
  }
  body {
    print raw_title
    print token
    return raw_title
  }
}
`,
    });

    expect(result.ok).toBe(false);
    expect(result.diagnostics.map((diagnostic) => diagnostic.code)).toEqual(
      expect.arrayContaining([
        "CHECK-INTENT-BOUNDARY-001",
        "CHECK-STORAGE-WRITE-002",
        "CHECK-CAPABILITY-004",
      ]),
    );
    expect(result.diagnostics.map((diagnostic) => diagnostic.problem)).toEqual(
      expect.arrayContaining([
        "Console.Write cannot output Raw<Text>; external output requires a literal, Sanitized<T>, or Redacted<T>.",
        "Console.Write cannot output Secret<Text>; external output requires a literal, Sanitized<T>, or Redacted<T>.",
        'Action UnsafeSave declares DB.Write("Todos"), but output type Raw<Text> does not match storage Todos value type Sanitized<Text>.',
        'Action effect DB.Write("Todos") is denied by capability UnsafeCapability.',
      ]),
    );
  });

  it("rejects undeclared raises and missing called-error propagation", () => {
    const result = checkSophiaFiles({
      "domains/Demo/capabilities/PureCapability.sophia": `
capability PureCapability {
  allow { }
}
`,
      "domains/Demo/errors/SaveError.sophia": `
error SaveError {
  variant InvalidTitle {
    reason: Text
  }
}
`,
      "domains/Demo/actions/RaiseWithoutErrors.sophia": `
action RaiseWithoutErrors {
  capability: PureCapability
  output { result: Text }
  effects { }
  errors { }
  body {
    raise InvalidTitle { reason = "empty" }
  }
}
`,
      "domains/Demo/actions/ValidateTitle.sophia": `
action ValidateTitle {
  capability: PureCapability
  input { title: Text }
  output { result: Text }
  effects { }
  errors { InvalidTitle }
  body {
    if title == "" {
      raise InvalidTitle { reason = "empty" }
    }
    return title
  }
}
`,
      "domains/Demo/actions/SaveTitle.sophia": `
action SaveTitle {
  capability: PureCapability
  input { title: Text }
  output { result: Text }
  effects { }
  errors { }
  body {
    return ValidateTitle { title = title }
  }
}
`,
    });

    expect(result.ok).toBe(false);
    expect(result.diagnostics.map((diagnostic) => diagnostic.code)).toEqual(
      expect.arrayContaining(["CHECK-ERROR-005", "CHECK-ACTION-CALL-008"]),
    );
    expect(result.diagnostics.map((diagnostic) => diagnostic.problem)).toEqual(
      expect.arrayContaining([
        "Action RaiseWithoutErrors raises InvalidTitle, but does not declare it in errors.",
        "Action SaveTitle calls ValidateTitle, but does not declare called error InvalidTitle.",
      ]),
    );
  });
});
