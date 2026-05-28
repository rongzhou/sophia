#!/usr/bin/env node
import { readFile, writeFile } from 'node:fs/promises';
import { spawnSync } from 'node:child_process';
import path from 'node:path';

function run(cmd, args, options = {}) {
  const r = spawnSync(cmd, args, { stdio: 'pipe', encoding: 'utf8', ...options });
  if (r.status !== 0) throw new Error(`${cmd} ${args.join(' ')} failed: ${r.stderr || r.stdout}`);
  return r.stdout.trim();
}

async function main() {
  const repoRoot = process.cwd();
  // Count test files and tests via vitest summary JSON
  const resultJson = run('npx', ['vitest', 'run', '--reporter=json']);
  let files = 0, tests = 0, pass = true;
  try {
    const report = JSON.parse(resultJson);
    files = Array.isArray(report.testResults) ? report.testResults.length : 0;
    tests = report.numTotalTests ?? 0;
    pass = report.success ?? true;
  } catch {
    // fallback: naive counts
    files = (resultJson.match(/\"filePath\":/g) || []).length;
    tests = (resultJson.match(/\"status\":\"(pass|fail)\"/g) || []).length;
  }

  const statusPath = path.join(repoRoot, 'docs/en/status.md');
  const md = await readFile(statusPath, 'utf8');
  const updated = md.replace(
    /(Recent local validation:[\s\S]*?- `npm run typecheck` passes\.)[\s\S]*?(## Current v0\.2 regression coverage:)/m,
    `$1\n\n- Tests: ${files} files, ${tests} tests (${pass ? 'all passed' : 'with failures'}).\n\n$2`
  );
  await writeFile(statusPath, updated, 'utf8');
  console.log(`Updated ${statusPath}`);
}

main().catch((e) => {
  console.error(e);
  process.exit(1);
});
