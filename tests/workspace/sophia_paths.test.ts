import { describe, expect, it } from "vitest";
import {
  expectedTopLevelKindForPath,
  expectedTopLevelPathInfo,
  isSafeRelativeArtifactPath,
  isSupportedSophiaFilePath,
} from "../../src/workspace/sophia_paths.js";

describe("sophia path helpers", () => {
  it("recognizes the v0 domain/entity/action/capability layout", () => {
    expect(expectedTopLevelKindForPath("domains/Demo/domain.sophia")).toBe("domain");
    expect(expectedTopLevelKindForPath("domains/Demo/entities/Todo.sophia")).toBe("entity");
    expect(expectedTopLevelKindForPath("domains/Demo/actions/Run.sophia")).toBe("action");
    expect(expectedTopLevelKindForPath("domains/Demo/capabilities/RunCapability.sophia")).toBe(
      "capability",
    );
    expect(expectedTopLevelKindForPath("domains/Demo/storages/Todos.sophia")).toBe("storage");
    expect(expectedTopLevelKindForPath("domains/Demo/errors/DemoError.sophia")).toBe("error");
    expect(expectedTopLevelKindForPath("domains/Demo/states/TodoStatus.sophia")).toBe("state");
    expect(expectedTopLevelKindForPath("src/domains/Demo/actions/Run.sophia", "src/domains")).toBe(
      "action",
    );
    expect(isSupportedSophiaFilePath("domains/Demo/entities/Todo.sophia")).toBe(true);
    expect(isSupportedSophiaFilePath("domains/demo/domain.sophia")).toBe(false);
    expect(isSupportedSophiaFilePath("domains/Demo/actions/run.sophia")).toBe(false);
    expect(isSupportedSophiaFilePath("domains/Demo/capabilities/runCapability.sophia")).toBe(false);
    expect(isSupportedSophiaFilePath("scratch/Demo.sophia")).toBe(false);
  });

  it("extracts the expected top-level declaration name from supported paths", () => {
    expect(expectedTopLevelPathInfo("domains/Demo/domain.sophia")).toEqual({
      kind: "domain",
      name: "Demo",
    });
    expect(expectedTopLevelPathInfo("domains/Demo/actions/Run.sophia")).toEqual({
      kind: "action",
      name: "Run",
    });
    expect(expectedTopLevelPathInfo("domains/Demo/capabilities/RunCapability.sophia")).toEqual({
      kind: "capability",
      name: "RunCapability",
    });
    expect(expectedTopLevelPathInfo("domains/Demo/entities/Run.sophia")).toEqual({
      kind: "entity",
      name: "Run",
    });
    expect(expectedTopLevelPathInfo("domains/Demo/storages/Todos.sophia")).toEqual({
      kind: "storage",
      name: "Todos",
    });
    expect(expectedTopLevelPathInfo("domains/Demo/errors/DemoError.sophia")).toEqual({
      kind: "error",
      name: "DemoError",
    });
    expect(expectedTopLevelPathInfo("domains/Demo/states/TodoStatus.sophia")).toEqual({
      kind: "state",
      name: "TodoStatus",
    });
  });

  it("rejects unsafe artifact paths", () => {
    expect(isSafeRelativeArtifactPath("domains/Demo/domain.sophia")).toBe(true);
    expect(isSafeRelativeArtifactPath("../domains/Demo/domain.sophia")).toBe(false);
    expect(isSafeRelativeArtifactPath("/domains/Demo/domain.sophia")).toBe(false);
    expect(isSafeRelativeArtifactPath("domains\\Demo\\domain.sophia")).toBe(false);
  });
});
