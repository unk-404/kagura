#!/usr/bin/env node
// Direct-workload smoke runner for a single JetStream benchmark source file.
// This deliberately does NOT emit an official JetStream score. It exists to
// expose crashes/timeouts while the official driver is not yet runnable.
import { spawnSync } from "node:child_process";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { performance } from "node:perf_hooks";

function arg(name, fallback) {
  const index = process.argv.indexOf(`--${name}`);
  return index >= 0 && index + 1 < process.argv.length ? process.argv[index + 1] : fallback;
}

const engine = arg("engine", "node");
const file = arg("file");
const entry = arg("entry", "runRichards");
const iterations = Number(arg("iterations", "1"));
const heapSize = arg("heap-size", "8388608");
const stepLimit = arg("step-limit", "1000000000");
if (!file) {
  console.error("usage: node tools/jetstream-direct-smoke.mjs --file FILE --entry FN [--iterations N] [--engine node|kagura]");
  process.exit(2);
}

const source = fs.readFileSync(file, "utf8");
const wrapper = `\nfor (var __i = 0; __i < ${iterations}; __i++) ${entry}();\nconsole.log("JETSTREAM_SMOKE_OK");\n`;
const tempRoot = path.resolve(".agents", "tmp");
fs.mkdirSync(tempRoot, { recursive: true });
const directory = fs.mkdtempSync(path.join(tempRoot, "kagura-jetstream-"));
const script = path.join(directory, "workload.js");
fs.writeFileSync(script, source + wrapper);

const command =
  engine === "kagura"
    ? [path.resolve("target/release/kagura.exe"), ["run", "--heap-size", heapSize, "--step-limit", stepLimit, script]]
    : ["node", [script]];
const started = performance.now();
const result = spawnSync(command[0], command[1], { encoding: "utf8" });
const elapsed = performance.now() - started;
fs.rmSync(directory, { recursive: true, force: true });

console.log(
  JSON.stringify(
    {
      engine,
      file,
      entry,
      iterations,
      exitCode: result.status,
      elapsedMs: Math.round(elapsed),
      stdout: result.stdout.trim(),
      stderrTail: result.stderr.trim().split("\n").slice(-3).join("\n"),
      officialScore: null,
    },
    null,
    2,
  ),
);
process.exit(result.status === 0 ? 0 : 1);
