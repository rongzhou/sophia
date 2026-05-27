# Sophia Heuristic Workflow

This document defines Sophia's LLM programming workflow. It is not Sophia-Core language semantics; it is the protocol around exploration, generation, checking, repair, selection, and materialization.

Core boundary: exploration may be nondeterministic, but formal source and compilation must be deterministic. LLMs provide three non-replaceable heuristic abilities:

1. Generate structured `.pseudo`.
2. Implement `.pseudo` as checkable `.sophia` candidate source.
3. Choose the next graph action heuristically.

LLMs may also participate in goal analysis and repair. They do not judge correctness for `sophia check`, `build`, `run`, constraint audit, artifact diff, or materialize preflight.

## 1. Two Layers

| Layer                     | Nature                                 | Artifacts                                                                                         | Responsibility                                                                                |
| ------------------------- | -------------------------------------- | ------------------------------------------------------------------------------------------------- | --------------------------------------------------------------------------------------------- |
| Heuristic exploration     | nondeterministic, branchable, fallible | GoalNode, DecisionNode, PseudocodeNode, CodeNode, CheckResultNode, SelectionNode, MaterializeNode | Let the LLM propose candidates in a constrained space while preserving versions and failures. |
| Deterministic compilation | deterministic, reproducible, testable  | `.sophia`, ASG index, diagnostics, TypeScript artifact, runtime result                            | Parse, check, audit, generate, build, and run formal source.                                  |

Sophia is not "LLM directly writes final code"; it also does not abandon LLM programming ability in favor of deterministic rules. The aim is to extract LLM exploration from chat history into an auditable, replayable graph with deterministic context closure.

## 2. Two-Stage Programming

Local or code-weaker models often understand task semantics but fail when asked to emit formal code in one shot: missing effects, missing capabilities, invented syntax, inconsistent names, or natural language in body blocks.

Sophia uses two stages:

```text
user goal
  -> .pseudo          # structured pseudocode, not executable
  -> .sophia          # deterministic Sophia-Core candidate source
  -> sophia check     # deterministic, no LLM
  -> repair / revise  # new nodes, no history overwrite
  -> select
  -> materialize
  -> build / run
```

`.pseudo` is a semantic bridge from requirement to `.sophia`. It must describe steps, branches, loops, state updates, outputs, and constraints. It does not carry the full type system, capability/effect declarations, error algebra, formal body syntax, or Sophia syntax.

Iron rules:

- `.pseudo` guides `.sophia` generation but is never compiled or executed.
- `.sophia` is the only source entering checker, build, and runtime.
- `.pseudo -> .sophia` is LLM-assisted implementation, not compilation.
- If `.pseudo` lacks key logic, implementation must ask for revision rather than guess.
- If `.sophia` violates `.pseudo` intent, diagnostics or audit failure should be produced.

## 3. `.pseudo` Contract

`.pseudo` is structured pseudocode, not prose and not a custom programming language. JSON is acceptable as a carrier, but its content must remain algorithmic pseudocode, not Sophia code or a pseudo-DSL.

It must clarify:

- input and output semantics;
- record-like data semantics when needed;
- algorithm steps;
- loop counts or loop conditions;
- branch conditions;
- variable or state updates;
- logical step boundaries for validation/update/orchestration;
- side-effect intent such as printing;
- forbidden behavior;
- expected output or key acceptance criteria.

Minimal shape:

```json
{
  "purpose": "task goal",
  "inputs": [{ "name": "input_name", "meaning": "input semantics" }],
  "outputs": [{ "name": "result", "meaning": "output semantics" }],
  "definitions": [{ "name": "RecordLikeConcept", "meaning": "plain-language field shape" }],
  "algorithm": [
    "create empty list result",
    "repeat N times: compute next value and append it",
    "return result"
  ],
  "effects": ["observable output: print each value"],
  "constraints": ["semantic constraints to preserve"],
  "forbidden": ["behavior not allowed"]
}
```

Do not put these in `.pseudo`:

- full Sophia-Core type signatures;
- formal effect names such as `Console.Write`;
- scaffold contracts, paths, capability bindings, or implementation hints;
- full capability/error algebra;
- formal action body;
- `program { ... }`, `subaction { ... }`, or `main_flow { ... }` pseudo-DSL;
- vague steps such as "handle properly";
- hints that depend on hidden verifier expected outputs.

`pseudocode_check` does not prove correctness. It only judges whether `.pseudo` is clear enough to safely enter implementation.

## 4. Implementation Rules

`.pseudo -> .sophia` is done by the LLM and is therefore nondeterministic. The output must be a candidate file set, not chat text. Deterministic scaffold may fix paths, names, public overrides, explicit v0 type signatures, state/effect contracts, and verifiable structure. It must not infer business logic from keywords or generate the algorithm body.

Rules:

- Every algorithm step must become deterministic Sophia-Core statements.
- `repeat`, `if`, `return`, `print`, and related structures must become formal syntax; natural-language body text is invalid.
- inputs/outputs must receive `.sophia` types.
- semantic effect intent must become formal effects, such as print -> `Console.Write`.
- public scaffold contracts may require state files and values but must not generate the business `match` body.
- scaffold placeholders are not contracts; missing type hints require LLM semantic implementation and checker/verifier judgment.
- record-like `.pseudo definitions` should become formal `entity` files when needed.
- logical step boundaries should become helper actions when semantically meaningful.
- forbidden clauses should become capability deny rules, audit constraints, or implementation constraints.
- expected outputs should become tests/verifier/audit inputs, not program behavior.

After implementation, create a CodeNode and run graph check/verify immediately. A CodeNode that has not passed deterministic gates must not be materialized.

## 5. Development Graph

Sophia uses an append-only Development Graph:

```text
GoalNode
  -> DecisionNode
      -> PseudocodeNode
      -> CodeNode
      -> CheckResultNode
      -> RepairCode -> CodeNode(v+1)
      -> ReviseDesign -> PseudocodeNode(v+1)
      -> Backtrack -> ancestor / sibling node

Accepted CodeNode
  -> SelectionNode
  -> MaterializeNode
  -> domains/<Domain>/...
```

Rules:

- Nodes are immutable.
- Failures are kept and marked failed, abandoned, or superseded.
- `revise_design`, `repair_code`, and `merge` create new nodes.
- Selection is expressed by SelectionNode.
- Materialization is expressed by MaterializeNode.
- `domains/` contains only selected, gated formal source.
- `sophia-runs/graph/` stores exploration artifacts.

## 6. Nodes and Actions

Core nodes:

| Node            | Meaning                                                  |
| --------------- | -------------------------------------------------------- |
| GoalNode        | User goal or subgoal.                                    |
| DecisionNode    | Action choice, state assessment, rationale.              |
| PseudocodeNode  | Structured `.pseudo` version.                            |
| CodeNode        | Candidate `.sophia` file set.                            |
| CheckResultNode | Deterministic check result.                              |
| AuditNode       | Constraint/capability/artifact diff/strip-assist result. |
| SelectionNode   | Chosen candidate.                                        |
| MaterializeNode | Event writing candidate into formal source tree.         |

Core actions:

| Action             | Purpose                                      |
| ------------------ | -------------------------------------------- |
| `design_solution`  | Write structured `.pseudo`.                  |
| `implement_design` | Lower `.pseudo` into `.sophia`.              |
| `repair_code`      | Repair candidate source from diagnostics.    |
| `revise_design`    | Rewrite pseudocode when error is conceptual. |
| `decompose`        | Split a large goal.                          |
| `backtrack`        | Return from over-budget or invalid paths.    |
| `select`           | Select a candidate that passed gates.        |
| `materialize`      | Write selected candidate to `domains/`.      |

Action selection and execution are separate: a DecisionNode records allowed actions, scores, rationale, and chosen action; execution creates new graph nodes.

## 7. Decision Strategy

LLM decisions must be based on an action-space scaffold rather than free chat. The prompt includes node summary, ancestor chain, diagnostics, budgets, and action-rooted context. It must not include validation-only hidden expected output.

The deterministic action-space scaffold narrows safe actions and validates JSON shape; it does not choose for the LLM.

Heuristics:

1. Existing CodeNode with check/verify pass: `select`.
2. Existing CodeNode with local error and budget left: `repair_code`.
3. Existing CodeNode with conceptual error: `revise_design`.
4. Clear `.pseudo` and no CodeNode: `implement_design`.
5. No `.pseudo` and small/medium goal: `design_solution`.
6. Large/cross-domain goal: `decompose`.
7. Over budget or parent constraint violation: `backtrack`.

Dropping LLM node selection turns Sophia into a fixed pipeline executor and should not count as validating heuristic graph programming.

## 8. Budgets and Scoring

Exploration must avoid search explosion.

```text
budget {
  max_depth: 6
  max_children_per_node: 3
  max_repair_attempts_per_code_node: 2
  max_pseudocode_versions_per_goal: 3
  max_total_nodes_per_goal: 40
}
```

Candidate scoring may consider compile status, tests, constraints, simplicity, locality, capability minimality, and pseudocode clarity. If `compile = 0`, overall score must remain below selection thresholds.

## 9. Materialize Gate

`graph materialize` is the only graph command that can write candidate `.sophia` files into `domains/`.

Required:

- CodeNode selected by SelectionNode.
- Latest CheckResultNode passes.
- Constraint audit passes.
- Strip-assist / artifact diff gate passes.
- Candidate TypeScript build and strict typecheck preflight pass.
- Runtime validation and hidden verifier do not leak prompt data.

Materialization must be atomic: write a temporary directory, run preflight, then replace targets.

## 10. CLI Mapping

Common graph commands:

```bash
node dist/cli/main.js graph init
node dist/cli/main.js graph start "goal"
node dist/cli/main.js graph design N0001 --model qwen3.6:latest
node dist/cli/main.js graph pseudo-check fixtures/rabbit/rabbit.pseudo
node dist/cli/main.js graph pseudo-outline fixtures/rabbit/rabbit.pseudo
node dist/cli/main.js graph pseudo-scaffold fixtures/rabbit/rabbit.pseudo
node dist/cli/main.js graph implement-loop N0002 --model qwen3.6:latest --max-repairs 2
node dist/cli/main.js graph check N0005
node dist/cli/main.js graph audit N0005
node dist/cli/main.js graph diff N0005
node dist/cli/main.js graph verify N0005
node dist/cli/main.js graph select N0005
node dist/cli/main.js graph materialize N0005
```

Deterministic commands:

```bash
node dist/cli/main.js check
node dist/cli/main.js index
node dist/cli/main.js context --action ActionName
node dist/cli/main.js build
node dist/cli/main.js smoke
node dist/cli/main.js run ActionName
```

If Ollama is not running, LLM-dependent commands must fail explicitly and preserve failed artifacts.

## 11. Related Documents

- Current implementation status: `status.md`.
- Sophia-Core semantics: `sophia_language_design.md`.
- Roadmap: `roadmap.md`.
- Diagnostic conventions: `diagnostic_codes.md`.

Historical logs and plans are archived under `../archive/` and are not fact sources.
