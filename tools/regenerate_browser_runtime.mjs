#!/usr/bin/env node
import assert from "node:assert/strict";
import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { regenerateBundle, runtimeSources, sha256 } from "./browser_runtime.mjs";

const repoRoot = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
if (process.argv.includes("--check")) {
  console.log(`browser runtime reproducible: ${runtimeSources(repoRoot).metadata.bundleSha256}`);
} else {
  const metadataPath = path.join(repoRoot, "js/mq_js_bundle.provenance.json");
  const metadata = JSON.parse(fs.readFileSync(metadataPath, "utf8"));
  const base = fs.readFileSync(path.join(repoRoot, metadata.base.bundlePath), "utf8");
  assert.equal(sha256(base), metadata.base.bundleSha256, "upstream browser runtime checksum changed");
  const selection = metadata.extensions.fork_webgl_selection;
  const glSource = fs.readFileSync(path.join(repoRoot, selection.sourcePath), "utf8");
  const bundle = regenerateBundle(base, glSource);
  fs.writeFileSync(path.join(repoRoot, "js/mq_js_bundle.js"), bundle);
  metadata.bundleSha256 = sha256(bundle);
  selection.sourceSha256 = sha256(glSource);
  fs.writeFileSync(metadataPath, `${JSON.stringify(metadata, null, 2)}\n`);
  console.log(`browser runtime regenerated: ${metadata.bundleSha256}`);
}
