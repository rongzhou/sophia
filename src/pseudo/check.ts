import type { CheckResult, Diagnostic } from "../lang/diagnostics.js";
import { error, warning } from "../lang/diagnostics.js";
import { extractNamedSection, readBraceBody } from "../lang/braces.js";
import type { SophiaField } from "../lang/types.js";
import { escapeRegExp } from "../util/strings.js";
import { hasPseudocodeSection, pseudocodeSection } from "./document.js";

export interface PseudocodeChecks {
  has_purpose: boolean;
  has_inputs: boolean;
  has_outputs: boolean;
  has_algorithm: boolean;
  has_expected: boolean;
  loop_details_explicit: boolean;
  state_updates_explicit: boolean;
  no_vague_steps: boolean;
}

export interface PseudocodeCheckResult extends CheckResult {
  checks: PseudocodeChecks;
}

const REQUIRED_SECTIONS = ["purpose", "inputs", "outputs", "algorithm"] as const;
const VAGUE_PATTERNS = [
  /\bhandle properly\b/i,
  /\bdo the calculation\b/i,
  /\bprocess safely\b/i,
  /\bcalculate sequence\b/i,
  /\bseveral times\b/i,
];

export function checkPseudocode(content: string): PseudocodeCheckResult {
  const diagnostics: Diagnostic[] = [];
  const sections = new Set([...content.matchAll(/^\s*([a-z_]+)\s*\{/gm)].map((match) => match[1]));

  for (const section of REQUIRED_SECTIONS) {
    if (!sections.has(section) && !hasPseudocodeSection(content, section)) {
      diagnostics.push(
        error(
          "PSEUDO-SECTION-001",
          undefined,
          `Missing required section: ${section}.`,
          `Add a ${section} { ... } section.`,
        ),
      );
    }
  }

  const algorithm =
    pseudocodeSection(content, "algorithm") ?? extractNamedSection(content, "algorithm") ?? "";
  const inputs = pseudocodeSection(content, "inputs") ?? extractNamedSection(content, "inputs") ?? "";
  const outputs =
    pseudocodeSection(content, "outputs") ?? extractNamedSection(content, "outputs") ?? "";
  const effects =
    pseudocodeSection(content, "effects") ?? extractNamedSection(content, "effects") ?? "";
  const hasPrint = /\bprint\b/.test(algorithm);
  if (hasPrint && !/\b(?:print|output|write|emit)\b/i.test(effects)) {
    diagnostics.push(
      warning(
        "PSEUDO-EFFECT-001",
        undefined,
        "The algorithm uses print, but the effects section does not describe the observable output intent.",
        "Keep .pseudo effect wording semantic, for example describe that the program prints or writes a value. The implementation stage owns formal effect declarations.",
      ),
    );
  }

  const outputFields = extractPseudoFields(outputs);
  if (outputFields.length !== 1) {
    diagnostics.push(
      warning(
        "PSEUDO-OUTPUT-001",
        undefined,
        `Pseudocode declares ${outputFields.length} output fields; the current v0 scaffold expects one action result field.`,
        "Keep the solving intent intact. During implementation, package multiple semantic outputs into one result value or ask for clarification if the goal requires multiple independent results.",
      ),
    );
  }

  if (/\brepeat\s+several\s+times\b/i.test(algorithm)) {
    diagnostics.push(
      error(
        "PSEUDO-LOOP-001",
        undefined,
        "The algorithm says repeat several times, but does not specify a count or condition.",
        "Use repeat N times or provide a precise loop condition.",
      ),
    );
  }

  if (/\b(?:is|are)\s+empty\b|==\s*\[\s*\]|!=\s*\[\s*\]/i.test(algorithm)) {
    diagnostics.push(
      warning(
        "PSEUDO-LIST-001",
        undefined,
        "Algorithm tests list emptiness directly; this is valid solving intent but v0 has no list length or list equality operation.",
        "During implementation, preserve the intent by tracking an explicit count or flag when the list is built.",
      ),
    );
  }

  if (/\b(?:increment|decrement)\s+[a-z_]\w*\b/i.test(algorithm)) {
    diagnostics.push(
      warning(
        "PSEUDO-STATE-001",
        undefined,
        "Algorithm uses increment/decrement shorthand; this is valid pseudocode when the target state and amount are clear.",
        "Implementation should translate clear shorthand into explicit Sophia set statements such as set name = name + 1.",
      ),
    );
  }

  if (/\bconvert\s+.+?\s+to\s+Text\b/i.test(algorithm)) {
    diagnostics.push(
      warning(
        "PSEUDO-TEXT-001",
        undefined,
        "Algorithm asks for explicit conversion to Text; keep this as semantic intent, not required Sophia syntax.",
        "During implementation, compile console-only text conversion as print value directly. Only produce Text expressions for actual Text outputs.",
      ),
    );
  }

  if (/\bprint\s+.+?\s+as\s+text\b/i.test(algorithm)) {
    diagnostics.push(
      warning(
        "PSEUDO-TEXT-002",
        undefined,
        'Algorithm writes "print ... as text"; treat this as console-output intent, not a required conversion operation.',
        "During implementation, emit print value directly; the runtime console effect records the printed value as text.",
      ),
    );
  }

  const intLikeBoolNames = variableNamesAssignedAsZeroOne(algorithm);
  for (const name of intLikeBoolNames) {
    if (new RegExp(`\\bif\\s+${escapeRegExp(name)}\\s*\\{`, "i").test(algorithm)) {
      diagnostics.push(
        warning(
          "PSEUDO-BOOL-001",
          undefined,
          `Algorithm uses ${name} as a Bool-like condition after assigning numeric 0/1 values.`,
          "Implementation should map unambiguous flag semantics to Bool state or explicit Int comparison.",
        ),
      );
    }
  }

  if (hasSuspiciousExclusiveInputListProcessing(inputs, outputs, algorithm)) {
    diagnostics.push(
      error(
        "PSEUDO-BRANCH-002",
        undefined,
        "Algorithm appears to process multiple inputs for a list result through an else-nested branch chain, making later inputs conditional on earlier inputs failing.",
        "Use separate independent if blocks for each input/item that should be considered in order. Reserve nested if inside else for mutually exclusive classification of the same value.",
      ),
    );
  }

  const delegatingMainFlow = findSingleDelegatingMainFlow(algorithm);
  if (delegatingMainFlow) {
    diagnostics.push(
      warning(
        "PSEUDO-FLOW-001",
        undefined,
        `main_flow ${delegatingMainFlow.mainFlowName} only delegates to subaction ${delegatingMainFlow.subactionName}.`,
        "This is valid pseudocode, but implementation should avoid creating a pure wrapper action that adds no semantic boundary.",
      ),
    );
  }

  if (
    extractNamedSection(content, "implementation_hints") !== null ||
    hasPseudocodeSection(content, "implementation_hints")
  ) {
    diagnostics.push(
      warning(
        "PSEUDO-HINT-001",
        undefined,
        "Pseudocode contains implementation_hints, which are implementation-stage metadata rather than solving logic.",
        "Remove implementation_hints from .pseudo. Formal names and contracts must come from the implementation stage or public scaffold override.",
      ),
    );
  }

  for (const pattern of VAGUE_PATTERNS) {
    if (pattern.test(algorithm)) {
      diagnostics.push(
        warning(
          "PSEUDO-VAGUE-001",
          undefined,
          `Algorithm contains vague step: ${pattern.source}.`,
          "Replace vague wording with concrete state updates, branches, or returns.",
        ),
      );
    }
  }

  const checks: PseudocodeChecks = {
    has_purpose: sections.has("purpose") || hasPseudocodeSection(content, "purpose"),
    has_inputs: sections.has("inputs") || hasPseudocodeSection(content, "inputs"),
    has_outputs: sections.has("outputs") || hasPseudocodeSection(content, "outputs"),
    has_algorithm: sections.has("algorithm") || hasPseudocodeSection(content, "algorithm"),
    has_expected: sections.has("expected") || hasPseudocodeSection(content, "expected"),
    loop_details_explicit: !/\brepeat\s+several\s+times\b/i.test(algorithm),
    state_updates_explicit: !/\brepeat\b/i.test(algorithm) || /\bset\b|\bappend\b/i.test(algorithm),
    no_vague_steps: !VAGUE_PATTERNS.some((pattern) => pattern.test(algorithm)),
  };

  return {
    ok: !diagnostics.some((diagnostic) => diagnostic.severity === "error"),
    diagnostics,
    checks,
  };
}

function findSingleDelegatingMainFlow(
  algorithm: string,
): { mainFlowName: string; subactionName: string } | null {
  const subactionNames = new Set(
    [...algorithm.matchAll(/\bsubaction\s+([A-Z][A-Za-z0-9]*)\s*\{/g)]
      .map((match) => match[1])
      .filter((name): name is string => Boolean(name)),
  );
  if (subactionNames.size === 0) return null;

  for (const match of algorithm.matchAll(/\bmain_flow\s+([A-Z][A-Za-z0-9]*)\s*\{/g)) {
    if (!match[1] || match.index === undefined) continue;
    const body = readBraceBody(algorithm, match.index + match[0].length);
    if (body === null) continue;
    const calledSubactions = [
      ...new Set(
        [...body.matchAll(/\boutput\s+of\s+([A-Z][A-Za-z0-9]*)\b/g)]
          .map((callMatch) => callMatch[1])
          .filter((name): name is string => name !== undefined && subactionNames.has(name)),
      ),
    ];
    if (calledSubactions.length !== 1) continue;
    const meaningfulLines = body
      .split("\n")
      .map((line) => line.trim())
      .filter(Boolean);
    const nonDelegatingLines = meaningfulLines.filter(
      (line) =>
        !new RegExp(`\\boutput\\s+of\\s+${escapeRegExp(calledSubactions[0] ?? "")}\\b`).test(
          line,
        ) && !/^return\s+[a-z_]\w*$/i.test(line),
    );
    if (nonDelegatingLines.length === 0) {
      return { mainFlowName: match[1], subactionName: calledSubactions[0] ?? "" };
    }
  }
  return null;
}

function extractPseudoFields(sectionBody: string): SophiaField[] {
  return [...sectionBody.matchAll(/\b([a-z_]\w*)\s*(?::=|:)\s*([^,\n]+)/g)]
    .map((match) => ({ name: match[1] ?? "", type: match[2]?.trim() ?? "" }))
    .filter((field) => Boolean(field.name && field.type));
}

function variableNamesAssignedAsZeroOne(algorithm: string): Set<string> {
  const names = new Set<string>();
  for (const match of algorithm.matchAll(/\bset\s+([a-z_]\w*)\s+to\s+(?:0|1)\b/gi)) {
    if (match[1]) names.add(match[1]);
  }
  return names;
}

function hasSuspiciousExclusiveInputListProcessing(
  inputs: string,
  outputs: string,
  algorithm: string,
): boolean {
  const inputNames = [...inputs.matchAll(/\b([a-z_][A-Za-z0-9_]*)\s*:/g)].flatMap((match) =>
    match[1] ? [match[1]] : [],
  );
  if (inputNames.length < 2) return false;
  if (!/\bList\s*</i.test(outputs)) return false;
  if (!/\bappend\b/i.test(algorithm)) return false;

  const escapedInputs = inputNames.map(escapeRegExp).join("|");
  const nestedDifferentInputBranch = new RegExp(
    `\\bif\\s+(${escapedInputs})\\b[^{}]*\\{[\\s\\S]*?\\}\\s*else\\s*\\{\\s*if\\s+(${escapedInputs})\\b`,
    "i",
  );
  const match = nestedDifferentInputBranch.exec(algorithm);
  return Boolean(match && match[1] !== match[2]);
}
