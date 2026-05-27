---
title: Language Design
sidebar_label: Language Design
slug: /language-design
---

# Sophia Language Design

Sophia is an LLM-native deterministic semantic programming language for unattended LLM automatic programming.

The core question is: if an LLM has strong natural-language semantic understanding but weak code pretraining, can a language, checker, and workflow designed for LLMs let it make stable programming progress without human review as the correctness fallback?

Sophia's answer is to let the LLM handle semantic understanding, task decomposition, structured expression, and repair suggestions, while the language, compiler, and tools handle determinism, boundaries, types, side effects, errors, and capability constraints. LLMs may generate source, but source behavior is determined only by the formal language and compiler.

## 1. Core Positioning

Sophia is not natural-language programming and not a prompt DSL. It is a compilable language.

Natural language exists in Sophia as an assistive layer for LLM understanding, generation, and repair. It cannot determine program behavior, typechecking, IR implementation, or code generation.

| Layer           | Role                                                                                                                         | Determines program semantics or deterministic tool behavior |
| --------------- | ---------------------------------------------------------------------------------------------------------------------------- | ----------------------------------------------------------- |
| Formal Core     | `domain`, `entity`, `state`, `transition`, `error`, `capability`, `storage`, `action`, `task`, fields, bodies, effects, etc. | Yes                                                         |
| Semantic Assist | `meaning`, `purpose`, `not`, `because`, examples, anti-patterns, plans, repair notes                                         | No                                                          |

The compiler must support strip-assist equivalence: removing all Semantic Assist fields must not change Formal Core, IR, or codegen results.

In the complete design, `action`, `transition`, `entity`, `effect`, and `capability` affect runtime semantics or codegen. `task` does not affect runtime codegen, but it affects task closure, context, exclude checks, and LLM work boundaries. v0.2 implements only action-rooted semantic context, not top-level `task`.

## 2. Design Goals

Sophia targets low-code-pretraining or non-code-optimized LLMs. It prioritizes local semantic recovery, automatic repair, context trimming, and unattended constraint preservation. Human handwriting, reading, review, and maintenance convenience are not primary goals.

### 2.1 v0.2 Boundary

v0.2 is a committed, testable subset:

| Category                     | v0.2 committed scope                                                                                                                                                                                        |
| ---------------------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Compilable core              | `domain`, `entity`, `state`, `storage`, `error`, `capability`, `action`; `Optional<T>` / `Some` / `None`; `to_text(Int)`; restricted body with `match`; TypeScript backend; runtime input/output validation |
| Machine-checkable boundaries | Intent wrapper types, explicit conversion actions, Console / DB.Write intent boundaries, effect/capability/error/action-call checks, strip-assist codegen equivalence                                       |
| LLM semantic context         | `sophia context --action <ActionName>` generates a deterministic semantic neighborhood from an action root                                                                                                  |

Future design, not v0.2: `transition`, `task`, `context --task`, error handling/exhaustiveness, body-level storage operations, `requires` / `ensures`, invariants, `Result<T,E>`, `entity.with`, cross-domain boundaries, Semantic Identity, Evolution Boundary, and independent Sophia IR.

Current `match` is ordinary body branching over Bool, state, and Optional with explicit exhaustiveness. Sophia does not provide `_` catch-all.

Goals:

| Goal                          | Meaning                                                                                       |
| ----------------------------- | --------------------------------------------------------------------------------------------- |
| LLM-native                    | Syntax, diagnostics, context, and graph artifacts are designed primarily for LLM consumption. |
| Unattended automation         | Correctness, repair, and materialization gates do not depend on human review.                 |
| Local reasoning               | v0.2 gives the LLM action-rooted semantic context; future work extends this to task closure.  |
| Same source, same artifact    | Source, compiler, dependency, and target determine outputs.                                   |
| Explicit semantic state       | Types, states, errors, effects, and capability boundaries replace unreliable model memory.    |
| Automatic evolution stability | Tools monitor semantic drift during long-term iteration.                                      |

Non-goals:

- Human writing brevity.
- Human reading friendliness.
- Human review friendliness as a correctness dependency.
- Natural language deciding behavior.
- Compiler calls to LLMs.
- Complex generics, macros, reflection, dynamic eval, async, threads, distributed transactions in v0.
- Dynamic SQL, raw network, arbitrary filesystem, randomness, or complex runtime in v0.

## 3. Principles

| Principle                    | Requirement                                                                           |
| ---------------------------- | ------------------------------------------------------------------------------------- |
| LLM-native surface           | Syntax, diagnostics, context, and graph artifacts are for LLM/tool consumption first. |
| Unattended automation        | Safety and correctness gates do not rely on human review.                             |
| Deterministic core           | Executable behavior must be determined by formal syntax.                              |
| Natural language assist only | Natural language helps LLMs but does not affect compiled behavior.                    |
| Explicit expression          | Inputs, outputs, errors, effects, capabilities, states, and constraints are explicit. |
| Recoverable semantics        | Source blocks should let the LLM recover meaning from local context.                  |
| Filesystem ASG               | The semantic model is an ASG; v0 implements it with directories and files.            |
| Same source, same artifact   | Same source and target produce stable output.                                         |

The design philosophy is: turn everything the model would need to remember into something the program must express.

### 3.1 Feature Admission

A new feature must satisfy:

1. **LLM-consumable**: it enters deterministic context or reduces remembered/guessed state.
2. **Machine-checkable**: parser, checker, audit, runtime validation, or graph gate can check it.
3. **Closure-friendly**: dependencies form explicit ASG edges.
4. **Repair-guiding**: failures produce structured diagnostics.
5. **No human fallback**: correctness/safety/materialization cannot be delegated to review.
6. **No legacy convenience**: if the main benefit is human familiarity or brevity, reject by default.

## 4. Pseudocode and Formal Code Boundary

`.pseudo` bridges natural-language requirements and `.sophia`. It must be neither too close to formal code nor too vague.

`.pseudo` expresses solving logic, not compilable syntax. Pseudocode checks may require clear task intent, input/output semantics, loop counts, branch conditions, state updates, and effect intent. They must not require Sophia-Core syntax, expression syntax, or formal types. Translating pseudocode into compilable `.sophia` is the implementation stage.

| Content                | `.pseudo`                 | `.sophia`                 |
| ---------------------- | ------------------------- | ------------------------- |
| Task intent            | required                  | assistive `meaning` only  |
| Input/output semantics | required, may be informal | formal types required     |
| Algorithm steps        | required                  | formal body statements    |
| Loops/branches         | clear counts/conditions   | formal control structures |
| Effects                | semantic intent           | formal effects            |
| Errors                 | semantic branches         | error variants            |
| Capability boundary    | forbidden/needed intent   | capability allow/deny     |
| Executability          | never                     | yes                       |

The compiler scans only `.sophia`. `.pseudo` lives in graph artifacts or experiment inputs.

If `.pseudo` and `.sophia` disagree, `.sophia` is the program semantics; `.pseudo` is repair/audit material.

## 5. Filesystem ASG

Sophia's semantic model is an Abstract Semantic Graph. v0 implements it with domain-first file layout: one semantic node per file.

v0.2 accepts `domain`, `entity`, `state`, `storage`, `error`, `capability`, and `action`. `transition` and `task` are future design.

Sophia's top level is not an OOP class/member tree. Entity, Action, Capability, Error, Storage, and State are peer ASG nodes inside a domain. They connect by explicit references, not implicit ownership.

| Concept      | Role                                      | Must not do                           |
| ------------ | ----------------------------------------- | ------------------------------------- |
| `domain`     | namespace and aggregate boundary          | business execution logic              |
| `entity`     | domain concept, fields, semantic identity | IO or workflow ownership              |
| `transition` | pure state transition                     | storage/time/network/secret access    |
| `action`     | executable use case and runtime entry     | implicit authority                    |
| `capability` | effect sandbox                            | business algorithm                    |
| `error`      | closed error algebra                      | scattered string convention           |
| `storage`    | persistence abstraction                   | raw SQL or dynamic external resources |
| `task`       | LLM work unit and closure root            | runtime behavior                      |

Constraints:

- One formal top-level node per file.
- Node files live under the owning domain.
- v0 layout uses PascalCase domain directories and PascalCase node names.
- Top-level relationships form explicit ASG edges.
- No implicit imports.
- No same-scope shadowing.
- Cross-domain references must be explicit in future boundary syntax.
- `asg_index.json` is rebuildable cache, not semantic source.

## 6. Formal Core

Formal Core units are ASG nodes. Each node can be parsed and indexed independently; semantic checks happen over graph relationships.

Important graph edges:

| From     | Edge               | To                        | Meaning                                       |
| -------- | ------------------ | ------------------------- | --------------------------------------------- |
| `action` | `uses_type`        | entity/state/scalar       | input/output/body uses type                   |
| `action` | `binds_capability` | capability                | allowed effects policy                        |
| `action` | `declares_effect`  | effect                    | side effects action may perform               |
| `action` | `raises`           | error.variant             | declared domain errors                        |
| `action` | `reads/writes`     | storage                   | persistent abstraction access                 |
| `action` | `calls`            | transition/action         | reuse pure transitions or constrained actions |
| `entity` | `has_field`        | field                     | data structure                                |
| `task`   | `includes`         | ASG node                  | LLM work closure                              |
| `task`   | `excludes`         | capability/effect/storage | forbidden work boundary                       |

## 7. v0.2 Node and Body Summary

Implemented nodes:

- `domain`: domain boundary.
- `entity`: declared fields, field access, complete construction.
- `state`: closed set of values; used in exhaustive `match`.
- `storage`: storage value type and boundary metadata.
- `error`: variants and `raise` field checking.
- `capability`: allow/deny effect policy.
- `action`: executable body, input/output/effects/errors.

Implemented body subset:

- locals: `let`, `let mutable`, `set`;
- control: `if/else`, `repeat N times`, `match`;
- result: `return`, `raise`;
- effects: `print`;
- expressions: literals, variables, arithmetic, comparison, Boolean ops, Text/List concat, field access, entity construction, `Some`, `None`, action calls.

The subset is intentionally small. Additions must serve LLM-native automation rather than human convenience.

## 8. Strip-Assist

Semantic Assist fields help LLMs understand code but must not affect compiled behavior. The current implementation includes a TypeScript artifact equivalence gate. Future work should compare formal IR hashes as well.

## 9. v0.2 Limits

Not implemented:

- top-level `task`;
- top-level `transition`;
- body-level storage operations;
- error handling / exhaustive error matching;
- `requires` / `ensures` proofs;
- invariants;
- `entity.with`;
- cross-domain library protocols;
- evolution boundary enforcement;
- independent Sophia IR.

These omissions are deliberate. v0.2 prioritizes a stable, testable core over language expansion.
