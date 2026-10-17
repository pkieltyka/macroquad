#!/usr/bin/env node
import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { checkBrowserRuntime, runtimeSources, validateRustPairing } from "./browser_runtime.mjs";

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const args = process.argv.slice(2);
try {
  if (args.length === 1 && args[0] === "--source-only") {
    const sources = runtimeSources(root);
    validateRustPairing(fs.readFileSync(path.join(root, "Cargo.lock"), "utf8"), sources.metadata.rustPackages);
    console.log(`browser runtime source pairing valid: ${sources.metadata.bundleSha256}`);
  } else if ((args.length === 2 || (args.length === 3 && args[2] === "--no-audio")) && args[0] === "--wasm") {
    console.log(JSON.stringify(checkBrowserRuntime(root, path.resolve(args[1]), args[2] !== "--no-audio")));
  } else {
    throw new Error("usage: node tools/check_browser_runtime.mjs --source-only | --wasm PATH [--no-audio]");
  }
} catch (error) {
  console.error(error.message);
  process.exitCode = 1;
}
