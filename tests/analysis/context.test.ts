import { describe, expect, it } from "vitest";
import { buildActionContext } from "../../src/analysis/context.js";

describe("buildActionContext", () => {
  it("builds a stable action-rooted semantic closure", () => {
    const result = buildActionContext(
      {
        "domains/Demo/domain.sophia": "domain Demo {}\n",
        "domains/Demo/capabilities/StorageCapability.sophia": `
capability StorageCapability {
  allow { DB.Write("Todos") }
  deny { Console.Write }
}
`,
        "domains/Demo/entities/Todo.sophia": `
entity Todo {
  fields {
    title: Sanitized<Text>
    status: TodoStatus
  }
}
`,
        "domains/Demo/states/TodoStatus.sophia": `
state TodoStatus {
  value Pending { }
  value Done { }
}
`,
        "domains/Demo/storages/Todos.sophia": `
storage Todos {
  key: Persisted<Text>
  value: Todo
}
`,
        "domains/Demo/errors/TodoError.sophia": `
error TodoError {
  variant InvalidTitle {
    title: Raw<Text>
  }
}
`,
        "domains/Demo/actions/SanitizeTitle.sophia": `
action SanitizeTitle {
  capability: StorageCapability
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
  capability: StorageCapability
  input { title: Raw<Text> }
  output { result: Todo }
  effects { DB.Write("Todos") }
  errors { InvalidTitle }
  body {
    let safe_title = SanitizeTitle { title = title }
    return Todo { title = safe_title, status = TodoStatus.Pending }
  }
}
`,
      },
      "CreateTodo",
    );

    expect(result.ok).toBe(true);
    expect(result.diagnostics).toEqual([]);
    expect(result.files).toEqual([
      "domains/Demo/actions/CreateTodo.sophia",
      "domains/Demo/actions/SanitizeTitle.sophia",
      "domains/Demo/capabilities/StorageCapability.sophia",
      "domains/Demo/domain.sophia",
      "domains/Demo/entities/Todo.sophia",
      "domains/Demo/errors/TodoError.sophia",
      "domains/Demo/states/TodoStatus.sophia",
      "domains/Demo/storages/Todos.sophia",
    ]);
    expect(result.sources.map((source) => source.path)).toEqual(result.files);
    expect(result.sources.find((source) => source.path.endsWith("CreateTodo.sophia"))?.content)
      .toContain("action CreateTodo");
    expect(result.nodes.map((node) => `${node.kind}:${node.name}`)).toEqual([
      "Action:CreateTodo",
      "Action:SanitizeTitle",
      "Capability:StorageCapability",
      "Domain:Demo",
      "Entity:Todo",
      "Error:TodoError",
      "State:TodoStatus",
      "Storage:Todos",
    ]);
    expect(result.summary).toEqual({
      actions: ["CreateTodo", "SanitizeTitle"],
      capabilities: ["StorageCapability"],
      domains: ["Demo"],
      entities: ["Todo"],
      errors: ["TodoError"],
      states: ["TodoStatus"],
      storages: ["Todos"],
    });
    expect(result.edges).toEqual(
      expect.arrayContaining([
        {
          from: "CreateTodo",
          relation: "binds_capability",
          to: "StorageCapability",
          to_kind: "Capability",
        },
        {
          from: "CreateTodo",
          relation: "calls",
          to: "SanitizeTitle",
          to_kind: "Action",
        },
        {
          from: "CreateTodo",
          relation: "declares_effect",
          to: 'DB.Write("Todos")',
          to_kind: "Effect",
        },
        {
          from: "StorageCapability",
          relation: "allows_effect",
          to: 'DB.Write("Todos")',
          to_kind: "Effect",
        },
        {
          from: "StorageCapability",
          relation: "allows_effect",
          to: "Todos",
          to_kind: "Storage",
          detail: 'DB.Write("Todos")',
        },
        {
          from: "StorageCapability",
          relation: "denies_effect",
          to: "Console.Write",
          to_kind: "Effect",
        },
        {
          from: "CreateTodo",
          relation: "raises",
          to: "TodoError",
          to_kind: "Error",
          detail: "InvalidTitle",
        },
        {
          from: "CreateTodo",
          relation: "writes",
          to: "Todos",
          to_kind: "Storage",
          detail: 'DB.Write("Todos")',
        },
        {
          from: "CreateTodo",
          relation: "uses_type",
          to: "Todo",
          to_kind: "Entity",
          detail: "result",
        },
        {
          from: "Todo",
          relation: "uses_type",
          to: "TodoStatus",
          to_kind: "State",
          detail: "status",
        },
      ]),
    );
  });

  it("keeps the JSON contract stable for prompt consumers", () => {
    const result = buildActionContext(
      {
        "domains/Demo/domain.sophia": "domain Demo {}\n",
        "domains/Demo/capabilities/PureCapability.sophia": "capability PureCapability { allow { } }\n",
        "domains/Demo/actions/ReturnOne.sophia": `
action ReturnOne {
  capability: PureCapability
  output { result: Int }
  effects { }
  body {
    return 1
  }
}
`,
      },
      "ReturnOne",
    );

    expect(JSON.parse(JSON.stringify(result))).toEqual({
      ok: true,
      root: { kind: "Action", name: "ReturnOne" },
      files: [
        "domains/Demo/actions/ReturnOne.sophia",
        "domains/Demo/capabilities/PureCapability.sophia",
        "domains/Demo/domain.sophia",
      ],
      nodes: [
        {
          domain: "Demo",
          kind: "Action",
          name: "ReturnOne",
          path: "domains/Demo/actions/ReturnOne.sophia",
        },
        {
          domain: "Demo",
          kind: "Capability",
          name: "PureCapability",
          path: "domains/Demo/capabilities/PureCapability.sophia",
        },
        {
          domain: "Demo",
          kind: "Domain",
          name: "Demo",
          path: "domains/Demo/domain.sophia",
        },
      ],
      edges: [
        {
          from: "ReturnOne",
          relation: "binds_capability",
          to: "PureCapability",
          to_kind: "Capability",
        },
      ],
      sources: [
        {
          content: expect.stringContaining("action ReturnOne"),
          path: "domains/Demo/actions/ReturnOne.sophia",
        },
        {
          content: "capability PureCapability { allow { } }\n",
          path: "domains/Demo/capabilities/PureCapability.sophia",
        },
        {
          content: "domain Demo {}\n",
          path: "domains/Demo/domain.sophia",
        },
      ],
      summary: {
        actions: ["ReturnOne"],
        capabilities: ["PureCapability"],
        domains: ["Demo"],
        entities: [],
        errors: [],
        states: [],
        storages: [],
      },
      diagnostics: [],
    });
  });

  it("includes entity and state files referenced through Optional wrappers", () => {
    const result = buildActionContext(
      {
        "domains/Demo/domain.sophia": "domain Demo {}\n",
        "domains/Demo/capabilities/PureCapability.sophia": "capability PureCapability { allow { } }\n",
        "domains/Demo/states/TodoStatus.sophia": `
state TodoStatus {
  value Pending { }
  value Done { }
}
`,
        "domains/Demo/entities/Todo.sophia": `
entity Todo {
  fields {
    status: Optional<TodoStatus>
  }
}
`,
        "domains/Demo/actions/MaybeTodo.sophia": `
action MaybeTodo {
  capability: PureCapability
  output { result: Optional<Todo> }
  effects { }
  body {
    return None
  }
}
`,
      },
      "MaybeTodo",
    );

    expect(result.ok).toBe(true);
    expect(result.files).toEqual([
      "domains/Demo/actions/MaybeTodo.sophia",
      "domains/Demo/capabilities/PureCapability.sophia",
      "domains/Demo/domain.sophia",
      "domains/Demo/entities/Todo.sophia",
      "domains/Demo/states/TodoStatus.sophia",
    ]);
    expect(result.edges).toEqual(
      expect.arrayContaining([
        {
          from: "MaybeTodo",
          relation: "uses_type",
          to: "Todo",
          to_kind: "Entity",
          detail: "result",
        },
        {
          from: "Todo",
          relation: "uses_type",
          to: "TodoStatus",
          to_kind: "State",
          detail: "status",
        },
      ]),
    );
  });

  it("reports an unknown action root", () => {
    const result = buildActionContext({}, "MissingAction");

    expect(result.ok).toBe(false);
    expect(result.diagnostics).toEqual([
      {
        code: "CONTEXT-ACTION-001",
        severity: "error",
        location: "<context>",
        problem: "Unknown action: MissingAction.",
      },
    ]);
  });

  it("reports duplicate action roots without silently choosing one", () => {
    const result = buildActionContext(
      {
        "domains/One/actions/Demo.sophia": "action Demo {}\n",
        "domains/Two/actions/Demo.sophia": "action Demo {}\n",
      },
      "Demo",
    );

    expect(result.ok).toBe(false);
    expect(result.diagnostics).toContainEqual({
      code: "CONTEXT-ACTION-002",
      severity: "error",
      location: "domains/Two/actions/Demo.sophia",
      problem: "Duplicate action declaration: Demo.",
    });
  });
});
