import {
  buildImplementationStructurePlan,
  type ImplementationStructureOverride,
} from "./structure_plan.js";

export const SCAFFOLD_TODO_PATTERN = /\[(?:TODO|TOOL-INFERRED)\b|TODO:\s*LLM-fill/;

const SCAFFOLD_TOOL_INFERRED_NO_INPUTS = "// [TOOL-INFERRED] no inputs";
const SCAFFOLD_TOOL_INFERRED_NO_EFFECTS = "// [TOOL-INFERRED] no effects";
const SCAFFOLD_TOOL_INFERRED_OUTPUT =
  "// [TOOL-INFERRED] no explicit output type; replace result if pseudo requires one";
const SCAFFOLD_LLM_FILL_ERRORS =
  "// [TODO: LLM-fill only if pseudo.forbidden implies an explicit v0 error]";
const SCAFFOLD_LLM_FILL_BODY = "// [TODO: LLM-fill from pseudo.algorithm]";

export interface SophiaScaffold {
  files: Record<string, string>;
  notes: string[];
}

export function buildSophiaScaffold(
  pseudocode: string,
  structureOverride: ImplementationStructureOverride = {},
): SophiaScaffold {
  const plan = buildImplementationStructurePlan(pseudocode, structureOverride);
  const inputLines = plan.action_contract_hints.inputs.map(
    (field) => `    ${field.name}: ${field.type}`,
  );
  const output = plan.action_contract_hints.output;
  const effectLines = plan.action_contract_hints.effects.map((effect) => `    ${effect}`);
  const allowEffects = plan.action_contract_hints.effects.join(" ");

  return {
    files: {
      [plan.files.domain]: `domain ${plan.symbols.domain} {
}`,
      ...Object.fromEntries(
        plan.action_contract_hints.entities.map((entity, index) => [
          plan.files.entities[index] ??
            `domains/${plan.symbols.domain}/entities/${entity.name}.sophia`,
          `entity ${entity.name} {
  fields {
${entity.fields.map((field) => `    ${field.name}: ${field.type}`).join("\n")}
  }
}`,
        ]),
      ),
      ...Object.fromEntries(
        plan.action_contract_hints.states.map((state, index) => [
          plan.files.states[index] ?? `domains/${plan.symbols.domain}/states/${state.name}.sophia`,
          `state ${state.name} {
${state.values.map((value) => `  value ${value} { }`).join("\n")}
}`,
        ]),
      ),
      [plan.files.capability]: `capability ${plan.symbols.capability} {
  allow { ${allowEffects} }
}`,
      [plan.files.action]: `action ${plan.symbols.action} {
  capability: ${plan.symbols.capability}
  input {
${inputLines.length > 0 ? inputLines.join("\n") : `    ${SCAFFOLD_TOOL_INFERRED_NO_INPUTS}`}
  }
  output {
${
  output
    ? `    ${output.name}: ${output.type}`
    : `    result: Unit\n    ${SCAFFOLD_TOOL_INFERRED_OUTPUT}`
}
  }
  effects {
${effectLines.length > 0 ? effectLines.join("\n") : `    ${SCAFFOLD_TOOL_INFERRED_NO_EFFECTS}`}
  }
  errors {
    ${SCAFFOLD_LLM_FILL_ERRORS}
  }
  body {
    ${SCAFFOLD_LLM_FILL_BODY}
  }
}`,
    },
    notes: [
      "Deterministic scaffold generated from .pseudo structure only.",
      "The scaffold intentionally leaves body and non-structural error semantics as TODOs for LLM implementation.",
      "Do not treat scaffold comments as final Sophia-Core source.",
    ],
  };
}
