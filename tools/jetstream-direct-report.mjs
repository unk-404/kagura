#!/usr/bin/env node
// Reproducible direct-workload comparison for a named subset of JetStream
// benchmark sources. This is NOT the official JetStream 3.0 harness and never
// reports an official score; it exists to measure crash/timeout coverage while
// the official driver cannot run under Kagura.
import { spawnSync } from "node:child_process";
import fs from "node:fs";
import path from "node:path";
import { performance } from "node:perf_hooks";

function arg(name, fallback) {
  const index = process.argv.indexOf(`--${name}`);
  return index >= 0 && index + 1 < process.argv.length ? process.argv[index + 1] : fallback;
}

const corpus = arg("corpus");
if (!corpus) {
  console.error("usage: node tools/jetstream-direct-report.mjs --corpus DIR [--timeout-ms N]");
  process.exit(2);
}
const timeoutMs = Number(arg("timeout-ms", "20000"));
const iterations = Number(arg("iterations", "1"));
const names = [
  "3d-cube",
  "3d-raytrace",
  "base64",
  "crypto-md5",
  "crypto-sha1",
  "date-format-tofte",
  "date-format-xparb",
  "n-body",
  "regex-dna",
  "string-unpack-code",
  "tagcloud",
];
const prelude =
  "var performance = (typeof globalThis !== 'undefined' && globalThis.performance) || { now: function () { return Date.now(); } };\n";
const tempRoot = path.resolve(".agents", "tmp");
fs.mkdirSync(tempRoot, { recursive: true });
const directory = fs.mkdtempSync(path.join(tempRoot, "kagura-direct-report-"));

function run(engine, source, label) {
  const call = "new Benchmark().runIteration()";
  const wrapper = `\nfor (var __i = 0; __i < ${iterations}; __i++) ${call}; console.log("JETSTREAM_SMOKE_OK");\n`;
  const script = path.join(directory, `${label}.js`);
  fs.writeFileSync(script, prelude + source + wrapper);
  const command =
    engine === "kagura"
      ? [
          path.resolve("target/release/kagura.exe"),
          [
            "run",
            "--heap-size",
            "8388608",
            "--step-limit",
            "5000000",
            script,
          ],
        ]
      : ["node", [script]];
  const started = performance.now();
  const result = spawnSync(command[0], command[1], {
    encoding: "utf8",
    timeout: timeoutMs,
    maxBuffer: 1024 * 1024,
  });
  return {
    elapsedMs: Math.round(performance.now() - started),
    exitCode: result.status,
    timedOut: result.error?.code === "ETIMEDOUT",
    ok: result.status === 0 && !result.error,
    stdout: result.stdout.slice(0, 200),
    stderr: result.stderr.trim().split("\n").slice(-2).join(" | ").slice(0, 160),
  };
}

const rows = [];
for (const name of names) {
  const file = path.join(corpus, "SunSpider", `${name}.js`);
  if (!fs.existsSync(file)) continue;
  const source = fs.readFileSync(file, "utf8");
  const kagura = run("kagura", source, `${name}-kagura`);
  const node = run("node", source, `${name}-node`);
  rows.push({ name, kagura, node });
  console.error(`${name}: kagura=${kagura.ok ? "ok" : "fail"} node=${node.ok ? "ok" : "fail"}`);
}
fs.rmSync(directory, { recursive: true, force: true });

const lines = [
  "# JetStream direct-workload subset (not an official score)",
  "",
  `- Timeout per run: ${timeoutMs} ms`,
  `- Iterations: ${iterations}`,
  "- Engine entry: `new Benchmark().runIteration()`",
  "- All runs have `officialScore: null`",
  "",
  "| Workload | Kagura | Kagura ms | Node | Node ms | Kagura error |",
  "| --- | --- | ---: | --- | ---: | --- |",
];
for (const row of rows) {
  const k = row.kagura;
  const n = row.node;
  const status = (r) => (r.timedOut ? "timeout" : r.ok ? "ok" : `fail(${r.exitCode})`);
  const error = k.timedOut ? "timeout" : k.ok ? "" : k.stderr.replaceAll("|", "\\|").slice(0, 120);
  lines.push(
    `| ${row.name} | ${status(k)} | ${k.elapsedMs} | ${status(n)} | ${n.elapsedMs} | ${error} |`,
  );
}
const report = lines.join("\n") + "\n";
const output = path.resolve(arg("output", ".agents/jetstream-direct-report.md"));
fs.writeFileSync(output, report);
process.stdout.write(report);
