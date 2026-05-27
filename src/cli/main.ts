#!/usr/bin/env node
import { Command } from "commander";
import { registerBaseCommands } from "./base_commands.js";
import { registerExperimentCommands } from "./experiment_commands.js";
import { registerGraphCommands } from "./graph_commands.js";

const program = new Command();

program.name("sophia").description("Sophia v0 experimental CLI").version("0.0.0");

registerBaseCommands(program);
registerGraphCommands(program);
registerExperimentCommands(program);

program.parseAsync().catch((error: unknown) => {
  console.error(error instanceof Error ? error.message : String(error));
  process.exitCode = 1;
});
