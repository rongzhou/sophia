#!/usr/bin/env node
import { Command } from "commander";
import { registerBaseCommands } from "./base_commands.js";
import { registerExperimentCommands } from "./experiment_commands.js";
import { registerGraphCommands } from "./graph_commands.js";
import { isLlmCallError } from "../llm/errors.js";

const program = new Command();

program.name("sophia").description("Sophia v0 experimental CLI").version("0.3.0");

registerBaseCommands(program);
registerGraphCommands(program);
registerExperimentCommands(program);

program.parseAsync().catch((error: unknown) => {
  if (isLlmCallError(error)) {
    console.error(error.message);
    console.error(
      "LLM prompt/response details are available on the error object; graph commands also persist raw LLM failure artifacts under sophia-runs/graph when capture is enabled.",
    );
  } else {
    console.error(error instanceof Error ? (error.stack ?? error.message) : String(error));
  }
  process.exitCode = 1;
});
