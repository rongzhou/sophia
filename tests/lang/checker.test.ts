import { describe, expect, it } from "vitest";
import { checkSophiaFiles } from "../../src/lang/checker.js";

describe("checkSophiaFiles", () => {
  it("rejects unsupported var and direct Console.Write calls", () => {
    const result = checkSophiaFiles({
      "domains/RabbitDomain/capabilities/RabbitConsoleCapability.sophia": `
capability RabbitConsoleCapability {
  allow { Console.Write }
  deny { }
}
`,
      "domains/RabbitDomain/actions/PrintFirstTenRabbitNumbers.sophia": `
action PrintFirstTenRabbitNumbers {
  capability: RabbitConsoleCapability
  output { result: List<Int> }
  effects { Console.Write }
  body {
    var numbers = []
    Console.Write(1)
    return numbers
  }
}
`,
    });

    expect(result.ok).toBe(false);
    expect(result.diagnostics.map((diagnostic) => diagnostic.code)).toEqual(
      expect.arrayContaining(["CHECK-SYNTAX-006", "CHECK-BODY-002"]),
    );
  });

  it("accepts a minimal print action", () => {
    const result = checkSophiaFiles({
      "domains/Demo/capabilities/ConsoleCapability.sophia": `
capability ConsoleCapability {
  allow { Console.Write }
  deny { }
}
`,
      "domains/Demo/actions/HelloWorld.sophia": `
action HelloWorld {
  capability: ConsoleCapability
  output { result: Unit }
  effects { Console.Write }
  body {
    print "Hello, Sophia!"
    return unit
  }
}
`,
    });

    expect(result.ok).toBe(true);
  });

  it("allows explicit Int-to-Text conversion at the Console boundary", () => {
    const result = checkSophiaFiles({
      "domains/Demo/capabilities/ConsoleCapability.sophia": `
capability ConsoleCapability {
  allow { Console.Write }
}
`,
      "domains/Demo/actions/PrintCount.sophia": `
action PrintCount {
  capability: ConsoleCapability
  input { count: Int }
  output { result: Unit }
  effects { Console.Write }
  body {
    print to_text(count)
    return unit
  }
}
`,
    });

    expect(result.ok).toBe(true);
  });

  it("still rejects raw Int output at the Console boundary", () => {
    const result = checkSophiaFiles({
      "domains/Demo/capabilities/ConsoleCapability.sophia": `
capability ConsoleCapability {
  allow { Console.Write }
}
`,
      "domains/Demo/actions/PrintCount.sophia": `
action PrintCount {
  capability: ConsoleCapability
  input { count: Int }
  output { result: Unit }
  effects { Console.Write }
  body {
    print count
    return unit
  }
}
`,
    });

    expect(result.ok).toBe(false);
    expect(result.diagnostics.map((diagnostic) => diagnostic.code)).toContain(
      "CHECK-INTENT-BOUNDARY-001",
    );
  });

  it("does not treat natural-language meaning text as unsupported syntax", () => {
    const result = checkSophiaFiles({
      "domains/Demo/capabilities/PureCapability.sophia": `
capability PureCapability {
  allow { }
}
`,
      "domains/Demo/actions/DescribeLoop.sophia": `
action DescribeLoop {
  meaning: "Returns a value while describing why a for loop or var declaration is not used."
  capability: PureCapability
  output { result: Int }
  effects { }
  body {
    return 1
  }
}
`,
    });

    expect(result.ok).toBe(true);
  });

  it("accepts action calls when inputs, output type, and effects line up", () => {
    const result = checkSophiaFiles({
      "domains/Demo/capabilities/PureCapability.sophia": `
capability PureCapability {
  allow { }
}
`,
      "domains/Demo/actions/IsPositive.sophia": `
action IsPositive {
  capability: PureCapability
  input { amount: Int }
  output { result: Bool }
  effects { }
  body {
    return amount > 0
  }
}
`,
      "domains/Demo/actions/ApplyPositive.sophia": `
action ApplyPositive {
  capability: PureCapability
  input { amount: Int }
  output { result: Int }
  effects { }
  body {
    let allowed = IsPositive { amount = amount }
    if allowed {
      return amount * 2
    } else {
      return amount
    }
  }
}
`,
    });

    expect(result.ok).toBe(true);
  });

  it("rejects action calls written with an unsupported call keyword", () => {
    const result = checkSophiaFiles({
      "domains/Demo/capabilities/PureCapability.sophia": `
capability PureCapability {
  allow { }
}
`,
      "domains/Demo/actions/IsPositive.sophia": `
action IsPositive {
  capability: PureCapability
  input { amount: Int }
  output { result: Bool }
  effects { }
  body {
    return amount > 0
  }
}
`,
      "domains/Demo/actions/ApplyPositive.sophia": `
action ApplyPositive {
  capability: PureCapability
  input { amount: Int }
  output { result: Int }
  effects { }
  body {
    let allowed = call IsPositive { amount = amount }
    if allowed {
      return amount * 2
    } else {
      return amount
    }
  }
}
`,
    });

    expect(result.ok).toBe(false);
    expect(result.diagnostics.map((diagnostic) => diagnostic.code)).toContain("CHECK-SYNTAX-009");
    expect(result.diagnostics.map((diagnostic) => diagnostic.problem)).toContain(
      "Sophia v0 action expressions do not use a call keyword.",
    );
  });

  it("rejects mutual action call recursion", () => {
    const result = checkSophiaFiles({
      "domains/Demo/capabilities/PureCapability.sophia": `
capability PureCapability {
  allow { }
}
`,
      "domains/Demo/actions/FirstAction.sophia": `
action FirstAction {
  capability: PureCapability
  input { value: Int }
  output { result: Int }
  effects { }
  body {
    return SecondAction { value = value }
  }
}
`,
      "domains/Demo/actions/SecondAction.sophia": `
action SecondAction {
  capability: PureCapability
  input { value: Int }
  output { result: Int }
  effects { }
  body {
    return FirstAction { value = value }
  }
}
`,
    });

    expect(result.ok).toBe(false);
    expect(result.diagnostics.map((diagnostic) => diagnostic.code)).toContain(
      "CHECK-ACTION-CALL-007",
    );
    expect(result.diagnostics.map((diagnostic) => diagnostic.problem)).toContain(
      "Recursive action call cycle is not supported in v0: FirstAction -> SecondAction -> FirstAction.",
    );
  });

  it("accepts declared error variants and raise statements", () => {
    const result = checkSophiaFiles({
      "domains/Demo/capabilities/PureCapability.sophia": `
capability PureCapability {
  allow { }
}
`,
      "domains/Demo/errors/AccountError.sophia": `
error AccountError {
  variant InvalidAmount {
    amount: Int
  }
}
`,
      "domains/Demo/actions/ValidateAmount.sophia": `
action ValidateAmount {
  capability: PureCapability
  input { amount: Int }
  output { result: Int }
  effects { }
  errors { InvalidAmount }
  body {
    if amount <= 0 {
      raise InvalidAmount { amount = amount }
    }
    return amount
  }
}
`,
    });

    expect(result.ok).toBe(true);
  });

  it("accepts declared state types and state value expressions", () => {
    const result = checkSophiaFiles({
      "domains/Demo/capabilities/PureCapability.sophia": `
capability PureCapability {
  allow { }
}
`,
      "domains/Demo/states/TodoStatus.sophia": `
state TodoStatus {
  value Pending { }
  value Done { }
}
`,
      "domains/Demo/entities/Todo.sophia": `
entity Todo {
  fields {
    status: TodoStatus
  }
}
`,
      "domains/Demo/actions/CompleteTodo.sophia": `
action CompleteTodo {
  capability: PureCapability
  input { todo: Todo }
  output { result: Todo }
  effects { }
  body {
    if todo.status == TodoStatus.Done {
      return todo
    } else {
      return Todo { status = TodoStatus.Done }
    }
  }
}
`,
    });

    expect(result.ok).toBe(true);
  });

  it("rejects unknown state values in typed expressions", () => {
    const result = checkSophiaFiles({
      "domains/Demo/capabilities/PureCapability.sophia": `
capability PureCapability {
  allow { }
}
`,
      "domains/Demo/states/TodoStatus.sophia": `
state TodoStatus {
  value Pending { }
}
`,
      "domains/Demo/actions/ReturnStatus.sophia": `
action ReturnStatus {
  capability: PureCapability
  output { result: TodoStatus }
  effects { }
  body {
    return TodoStatus.Done
  }
}
`,
    });

    expect(result.ok).toBe(false);
    expect(result.diagnostics.map((diagnostic) => diagnostic.code)).toContain("CHECK-RETURN-001");
  });

  it("accepts exhaustive match over state values", () => {
    const result = checkSophiaFiles({
      "domains/Demo/capabilities/PureCapability.sophia": `
capability PureCapability {
  allow { }
}
`,
      "domains/Demo/states/TodoStatus.sophia": `
state TodoStatus {
  value Pending { }
  value Done { }
}
`,
      "domains/Demo/entities/Todo.sophia": `
entity Todo {
  fields {
    status: TodoStatus
  }
}
`,
      "domains/Demo/actions/StatusLabel.sophia": `
action StatusLabel {
  capability: PureCapability
  input { todo: Todo }
  output { result: Text }
  effects { }
  body {
    match todo.status {
      TodoStatus.Pending {
        return "pending"
      }
      TodoStatus.Done {
        return "done"
      }
    }
  }
}
`,
    });

    expect(result.ok).toBe(true);
  });

  it("accepts exhaustive match over Optional values with Some binding", () => {
    const result = checkSophiaFiles({
      "domains/Demo/capabilities/PureCapability.sophia": `
capability PureCapability {
  allow { }
}
`,
      "domains/Demo/actions/LabelOrDefault.sophia": `
action LabelOrDefault {
  capability: PureCapability
  input { label: Optional<Text> }
  output { result: Text }
  effects { }
  body {
    match label {
      Some(value) {
        return value
      }
      None {
        return "missing"
      }
    }
  }
}
`,
    });

    expect(result.ok).toBe(true);
  });

  it("rejects non-exhaustive or mistyped match cases", () => {
    const result = checkSophiaFiles({
      "domains/Demo/capabilities/PureCapability.sophia": `
capability PureCapability {
  allow { }
}
`,
      "domains/Demo/actions/BrokenMatch.sophia": `
action BrokenMatch {
  capability: PureCapability
  input { value: Bool }
  output { result: Text }
  effects { }
  body {
    match value {
      true {
        return "yes"
      }
      None {
        return "none"
      }
    }
  }
}
`,
    });

    expect(result.ok).toBe(false);
    expect(result.diagnostics.map((diagnostic) => diagnostic.code)).toEqual(
      expect.arrayContaining(["CHECK-MATCH-003", "CHECK-MATCH-005"]),
    );
  });

  it("rejects undeclared or malformed error raises", () => {
    const result = checkSophiaFiles({
      "domains/Demo/capabilities/PureCapability.sophia": `
capability PureCapability {
  allow { }
}
`,
      "domains/Demo/errors/AccountError.sophia": `
error AccountError {
  variant InvalidAmount {
    amount: Int
  }
}
`,
      "domains/Demo/actions/ValidateAmount.sophia": `
action ValidateAmount {
  capability: PureCapability
  input { label: Text }
  output { result: Text }
  effects { }
  errors { }
  body {
    raise InvalidAmount { amount = label, extra = label }
    return label
  }
}
`,
    });

    expect(result.ok).toBe(false);
    expect(result.diagnostics.map((diagnostic) => diagnostic.code)).toEqual(
      expect.arrayContaining(["CHECK-ERROR-005", "CHECK-ERROR-007", "CHECK-TYPE-002"]),
    );
    expect(result.diagnostics.map((diagnostic) => diagnostic.problem)).toEqual(
      expect.arrayContaining([
        "Action ValidateAmount raises InvalidAmount, but does not declare it in errors.",
        "Raise InvalidAmount uses unknown field extra.",
        "Error field InvalidAmount.amount expects Int, got Text.",
      ]),
    );
  });

  it("requires callers to declare called action error variants", () => {
    const result = checkSophiaFiles({
      "domains/Demo/capabilities/PureCapability.sophia": `
capability PureCapability {
  allow { }
}
`,
      "domains/Demo/errors/AccountError.sophia": `
error AccountError {
  variant InvalidAmount {
    amount: Int
  }
}
`,
      "domains/Demo/actions/ValidateAmount.sophia": `
action ValidateAmount {
  capability: PureCapability
  input { amount: Int }
  output { result: Int }
  effects { }
  errors { InvalidAmount }
  body {
    if amount <= 0 {
      raise InvalidAmount { amount = amount }
    }
    return amount
  }
}
`,
      "domains/Demo/actions/UseAmount.sophia": `
action UseAmount {
  capability: PureCapability
  input { amount: Int }
  output { result: Int }
  effects { }
  errors { }
  body {
    return ValidateAmount { amount = amount }
  }
}
`,
    });

    expect(result.ok).toBe(false);
    expect(result.diagnostics.map((diagnostic) => diagnostic.code)).toContain(
      "CHECK-ACTION-CALL-008",
    );
    expect(result.diagnostics.map((diagnostic) => diagnostic.problem)).toContain(
      "Action UseAmount calls ValidateAmount, but does not declare called error InvalidAmount.",
    );
  });

  it("rejects typed or uninitialized local variable declarations with specific diagnostics", () => {
    const result = checkSophiaFiles({
      "domains/Demo/capabilities/PureCapability.sophia": `
capability PureCapability {
  allow { }
}
`,
      "domains/Demo/actions/TypedLocals.sophia": `
action TypedLocals {
  capability: PureCapability
  input { value: Int }
  output { result: Int }
  effects { }
  body {
    let doubled: Int = value * 2
    let mutable result: Int
    set result = doubled
    return result
  }
}
`,
    });

    expect(result.ok).toBe(false);
    expect(result.diagnostics.map((diagnostic) => diagnostic.code)).toEqual(
      expect.arrayContaining(["CHECK-SYNTAX-010", "CHECK-SYNTAX-011"]),
    );
  });

  it("rejects pseudo-style empty List expressions with a specific diagnostic", () => {
    const result = checkSophiaFiles({
      "domains/Demo/capabilities/PureCapability.sophia": `
capability PureCapability {
  allow { }
}
`,
      "domains/Demo/actions/BuildValues.sophia": `
action BuildValues {
  capability: PureCapability
  output { result: List<Int> }
  effects { }
  body {
    let mutable values = empty List<Int>
    return values
  }
}
`,
    });

    expect(result.ok).toBe(false);
    expect(result.diagnostics.map((diagnostic) => diagnostic.code)).toContain("CHECK-SYNTAX-012");
  });

  it("rejects Unit type and bare return forms with specific diagnostics", () => {
    const result = checkSophiaFiles({
      "domains/Demo/capabilities/ConsoleCapability.sophia": `
capability ConsoleCapability {
  allow { Console.Write }
}
`,
      "domains/Demo/actions/ReturnUpperUnit.sophia": `
action ReturnUpperUnit {
  capability: ConsoleCapability
  output { result: Unit }
  effects { Console.Write }
  body {
    print "x"
    return Unit
  }
}
`,
      "domains/Demo/actions/BareReturn.sophia": `
action BareReturn {
  capability: ConsoleCapability
  output { result: Unit }
  effects { Console.Write }
  body {
    print "x"
    return
  }
}
`,
    });

    expect(result.ok).toBe(false);
    expect(result.diagnostics.map((diagnostic) => diagnostic.code)).toEqual(
      expect.arrayContaining(["CHECK-SYNTAX-013", "CHECK-SYNTAX-014"]),
    );
  });

  it("rejects bare action call statements and unsupported static conversion helpers", () => {
    const result = checkSophiaFiles({
      "domains/Demo/capabilities/ConsoleCapability.sophia": `
capability ConsoleCapability {
  allow { Console.Write }
}
`,
      "domains/Demo/actions/PrintValue.sophia": `
action PrintValue {
  capability: ConsoleCapability
  input { value: Int }
  output { result: Unit }
  effects { Console.Write }
  body {
    let label = Int.toText(value)
    print label
    return unit
  }
}
`,
      "domains/Demo/actions/Caller.sophia": `
action Caller {
  capability: ConsoleCapability
  input { value: Int }
  output { result: Unit }
  effects { Console.Write }
  body {
    PrintValue { value = value }
    return unit
  }
}
`,
    });

    expect(result.ok).toBe(false);
    expect(result.diagnostics.map((diagnostic) => diagnostic.code)).toEqual(
      expect.arrayContaining(["CHECK-SYNTAX-015", "CHECK-SYNTAX-016"]),
    );
  });

  it("rejects action calls with missing, unknown, or mistyped inputs", () => {
    const result = checkSophiaFiles({
      "domains/Demo/capabilities/PureCapability.sophia": `
capability PureCapability {
  allow { }
}
`,
      "domains/Demo/actions/DoubleInput.sophia": `
action DoubleInput {
  capability: PureCapability
  input { count: Int }
  output { result: Int }
  effects { }
  body {
    return count * 2
  }
}
`,
      "domains/Demo/actions/CallBroken.sophia": `
action CallBroken {
  capability: PureCapability
  input { label: Text }
  output { result: Int }
  effects { }
  body {
    let doubled = DoubleInput { label = label }
    return doubled
  }
}
`,
    });

    expect(result.ok).toBe(false);
    expect(result.diagnostics.map((diagnostic) => diagnostic.code)).toEqual(
      expect.arrayContaining(["CHECK-ACTION-CALL-003", "CHECK-ACTION-CALL-004"]),
    );
  });

  it("rejects callers that do not declare called action effects", () => {
    const result = checkSophiaFiles({
      "domains/Demo/capabilities/ConsoleCapability.sophia": `
capability ConsoleCapability {
  allow { Console.Write }
}
`,
      "domains/Demo/actions/PrintLabel.sophia": `
action PrintLabel {
  capability: ConsoleCapability
  input { label: Text }
  output { result: Unit }
  effects { Console.Write }
  body {
    print label
    return unit
  }
}
`,
      "domains/Demo/actions/CallPrint.sophia": `
action CallPrint {
  capability: ConsoleCapability
  input { label: Text }
  output { result: Unit }
  effects { }
  body {
    return PrintLabel { label = label }
  }
}
`,
    });

    expect(result.ok).toBe(false);
    expect(result.diagnostics.map((diagnostic) => diagnostic.code)).toContain(
      "CHECK-ACTION-CALL-006",
    );
  });

  it("allows effectful actions to call pure helper actions without redeclaring Pure", () => {
    const result = checkSophiaFiles({
      "domains/Demo/capabilities/ConsoleCapability.sophia": `
capability ConsoleCapability {
  allow { Console.Write }
}
`,
      "domains/Demo/capabilities/PureCapability.sophia": `
capability PureCapability {
  allow { Pure }
}
`,
      "domains/Demo/actions/NormalizeLabel.sophia": `
action NormalizeLabel {
  capability: PureCapability
  input { label: Text }
  output { result: Text }
  effects { Pure }
  body {
    return label
  }
}
`,
      "domains/Demo/actions/PrintNormalized.sophia": `
action PrintNormalized {
  capability: ConsoleCapability
  input { label: Text }
  output { result: Unit }
  effects { Console.Write }
  body {
    let normalized = NormalizeLabel { label = label }
    print "done"
    return unit
  }
}
`,
    });

    expect(result.ok).toBe(true);
  });

  it("rejects unsupported function-style append", () => {
    const result = checkSophiaFiles({
      "domains/Demo/capabilities/ConsoleCapability.sophia": `
capability ConsoleCapability {
  allow { Console.Write }
  deny { }
}
`,
      "domains/Demo/actions/ListDemo.sophia": `
action ListDemo {
  capability: ConsoleCapability
  output { result: List<Int> }
  effects { Console.Write }
  body {
    let mutable numbers = []
    set numbers = append(numbers, 1)
    return numbers
  }
}
`,
    });

    expect(result.ok).toBe(false);
    expect(result.diagnostics.map((diagnostic) => diagnostic.code)).toContain("CHECK-BODY-003");
  });

  it("accepts supported list append method form", () => {
    const result = checkSophiaFiles({
      "domains/Demo/capabilities/ConsoleCapability.sophia": `
capability ConsoleCapability {
  allow { Console.Write }
}
`,
      "domains/Demo/actions/ListDemo.sophia": `
action ListDemo {
  capability: ConsoleCapability
  output { result: List<Int> }
  effects { Console.Write }
  body {
    let mutable numbers = []
    let item = 1
    set numbers = numbers.append(item)
    return numbers
  }
}
`,
    });

    expect(result.ok).toBe(true);
  });

  it("rejects returning an input variable with the wrong declared output type", () => {
    const result = checkSophiaFiles({
      "domains/Demo/capabilities/PureCapability.sophia": `
capability PureCapability {
  allow { }
}
`,
      "domains/Demo/actions/WrongReturn.sophia": `
action WrongReturn {
  capability: PureCapability
  input { title: Text }
  output { result: List<Int> }
  effects { }
  body {
    return title
  }
}
`,
    });

    expect(result.ok).toBe(false);
    expect(result.diagnostics.map((diagnostic) => diagnostic.code)).toContain("CHECK-RETURN-001");
  });

  it("rejects assigning a value with a different inferred type", () => {
    const result = checkSophiaFiles({
      "domains/Demo/capabilities/PureCapability.sophia": `
capability PureCapability {
  allow { }
}
`,
      "domains/Demo/actions/WrongAssignment.sophia": `
action WrongAssignment {
  capability: PureCapability
  output { result: Text }
  effects { }
  body {
    let mutable label = "pending"
    set label = 1
    return label
  }
}
`,
    });

    expect(result.ok).toBe(false);
    expect(result.diagnostics.map((diagnostic) => diagnostic.code)).toContain("CHECK-TYPE-002");
  });

  it("rejects invented body statements", () => {
    const result = checkSophiaFiles({
      "domains/Demo/capabilities/ConsoleCapability.sophia": `
capability ConsoleCapability {
  allow { Console.Write }
  deny { }
}
`,
      "domains/Demo/actions/Demo.sophia": `
action Demo {
  capability: ConsoleCapability
  output { result: Unit }
  effects { Console.Write }
  body {
    push numbers 1
    return unit
  }
}
`,
    });

    expect(result.ok).toBe(false);
    expect(result.diagnostics.map((diagnostic) => diagnostic.code)).toContain("CHECK-BODY-004");
  });

  it("rejects print without declared effect", () => {
    const result = checkSophiaFiles({
      "domains/Demo/capabilities/ConsoleCapability.sophia": `
capability ConsoleCapability {
  allow { Console.Write }
  deny { }
}
`,
      "domains/Demo/actions/Demo.sophia": `
action Demo {
  capability: ConsoleCapability
  output { result: Unit }
  effects { }
  body {
    print "hello"
    return unit
  }
}
`,
    });

    expect(result.ok).toBe(false);
    expect(result.diagnostics.map((diagnostic) => diagnostic.code)).toContain("CHECK-EFFECT-001");
  });

  it("rejects effects not allowed by capability", () => {
    const result = checkSophiaFiles({
      "domains/Demo/capabilities/ConsoleCapability.sophia": `
capability ConsoleCapability {
  allow { }
  deny { }
}
`,
      "domains/Demo/actions/Demo.sophia": `
action Demo {
  capability: ConsoleCapability
  output { result: Unit }
  effects { Console.Write }
  body {
    print "hello"
    return unit
  }
}
`,
    });

    expect(result.ok).toBe(false);
    expect(result.diagnostics.map((diagnostic) => diagnostic.code)).toContain(
      "CHECK-CAPABILITY-002",
    );
  });

  it("rejects effects denied by capability even when allowed", () => {
    const result = checkSophiaFiles({
      "domains/Demo/capabilities/ConsoleCapability.sophia": `
capability ConsoleCapability {
  allow { Console.Write }
  deny { Console.Write }
}
`,
      "domains/Demo/actions/Demo.sophia": `
action Demo {
  capability: ConsoleCapability
  output { result: Unit }
  effects { Console.Write }
  body {
    print "hello"
    return unit
  }
}
`,
    });

    const codes = result.diagnostics.map((diagnostic) => diagnostic.code);
    expect(result.ok).toBe(false);
    expect(codes).toContain("CHECK-CAPABILITY-004");
    expect(codes).not.toContain("CHECK-CAPABILITY-002");
  });

  it("validates unsupported effects in capability deny blocks", () => {
    const result = checkSophiaFiles({
      "domains/Demo/capabilities/NetworkCapability.sophia": `
capability NetworkCapability {
  allow { Pure }
  deny { Network.Out("Api") }
}
`,
      "domains/Demo/actions/Demo.sophia": `
action Demo {
  capability: NetworkCapability
  output { result: Unit }
  effects { Pure }
  body {
    return unit
  }
}
`,
    });

    expect(result.ok).toBe(false);
    expect(result.diagnostics.map((diagnostic) => diagnostic.code)).toContain("CHECK-EFFECT-004");
  });

  it("rejects combining Pure with observable effects", () => {
    const result = checkSophiaFiles({
      "domains/Demo/capabilities/ConsoleCapability.sophia": `
capability ConsoleCapability {
  allow { Pure Console.Write }
}
`,
      "domains/Demo/actions/Demo.sophia": `
action Demo {
  capability: ConsoleCapability
  output { result: Unit }
  effects { Pure Console.Write }
  body {
    print "hello"
    return unit
  }
}
`,
    });

    expect(result.ok).toBe(false);
    expect(result.diagnostics.map((diagnostic) => diagnostic.code)).toContain("CHECK-EFFECT-002");
  });

  it("accepts simple if else body shape", () => {
    const result = checkSophiaFiles({
      "domains/Demo/capabilities/ConsoleCapability.sophia": `
capability ConsoleCapability {
  allow { Console.Write }
  deny { }
}
`,
      "domains/Demo/actions/BranchDemo.sophia": `
action BranchDemo {
  capability: ConsoleCapability
  input { count: Int }
  output { result: Unit }
  effects { Console.Write }
  body {
    if count == 0 {
      print "zero"
    } else {
      print "nonzero"
    }
    return unit
  }
}
`,
    });

    expect(result.ok).toBe(true);
  });

  it("accepts named Bool conditions and rejects non-Bool if conditions", () => {
    const accepted = checkSophiaFiles({
      "domains/Demo/capabilities/PureCapability.sophia": `
capability PureCapability {
  allow { }
}
`,
      "domains/Demo/actions/IsPositive.sophia": `
action IsPositive {
  capability: PureCapability
  input { count: Int }
  output { result: Bool }
  effects { }
  body {
    let is_positive = count > 0
    let is_small = count < 10
    if is_positive and is_small {
      return true
    } else {
      return false
    }
  }
}
`,
    });
    expect(accepted.ok).toBe(true);

    const rejected = checkSophiaFiles({
      "domains/Demo/capabilities/PureCapability.sophia": `
capability PureCapability {
  allow { }
}
`,
      "domains/Demo/actions/BadCondition.sophia": `
action BadCondition {
  capability: PureCapability
  input { count: Int }
  output { result: Int }
  effects { }
  body {
    if count {
      return 1
    } else {
      return 0
    }
  }
}
`,
    });
    expect(rejected.ok).toBe(false);
    expect(rejected.diagnostics.map((diagnostic) => diagnostic.code)).toContain("CHECK-TYPE-003");
  });

  it("accepts entity fields, field access, and complete entity construction", () => {
    const result = checkSophiaFiles({
      "domains/AccountDomain/domain.sophia": "domain AccountDomain { }\n",
      "domains/AccountDomain/entities/Account.sophia": `
entity Account {
  fields {
    balance: Int
    is_locked: Bool
  }
}
`,
      "domains/AccountDomain/capabilities/PureCapability.sophia": `
capability PureCapability {
  allow { }
}
`,
      "domains/AccountDomain/actions/Deposit.sophia": `
action Deposit {
  capability: PureCapability
  input {
    account: Account
    amount: Int
  }
  output { result: Account }
  effects { }
  body {
    let updated = Account { balance = account.balance + amount, is_locked = account.is_locked }
    return updated
  }
}
`,
    });

    expect(result.ok).toBe(true);
  });

  it("rejects incomplete or mistyped entity construction", () => {
    const result = checkSophiaFiles({
      "domains/AccountDomain/domain.sophia": "domain AccountDomain { }\n",
      "domains/AccountDomain/entities/Account.sophia": `
entity Account {
  fields {
    balance: Int
    is_locked: Bool
  }
}
`,
      "domains/AccountDomain/capabilities/PureCapability.sophia": `
capability PureCapability {
  allow { }
}
`,
      "domains/AccountDomain/actions/BadDeposit.sophia": `
action BadDeposit {
  capability: PureCapability
  input {
    account: Account
    amount: Int
  }
  output { result: Account }
  effects { }
  body {
    return Account { balance = account.is_locked, locked = false }
  }
}
`,
    });

    expect(result.ok).toBe(false);
    expect(result.diagnostics.map((diagnostic) => diagnostic.code)).toEqual(
      expect.arrayContaining(["CHECK-ENTITY-004", "CHECK-ENTITY-005", "CHECK-TYPE-002"]),
    );
  });

  it("rejects unsupported field types", () => {
    const result = checkSophiaFiles({
      "domains/Demo/capabilities/ConsoleCapability.sophia": `
capability ConsoleCapability {
  allow { Console.Write }
  deny { }
}
`,
      "domains/Demo/actions/Demo.sophia": `
action Demo {
  capability: ConsoleCapability
  input { id: Uuid }
  output { result: Unit }
  effects { Console.Write }
  body {
    print "hello"
    return unit
  }
}
`,
    });

    expect(result.ok).toBe(false);
    expect(result.diagnostics.map((diagnostic) => diagnostic.code)).toContain("CHECK-TYPE-001");
  });

  it("accepts intent-typed contracts only across explicit conversion actions", () => {
    const result = checkSophiaFiles({
      "domains/Demo/capabilities/PureCapability.sophia": `
capability PureCapability {
  allow { }
}
`,
      "domains/Demo/entities/Todo.sophia": `
entity Todo {
  fields {
    title: Sanitized<Text>
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
      "domains/Demo/actions/CreateTodo.sophia": `
action CreateTodo {
  capability: PureCapability
  input { title: Raw<Text> }
  output { result: Todo }
  effects { }
  body {
    let sanitized = SanitizeTitle { title = title }
    return Todo { title = sanitized }
  }
}
`,
    });

    expect(result.ok).toBe(true);
  });

  it("rejects Raw values where Sanitized intent is required", () => {
    const result = checkSophiaFiles({
      "domains/Demo/capabilities/PureCapability.sophia": `
capability PureCapability {
  allow { }
}
`,
      "domains/Demo/entities/Todo.sophia": `
entity Todo {
  fields {
    title: Sanitized<Text>
  }
}
`,
      "domains/Demo/actions/DisplayTitle.sophia": `
action DisplayTitle {
  capability: PureCapability
  input { title: Sanitized<Text> }
  output { result: Sanitized<Text> }
  effects { }
  body {
    return title
  }
}
`,
      "domains/Demo/actions/CreateTodo.sophia": `
action CreateTodo {
  capability: PureCapability
  input { title: Raw<Text> }
  output { result: Todo }
  effects { }
  body {
    let displayed = DisplayTitle { title = title }
    return Todo { title = title }
  }
}
`,
    });

    expect(result.ok).toBe(false);
    expect(result.diagnostics.map((diagnostic) => diagnostic.code)).toContain("CHECK-TYPE-002");
    expect(result.diagnostics.map((diagnostic) => diagnostic.problem)).toEqual(
      expect.arrayContaining([
        "Action call DisplayTitle.title expects Sanitized<Text>, got Raw<Text>.",
        "Entity field Todo.title expects Sanitized<Text>, got Raw<Text>.",
      ]),
    );
  });

  it("rejects malformed intent conversion actions", () => {
    const result = checkSophiaFiles({
      "domains/Demo/capabilities/ConsoleCapability.sophia": `
capability ConsoleCapability {
  allow { Console.Write }
}
`,
      "domains/Demo/actions/BadConversion.sophia": `
action BadConversion {
  capability: ConsoleCapability
  intent_conversion: true
  input { title: Raw<Text> }
  output { result: Sanitized<Int> }
  effects { Console.Write }
  body {
    print "converting"
    return title
  }
}
`,
    });

    expect(result.ok).toBe(false);
    expect(result.diagnostics.map((diagnostic) => diagnostic.code)).toEqual(
      expect.arrayContaining([
        "CHECK-INTENT-CONVERSION-002",
        "CHECK-INTENT-CONVERSION-003",
        "CHECK-INTENT-CONVERSION-004",
      ]),
    );
  });

  it("allows only sanitized or redacted intent values at Console.Write boundaries", () => {
    const accepted = checkSophiaFiles({
      "domains/Demo/capabilities/ConsoleCapability.sophia": `
capability ConsoleCapability {
  allow { Console.Write }
}
`,
      "domains/Demo/actions/PrintSafeTitle.sophia": `
action PrintSafeTitle {
  capability: ConsoleCapability
  input {
    title: Sanitized<Text>
    token: Redacted<Text>
  }
  output { result: Unit }
  effects { Console.Write }
  body {
    print title
    print token
    return unit
  }
}
`,
    });
    expect(accepted.ok).toBe(true);

    const rejected = checkSophiaFiles({
      "domains/Demo/capabilities/ConsoleCapability.sophia": `
capability ConsoleCapability {
  allow { Console.Write }
}
`,
      "domains/Demo/actions/PrintUnsafeTitle.sophia": `
action PrintUnsafeTitle {
  capability: ConsoleCapability
  input {
    title: Raw<Text>
    token: Secret<Text>
  }
  output { result: Unit }
  effects { Console.Write }
  body {
    print title
    print token
    return unit
  }
}
`,
    });

    expect(rejected.ok).toBe(false);
    expect(rejected.diagnostics.map((diagnostic) => diagnostic.code)).toEqual(
      expect.arrayContaining(["CHECK-INTENT-BOUNDARY-001"]),
    );
    expect(rejected.diagnostics.map((diagnostic) => diagnostic.problem)).toEqual(
      expect.arrayContaining([
        "Console.Write cannot output Raw<Text>; external output requires a literal, Sanitized<T>, or Redacted<T>.",
        "Console.Write cannot output Secret<Text>; external output requires a literal, Sanitized<T>, or Redacted<T>.",
      ]),
    );
  });

  it("checks storage declarations and DB.Write intent value compatibility", () => {
    const accepted = checkSophiaFiles({
      "domains/Demo/capabilities/StorageCapability.sophia": `
capability StorageCapability {
  allow { DB.Write("Todos") }
}
`,
      "domains/Demo/storages/Todos.sophia": `
storage Todos {
  key: Persisted<Text>
  value: Sanitized<Text>
}
`,
      "domains/Demo/actions/SaveTodoTitle.sophia": `
action SaveTodoTitle {
  capability: StorageCapability
  input { title: Sanitized<Text> }
  output { result: Sanitized<Text> }
  effects { DB.Write("Todos") }
  body {
    return title
  }
}
`,
    });
    expect(accepted.ok).toBe(true);

    const rejected = checkSophiaFiles({
      "domains/Demo/capabilities/StorageCapability.sophia": `
capability StorageCapability {
  allow { DB.Write("Todos") }
}
`,
      "domains/Demo/storages/Todos.sophia": `
storage Todos {
  key: Persisted<Text>
  value: Sanitized<Text>
}
`,
      "domains/Demo/actions/SaveRawTodoTitle.sophia": `
action SaveRawTodoTitle {
  capability: StorageCapability
  input { title: Raw<Text> }
  output { result: Raw<Text> }
  effects { DB.Write("Todos") }
  body {
    return title
  }
}
`,
    });

    expect(rejected.ok).toBe(false);
    expect(rejected.diagnostics.map((diagnostic) => diagnostic.code)).toContain(
      "CHECK-STORAGE-WRITE-002",
    );
    expect(rejected.diagnostics.map((diagnostic) => diagnostic.problem)).toContain(
      'Action SaveRawTodoTitle declares DB.Write("Todos"), but output type Raw<Text> does not match storage Todos value type Sanitized<Text>.',
    );
  });

  it("rejects malformed storage declarations and writes to unknown storage", () => {
    const result = checkSophiaFiles({
      "domains/Demo/capabilities/StorageCapability.sophia": `
capability StorageCapability {
  allow { DB.Write("Missing") }
}
`,
      "domains/Demo/storages/Todos.sophia": `
storage Todos {
  key: UnknownId
}
`,
      "domains/Demo/actions/SaveTodo.sophia": `
action SaveTodo {
  capability: StorageCapability
  output { result: Sanitized<Text> }
  effects { DB.Write("Missing") }
  body {
    return "safe"
  }
}
`,
    });

    expect(result.ok).toBe(false);
    expect(result.diagnostics.map((diagnostic) => diagnostic.code)).toEqual(
      expect.arrayContaining([
        "CHECK-STORAGE-002",
        "CHECK-TYPE-001",
        "CHECK-STORAGE-WRITE-001",
        "CHECK-RETURN-001",
      ]),
    );
  });

  it("checks DB.Read storage targets exist", () => {
    const accepted = checkSophiaFiles({
      "domains/Demo/capabilities/StorageCapability.sophia": `
capability StorageCapability {
  allow { DB.Read("Todos") }
}
`,
      "domains/Demo/storages/Todos.sophia": `
storage Todos {
  key: Persisted<Text>
  value: Sanitized<Text>
}
`,
      "domains/Demo/actions/LoadTodoTitle.sophia": `
action LoadTodoTitle {
  capability: StorageCapability
  output { result: Unit }
  effects { DB.Read("Todos") }
  body {
    return unit
  }
}
`,
    });
    expect(accepted.ok).toBe(true);

    const rejected = checkSophiaFiles({
      "domains/Demo/capabilities/StorageCapability.sophia": `
capability StorageCapability {
  allow { DB.Read("Missing") }
}
`,
      "domains/Demo/actions/LoadTodoTitle.sophia": `
action LoadTodoTitle {
  capability: StorageCapability
  output { result: Unit }
  effects { DB.Read("Missing") }
  body {
    return unit
  }
}
`,
    });

    expect(rejected.ok).toBe(false);
    expect(rejected.diagnostics.map((diagnostic) => diagnostic.code)).toContain(
      "CHECK-STORAGE-READ-001",
    );
    expect(rejected.diagnostics.map((diagnostic) => diagnostic.problem)).toContain(
      "Action LoadTodoTitle reads unknown storage: Missing.",
    );
  });

  it("rejects bare DB effects without a storage target", () => {
    const result = checkSophiaFiles({
      "domains/Demo/capabilities/StorageCapability.sophia": `
capability StorageCapability {
  allow { DB.Write }
}
`,
      "domains/Demo/actions/SaveTodo.sophia": `
action SaveTodo {
  capability: StorageCapability
  output { result: Sanitized<Text> }
  effects { DB.Write }
  body {
    return "safe"
  }
}
`,
    });

    expect(result.ok).toBe(false);
    expect(result.diagnostics.map((diagnostic) => diagnostic.code)).toContain("CHECK-EFFECT-003");
    expect(result.diagnostics.map((diagnostic) => diagnostic.problem)).toContain(
      "Effect DB.Write must name a storage target.",
    );
  });

  it("rejects effects outside the implemented v0 effect set", () => {
    const result = checkSophiaFiles({
      "domains/Demo/capabilities/NetworkCapability.sophia": `
capability NetworkCapability {
  allow { Network.Out("Api") }
}
`,
      "domains/Demo/actions/FetchRemote.sophia": `
action FetchRemote {
  capability: NetworkCapability
  output { result: Unit }
  effects { Network.Out("Api") }
  body {
    return unit
  }
}
`,
    });

    expect(result.ok).toBe(false);
    expect(result.diagnostics.map((diagnostic) => diagnostic.code)).toContain("CHECK-EFFECT-004");
    expect(result.diagnostics.map((diagnostic) => diagnostic.problem)).toContain(
      'Unsupported v0 effect: Network.Out("Api").',
    );
  });

  it("rejects missing action structure", () => {
    const result = checkSophiaFiles({
      "domains/Demo/actions/Demo.sophia": `
action Demo {
  output { result: Unit }
}
`,
    });

    expect(result.ok).toBe(false);
    expect(result.diagnostics.map((diagnostic) => diagnostic.code)).toEqual(
      expect.arrayContaining(["CHECK-ACTION-001", "CHECK-ACTION-002", "CHECK-ACTION-003"]),
    );
  });

  it("rejects assignment to immutable or undeclared variables", () => {
    const result = checkSophiaFiles({
      "domains/Demo/capabilities/ConsoleCapability.sophia": `
capability ConsoleCapability {
  allow { Console.Write }
  deny { }
}
`,
      "domains/Demo/actions/Demo.sophia": `
action Demo {
  capability: ConsoleCapability
  output { result: List<Int> }
  effects { Console.Write }
  body {
    let numbers = []
    set numbers = numbers + [next]
    return missing
  }
}
`,
    });

    expect(result.ok).toBe(false);
    expect(result.diagnostics.map((diagnostic) => diagnostic.code)).toEqual(
      expect.arrayContaining(["CHECK-VAR-001", "CHECK-VAR-003"]),
    );
  });

  it("rejects unsupported file layout and top-level blocks", () => {
    const result = checkSophiaFiles({
      "scratch/Demo.sophia": `
workflow Demo {
  body { return unit }
}
`,
    });

    expect(result.ok).toBe(false);
    expect(result.diagnostics.map((diagnostic) => diagnostic.code)).toEqual(
      expect.arrayContaining(["CHECK-FILE-001", "CHECK-SYNTAX-007"]),
    );
  });

  it("rejects duplicate actions and capabilities", () => {
    const result = checkSophiaFiles({
      "domains/Demo/capabilities/ConsoleCapability.sophia": `
capability ConsoleCapability { allow { Console.Write } }
capability ConsoleCapability { allow { Console.Write } }
`,
      "domains/Demo/actions/Demo.sophia": `
action Demo {
  capability: ConsoleCapability
  output { result: Unit }
  effects { Console.Write }
  body { return unit }
}
action Demo {
  capability: ConsoleCapability
  output { result: Unit }
  effects { Console.Write }
  body { return unit }
}
`,
    });

    expect(result.ok).toBe(false);
    expect(result.diagnostics.map((diagnostic) => diagnostic.code)).toEqual(
      expect.arrayContaining(["CHECK-ACTION-004", "CHECK-CAPABILITY-003"]),
    );
    expect(result.diagnostics.map((diagnostic) => diagnostic.code)).toContain("CHECK-FILE-004");
  });

  it("rejects shadowing between top-level ASG node names", () => {
    const result = checkSophiaFiles({
      "domains/Demo/domain.sophia": "domain Demo {}\n",
      "domains/Demo/capabilities/PureCapability.sophia":
        "capability PureCapability { allow { } }\n",
      "domains/Demo/entities/ProcessDeposit.sophia": `
entity ProcessDeposit {
  fields {
    amount: Int
  }
}
`,
      "domains/Demo/actions/ProcessDeposit.sophia": `
action ProcessDeposit {
  capability: PureCapability
  input { amount: Int }
  output { result: Int }
  effects { }
  body {
    return amount
  }
}
`,
    });

    expect(result.ok).toBe(false);
    expect(result.diagnostics.map((diagnostic) => diagnostic.code)).toContain("CHECK-NAME-002");
  });

  it("accepts Int and Text returns in v0", () => {
    const result = checkSophiaFiles({
      "domains/Demo/capabilities/PureCapability.sophia": `
capability PureCapability {
  allow { }
}
`,
      "domains/Demo/actions/ReturnCount.sophia": `
action ReturnCount {
  capability: PureCapability
  output { result: Int }
  effects { }
  body {
    let total = 15
    return total
  }
}
`,
      "domains/Demo/actions/ReturnLabel.sophia": `
action ReturnLabel {
  capability: PureCapability
  output { result: Text }
  effects { }
  body {
    return "ok"
  }
}
`,
    });

    expect(result.ok).toBe(true);
  });

  it("accepts Optional returns and entity fields", () => {
    const result = checkSophiaFiles({
      "domains/Demo/capabilities/PureCapability.sophia": `
capability PureCapability {
  allow { }
}
`,
      "domains/Demo/entities/MaybeLabel.sophia": `
entity MaybeLabel {
  fields {
    label: Optional<Text>
  }
}
`,
      "domains/Demo/actions/ReturnNone.sophia": `
action ReturnNone {
  capability: PureCapability
  output { result: Optional<Text> }
  effects { }
  body {
    return None
  }
}
`,
      "domains/Demo/actions/ReturnSome.sophia": `
action ReturnSome {
  capability: PureCapability
  input { label: Text }
  output { result: Optional<Text> }
  effects { }
  body {
    return Some(label)
  }
}
`,
      "domains/Demo/actions/BuildMaybeLabel.sophia": `
action BuildMaybeLabel {
  capability: PureCapability
  input { label: Text }
  output { result: MaybeLabel }
  effects { }
  body {
    return MaybeLabel { label = Some(label) }
  }
}
`,
    });

    expect(result.ok).toBe(true);
  });

  it("accepts Text concatenation without implicit conversion", () => {
    const accepted = checkSophiaFiles({
      "domains/Demo/capabilities/PureCapability.sophia": `
capability PureCapability {
  allow { }
}
`,
      "domains/Demo/actions/ConcatText.sophia": `
action ConcatText {
  capability: PureCapability
  input { left: Text right: Text }
  output { result: Text }
  effects { }
  body {
    return left + right
  }
}
`,
    });
    const rejected = checkSophiaFiles({
      "domains/Demo/capabilities/PureCapability.sophia": `
capability PureCapability {
  allow { }
}
`,
      "domains/Demo/actions/ConcatText.sophia": `
action ConcatText {
  capability: PureCapability
  input { left: Text count: Int }
  output { result: Text }
  effects { }
  body {
    return left + count
  }
}
`,
    });

    expect(accepted.ok).toBe(true);
    expect(rejected.ok).toBe(false);
    expect(rejected.diagnostics.map((diagnostic) => diagnostic.code)).toContain("CHECK-RETURN-001");
  });

  it("accepts List<Text> returns for tiny todo-list fixtures", () => {
    const result = checkSophiaFiles({
      "domains/Demo/capabilities/PureCapability.sophia": `
capability PureCapability {
  allow { }
}
`,
      "domains/Demo/actions/ReturnTexts.sophia": `
action ReturnTexts {
  capability: PureCapability
  output { result: List<Text> }
  effects { }
  body {
    let mutable items = []
    let text = "buy milk"
    set items = items + [text]
    return items
  }
}
`,
    });

    expect(result.ok).toBe(true);
  });

  it("rejects invalid Int and Text return shapes", () => {
    const result = checkSophiaFiles({
      "domains/Demo/capabilities/PureCapability.sophia": `
capability PureCapability {
  allow { }
}
`,
      "domains/Demo/actions/ReturnCount.sophia": `
action ReturnCount {
  capability: PureCapability
  output { result: Int }
  effects { }
  body {
    return "not an int"
  }
}
`,
      "domains/Demo/actions/ReturnLabel.sophia": `
action ReturnLabel {
  capability: PureCapability
  output { result: Text }
  effects { }
  body {
    return 3
  }
}
`,
    });

    expect(result.ok).toBe(false);
    expect(result.diagnostics.map((diagnostic) => diagnostic.code)).toEqual(
      expect.arrayContaining(["CHECK-RETURN-001"]),
    );
  });

  it("requires every non-Unit output path to return or raise", () => {
    const result = checkSophiaFiles({
      "domains/Demo/capabilities/PureCapability.sophia": `
capability PureCapability {
  allow { }
}
`,
      "domains/Demo/actions/MaybeReturn.sophia": `
action MaybeReturn {
  capability: PureCapability
  input { ok: Bool }
  output { result: Int }
  effects { }
  body {
    if ok {
      return 1
    }
  }
}
`,
    });

    expect(result.ok).toBe(false);
    expect(result.diagnostics.map((diagnostic) => diagnostic.code)).toContain("CHECK-RETURN-002");
  });

  it("accepts paths that terminate through return or declared raise", () => {
    const result = checkSophiaFiles({
      "domains/Demo/capabilities/PureCapability.sophia": `
capability PureCapability {
  allow { }
}
`,
      "domains/Demo/errors/DemoError.sophia": `
error DemoError {
  variant NotAllowed {
    reason: Text
  }
}
`,
      "domains/Demo/actions/ReturnOrRaise.sophia": `
action ReturnOrRaise {
  capability: PureCapability
  input { ok: Bool }
  output { result: Int }
  effects { }
  errors { NotAllowed }
  body {
    if ok {
      return 1
    } else {
      raise NotAllowed { reason = "blocked" }
    }
  }
}
`,
    });

    expect(result.ok).toBe(true);
  });

  it("does not leak variables declared inside if blocks", () => {
    const result = checkSophiaFiles({
      "domains/Demo/capabilities/PureCapability.sophia": `
capability PureCapability {
  allow { }
}
`,
      "domains/Demo/actions/BranchLocal.sophia": `
action BranchLocal {
  capability: PureCapability
  input { ok: Bool }
  output { result: Int }
  effects { }
  body {
    if ok {
      let branch_value = 1
    }
    return branch_value
  }
}
`,
    });

    expect(result.ok).toBe(false);
    expect(result.diagnostics.map((diagnostic) => diagnostic.problem)).toContain(
      "Identifier is not declared: branch_value.",
    );
  });

  it("rejects missing output blocks, multiple outputs, and mismatched returns", () => {
    const result = checkSophiaFiles({
      "domains/Demo/capabilities/PureCapability.sophia": `
capability PureCapability {
  allow { }
}
`,
      "domains/Demo/actions/MissingOutput.sophia": `
action MissingOutput {
  capability: PureCapability
  effects { }
  body {
    return unit
  }
}
`,
      "domains/Demo/actions/MultipleOutputs.sophia": `
action MultipleOutputs {
  capability: PureCapability
  output {
    result: Int
    label: Text
  }
  effects { }
  body {
    return "wrong"
  }
}
`,
    });

    expect(result.ok).toBe(false);
    expect(result.diagnostics.map((diagnostic) => diagnostic.code)).toEqual(
      expect.arrayContaining(["CHECK-ACTION-005", "CHECK-OUTPUT-001", "CHECK-RETURN-001"]),
    );
  });

  it("rejects orphan else and unclosed body blocks", () => {
    const result = checkSophiaFiles({
      "domains/Demo/capabilities/ConsoleCapability.sophia": `
capability ConsoleCapability {
  allow { Console.Write }
}
`,
      "domains/Demo/actions/OrphanElse.sophia": `
action OrphanElse {
  capability: ConsoleCapability
  output { result: Unit }
  effects { Console.Write }
  body {
    repeat 1 times {
      print "x"
    } else {
      print "y"
    }
    return unit
  }
}
`,
      "domains/Demo/actions/UnclosedBlock.sophia": `
action UnclosedBlock {
  capability: ConsoleCapability
  output { result: Unit }
  effects { Console.Write }
  body {
    if true {
      print "x"
    return unit
  }
}
`,
    });

    expect(result.ok).toBe(false);
    expect(result.diagnostics.map((diagnostic) => diagnostic.code)).toContain("CHECK-BLOCK-001");
  });

  it("rejects unbalanced braces", () => {
    const result = checkSophiaFiles({
      "domains/Demo/actions/Demo.sophia": `
action Demo {
  capability: DemoCapability
  output { result: Unit }
  effects { }
  body {
    return unit
  }
`,
    });

    expect(result.ok).toBe(false);
    expect(result.diagnostics.map((diagnostic) => diagnostic.code)).toContain("CHECK-SYNTAX-008");
  });

  it("rejects files whose path kind does not match the top-level node", () => {
    const result = checkSophiaFiles({
      "domains/Demo/actions/Demo.sophia": `
capability DemoCapability {
  allow { }
}
`,
    });

    expect(result.ok).toBe(false);
    expect(result.diagnostics.map((diagnostic) => diagnostic.code)).toContain("CHECK-FILE-005");
  });

  it("rejects non-PascalCase top-level names and paths", () => {
    const result = checkSophiaFiles({
      "domains/sum/domain.sophia": `
domain sum {
}
`,
      "domains/Demo/capabilities/compute.sophia": `
capability compute {
  allow { }
}
`,
    });

    expect(result.ok).toBe(false);
    expect(result.diagnostics.map((diagnostic) => diagnostic.code)).toEqual(
      expect.arrayContaining(["CHECK-FILE-001", "CHECK-NAME-001"]),
    );
  });

  it("rejects supported paths whose node name does not match the declaration", () => {
    const result = checkSophiaFiles({
      "domains/Demo/domain.sophia": `
domain OtherDomain {
}
`,
      "domains/Demo/actions/Run.sophia": `
action Execute {
  capability: DemoCapability
  output { result: Unit }
  effects { }
  body {
    return unit
  }
}
`,
      "domains/Demo/capabilities/DemoCapability.sophia": `
capability OtherCapability {
  allow { }
}
`,
    });

    expect(result.ok).toBe(false);
    expect(result.diagnostics.map((diagnostic) => diagnostic.code)).toEqual(
      expect.arrayContaining(["CHECK-FILE-006"]),
    );
  });

  it("rejects list equality as an emptiness check", () => {
    const result = checkSophiaFiles({
      "domains/DemoDomain/domain.sophia": "domain DemoDomain {}",
      "domains/DemoDomain/capabilities/DemoCapability.sophia":
        "capability DemoCapability { allow { Console.Write } }",
      "domains/DemoDomain/actions/BuildValues.sophia": `
action BuildValues {
  capability: DemoCapability
  input {
    first: Int
  }
  output {
    result: List<Int>
  }
  effects {
    Console.Write
  }
  errors { }
  body {
    let mutable result = []
    if result == [] {
      print "empty"
    }
    return result
  }
}
`,
    });

    expect(result.ok).toBe(false);
    expect(result.diagnostics.map((diagnostic) => diagnostic.code)).toContain("CHECK-BODY-005");
  });
});
