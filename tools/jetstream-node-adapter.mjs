#!/usr/bin/env node
// Minimal JS-shell adapter for running the pinned JetStream 3.0 cli.js under
// Node 22.14.0. This is only a harness reference; it is not a Kagura result.
import fs from "node:fs";
import path from "node:path";
import vm from "node:vm";
import { createRequire } from "node:module";

globalThis.require = createRequire(import.meta.url);
globalThis.__dirname = process.cwd();
globalThis.print = (...args) => console.log(...args);
globalThis.printErr = (...args) => console.error(...args);
globalThis.readFile = (file) => fs.readFileSync(path.resolve(process.cwd(), file), "utf8");
globalThis.read = (file, mode) => {
  const buffer = fs.readFileSync(path.resolve(process.cwd(), file));
  return mode === "binary"
    ? new Int8Array(buffer.buffer, buffer.byteOffset, buffer.byteLength)
    : buffer.toString("utf8");
};
globalThis.load = (file) => {
  const absolute = path.resolve(process.cwd(), file);
  const source = fs.readFileSync(absolute, "utf8");
  vm.runInThisContext(source, { filename: absolute });
};

globalThis.runString = (source = "") => {
  const context = vm.createContext({
    console,
    performance,
    TextDecoder,
    TextEncoder,
    print,
    printErr,
    readFile: globalThis.readFile,
    read: globalThis.read,
    load: globalThis.load,
  });
  context.globalThis = context;
  context.self = context;
  context.loadString = (script) => vm.runInContext(script, context);
  if (source) vm.runInContext(source, context);
  return context;
};

const separator = process.argv.indexOf("--");
globalThis.arguments =
  separator >= 0 ? process.argv.slice(separator + 1) : process.argv.slice(2);

globalThis.load("cli.js");
