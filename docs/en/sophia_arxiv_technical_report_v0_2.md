# Sophia: An LLM-Native Graph Programming Path Beyond Code Pretraining

**Technical Report v0.2**  
**Date: 2026-05-26**  
**Status: early technical report / working prototype report**

---

## Abstract

The dominant path for AI programming today is large-scale code pretraining. It is fast, effective, and clearly valuable: models absorb syntax, library conventions, project structure, debugging patterns, and common engineering choices from huge code corpora. But a shortcut is not necessarily the only path, nor necessarily the long-term best substrate. Sophia asks whether programming ability can be decomposed into semantic understanding, program representation, deterministic checks, context trimming, and heuristic node workflows rather than being understood mainly as code-distribution knowledge internalized in model parameters.

This report makes two claims. First, code pretraining is valuable but not absolutely required. A model with strong general semantic understanding and weaker code pretraining can perform useful automatic programming when supported by the right external programming substrate and workflow. Second, human-first programming language paradigms are not the natural endpoint for LLM programming. A graph programming language designed for LLM understanding, paired with a heuristic node workflow, is viable.

Sophia is a prototype of this path. It organizes programs as formal ASG nodes and an append-only development graph rather than primarily as linear source files for human reading. It descends from natural-language goals into structured `.pseudo`, then into deterministic, checkable Sophia-Core. It externalizes types, intent wrappers, capability/effect boundaries, error propagation, action-rooted context, strip-assist equivalence, constraint audit, and materialize gates as machine-checkable artifacts. LLMs handle semantic understanding, decomposition, structured expression, candidate source generation, and heuristic node choice; checkers, compilers, audits, builds, and runtime validation judge correctness.

The purpose of v0.2 is not to prove that Sophia beats direct TypeScript on benchmark success rate. The current benchmark run shows that the alternative path is executable, recordable, reproducible, and covers a minimal language surface including `Optional`, `state`, effects, entities, and cross-action pipelines. Sophia's long-term value is showing that programming ability need not come entirely from code pretraining; it can also come from program representations and engineering disciplines redesigned for LLMs.

---

## 1. Introduction

The strongest LLMs today can generate a website, an interactive component, or even a small Three.js game from a short natural-language request. It is tempting to infer that programming ability must be deeply internalized inside the model, and that stronger models will simply write traditional code more directly.

That impression has a real basis. Code pretraining is currently the fastest and most efficient path to AI programming competence. It lets models learn from TypeScript, Python, Java, READMEs, issues, patches, test suites, and debugging patterns. Sophia does not deny this value. Code pretraining has proven itself as a powerful shortcut.

But a shortcut is not the same as the only path. Code pretraining teaches models how humans program in languages and tools designed for humans. It assumes, implicitly, that existing programming languages, source organization, and engineering workflows are the natural substrate for programming. Sophia asks a different question:

> If the programming actor is an LLM, should human language paradigms remain the only substrate? Or can we design a graph programming language better suited to LLM understanding and manipulation, so that a general semantic model can gain programming ability through external tooling without first internalizing huge traditional code corpora?

The idea can be understood through a "smart intern" analogy. A person with strong reasoning ability but little programming experience is not useless merely because they have not memorized a large codebase. Given clear task decomposition, pseudocode, templates, compilers, type checks, tests, diagnostics, permission boundaries, and version management, they can gradually perform real work. Their capability depends not only on memorized code patterns but also on whether the environment provides enough decomposition, checking, feedback, and rollback.

Sophia applies this analogy to LLM programming. Code pretraining corresponds to an experienced engineer who has seen a lot of code. Sophia asks whether a semantically strong but code-weaker model can work with an external programming exoskeleton. That exoskeleton is not a prompt wrapper; it is an LLM-native graph programming substrate. Programs are organized as formal ASG nodes and append-only development graphs. Natural-language goals descend into structured `.pseudo`, then into checkable Sophia-Core. The LLM designs, implements, repairs, backtracks, selects, and materializes on the graph. Deterministic checkers, audits, compilers, and runtime validators act as judges.

Contributions:

1. A problem setting beyond code pretraining: code pretraining is a useful shortcut, but programming competence need not be fully internalized in model parameters.
2. Sophia, an LLM-native graph programming language and heuristic node workflow built around ASG nodes, `.pseudo`, Sophia-Core, and development graphs.
3. A v0.2 prototype with deterministic checker, TypeScript backend, runtime validation, action-rooted context, constraint audit, strip-assist equivalence, and graph repair/materialize gates.
4. Current benchmark records as evidence of executability and language-surface coverage, while explicitly not treating benchmark success rate as the core proof.

## 2. Core Positioning

Sophia is a prototype programming path beyond code pretraining. It separates:

- **Fluency in traditional code**: syntax, libraries, and conventions internalized from code corpora.
- **Ability to perform programming tasks**: requirement understanding, state modeling, boundary preservation, error propagation, capability control, versioned rollback, and repair from feedback.

The first kind of ability is well served by code pretraining. The second can be partially externalized: a semantic model handles understanding and local expression; the language and workflow handle structure, boundaries, checks, rollback, and repair gates.

Sophia is not:

- a DSL for comfortable human handwriting;
- a prompt DSL;
- natural-language programming;
- a coding agent wrapper around TypeScript;
- a framework whose value is algorithm benchmark wins over direct-ts.

Sophia turns LLM programming disciplines into formal structures:

- data history: intent wrappers;
- side-effect authority: capability/effect;
- current semantic neighborhood: action-rooted context;
- error declaration and propagation: minimal error algebra;
- natural-language assist neutrality: strip-assist equivalence;
- repair regression: artifact diff / constraint audit;
- replayable failures: append-only graph.

## 3. Design Principles

**LLM-native, not human-first.**  
Sophia does not optimize for human brevity, readability, or IDE habits. Syntax, nodes, diagnostics, and context are designed for LLM local semantic recovery, repair, context trimming, and constraint preservation.

**Natural language assists but does not define semantics.**  
`meaning`, `purpose`, `.pseudo`, and repair notes help the LLM but cannot affect runtime behavior. Executable semantics come from Formal Core only.

**Formal Core is deterministic.**  
The same `.sophia` source, compiler, and target must produce the same checks and generated artifacts. Compiler, checker, audit, and build never call an LLM.

**All boundaries are explicit.**  
Inputs, outputs, errors, side effects, capabilities, states, storage intent, and action calls must exist in formal structure rather than chat memory or naming convention.

**Graph, not chat history.**  
Design, implementation, check, repair, audit, selection, and materialization are stored in an append-only graph.

**Heuristic node workflow, not fixed pipeline.**  
The LLM is not only a body filler. It chooses graph actions: design, implement, repair, revise, backtrack, select, and materialize. Deterministic scaffold constrains the action space but does not replace node decision ability.

## 4. v0.2 Implementation Boundary

v0.2 implements:

- top-level nodes: `domain`, `entity`, `state`, `storage`, `error`, `capability`, `action`;
- types: `Unit`, `Bool`, `Int`, `Text`, `List<Int>`, `List<Text>`, `Optional<T>`, entity/state types, intent wrappers;
- body subset: `let`, `let mutable`, `set`, `return`, `raise`, `if/else`, `match`, `repeat N times`, `print`, entity construction, action calls;
- checks: layout, naming, type compatibility, local scope, return/raise paths, entity construction, action calls, effects, errors, recursion, intent boundaries, capability allow/deny, unsupported syntax;
- tooling: `.pseudo` checks, prompts, action-rooted context, append-only graph, TypeScript backend, runtime validation, hidden verifier, strip-assist equivalence, constraint audit, artifact diff.

Recent validation:

- `npm run typecheck` passes.
- `npm test` passes: 35 files, 295 tests.
- Markdown/JSON/YAML Prettier check passes.

## 5. `.pseudo` and `.sophia`

Sophia's two-stage flow separates semantic design from formal implementation.

`.pseudo` is structured pseudocode. It must clarify purpose, input/output semantics, algorithm steps, loops, branches, state updates, effect intent, forbidden behavior, and acceptance criteria. It does not carry formal types, formal effects, capabilities, file paths, or scaffold contracts.

`.sophia` is the only compilable source. It lowers `.pseudo` semantics into Formal Core: types, actions, capabilities, effects, errors, body statements, and ASG edges.

Implementation-stage prompts hide validation-only expected outputs. v0.2 also removed formal syntax pollution from design prompts: pseudocode generation should not see Sophia type/effect syntax, source paths, labels, or pseudo-DSL examples.

## 6. ASG and Action-Rooted Context

Sophia uses an Abstract Semantic Graph. v0.2 implements it with domain-first filesystem layout and one top-level node per file.

Tools compute a deterministic semantic neighborhood from an action root:

- current action;
- entity/state input/output types;
- bound capability;
- declared effects;
- called actions;
- propagated errors;
- storage / intent boundaries;
- relevant diagnostics.

The LLM does not need to read the whole repository or infer relevance from huge files. Context closure is generated by tools and is stable, sortable, reproducible, and testable.

## 7. Example Externalized Discipline: Intent Types

Intent Types illustrate how Sophia externalizes programming discipline. They move semantic history out of chat memory and naming convention into checker-enforced language facts.

v0.2 supports:

- intent-typed fields, action inputs/outputs, and storage values;
- strict assignability for `Raw<T>`, `Secret<T>`, `Sanitized<T>`, `Redacted<T>`, and related wrappers;
- explicit `intent_conversion: true` actions;
- action-call intent matching;
- Console boundary rejection for Raw/Secret;
- DB.Write storage value matching;
- capability `deny` overriding `allow`.

An adversarial proof direction is accept/reject matrices: TypeScript + tsc/tests may accept candidates that Sophia statically rejects, such as Raw-to-Sanitized storage writes or Secret-to-Console leaks.

## 8. Capability, Effect, and Error

Traditional TypeScript functions can use ambient authority if the runtime provides it. Sophia requires action effect declarations, capability bindings, allow/deny checks, effect propagation through action calls, declared error variants, and called-error propagation.

These mechanisms matter because unattended repair can otherwise introduce new side effects, remove error paths, or weaken boundaries while fixing local issues.

## 9. Strip-Assist Equivalence

Sophia allows natural-language assist fields because LLMs need them. They must not affect behavior.

Strip-assist equivalence says that removing Semantic Assist fields must not change Formal Core or generated artifacts. v0.2 implements TypeScript artifact equivalence; future work should compare IR hashes.

## 10. Development Graph

Sophia stores unattended programming as an append-only Development Graph, not chat history.

Current node types include GoalNode, DecisionNode, PseudocodeNode, PseudocodeCheckNode, CodeNode, CheckResultNode, AuditNode, ArtifactDiffNode, SelectionNode, MaterializeNode, and RawLlmNode.

Nodes are immutable. Failures are kept. Repair, revise, select, and materialize create new nodes or edges. This makes failures replayable, repairs auditable, LLM decisions analyzable, and future edit/evolution boundaries possible.

## 11. Benchmark Position

The v0.2 benchmark suite has 16 tasks across L1, L2, L3, and category_a. The latest JSONL records show:

| Model          | Mode              | Tasks | Final pass | Avg wall time |
| -------------- | ----------------- | ----: | ---------: | ------------: |
| qwen3.6:latest | Sophia full       |    16 |      16/16 |        235.6s |
| qwen3.6:latest | Direct TypeScript |    16 |      14/16 |        103.2s |

This table proves only that the workflow runs, records, and covers a minimal language surface. It must not be used as the core value proof.

The two direct-ts failures are useful diagnostic examples:

- `build_three_numbers`: typechecked and ran but returned four numbers instead of three.
- `optional_label_default`: typechecked and ran but confused present optional text with missing text.

The main proof should instead address whether code-weaker models can program through the Sophia substrate and whether graph language/workflow can provide a viable path beyond human-language source paradigms.

## 12. Related Work

**Coding agents.** SWE-agent, Aider, and related systems improve how LLMs use existing languages and tools. Sophia explores the complementary hypothesis that programming languages themselves may need to be designed for LLMs.

**Code pretraining.** Mainstream code LLMs show that large-scale code pretraining is powerful. Sophia proposes a complementary route: combine general semantic ability with externalized program structure.

**Flow engineering.** Multi-stage generation, test feedback, and repair loops are common. Sophia's difference is that the loop operates over typed, checkable, auditable Sophia-Core artifacts.

**Program synthesis / sketching / typed holes.** Sophia fixes semantic containers and lets the model fill constrained local bodies, but it does not perform exhaustive formal search or require full proofs.

**Effect systems / information-flow types.** Sophia's intent, capability, and effect mechanisms are related, but are tuned for LLM-native workflows and automatic repair gates.

## 13. Limits

v0.2 remains an early prototype:

- benchmarks are small and not the main value proof;
- `task`, `transition`, Evolution Boundary, Semantic Identity, and cross-domain libraries are not implemented;
- body-level storage operations and DB.Read runtime are not implemented;
- error handling/exhaustiveness is not implemented;
- intent wrappers are erased in generated TypeScript runtime shape;
- intent checks are still local rather than cross-domain/library dataflow;
- strip-assist currently compares TypeScript artifacts, not IR hash;
- graph mainly covers synthesis/repair, not edit transitions.

## 14. Future Work

- Intent safety adversarial suite.
- Edit transitions and Evolution Boundary.
- Cross-domain / library boundary.
- Stronger strip-assist equivalence through IR/formal hashes.

## 15. Conclusion

Sophia v0.2 is not valuable because it passed two more small tasks than direct TypeScript. That result only shows the prototype runs.

The real thesis has two layers. First, code pretraining is a valuable shortcut but not an absolute prerequisite for LLM programming ability; part of programming competence can be shared between a semantic model and external language/tooling. Second, human-first programming languages are not the only possible substrate; an LLM-native graph language and heuristic node workflow is viable.

Intent Types, capability/effect, error propagation, strip-assist equivalence, action-rooted context, and the development graph are the current minimal working pieces of that direction.
