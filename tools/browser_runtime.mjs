import assert from "node:assert/strict";
import crypto from "node:crypto";
import fs from "node:fs";
import path from "node:path";
import vm from "node:vm";

export const sha256 = (bytes) => crypto.createHash("sha256").update(bytes).digest("hex");

export function regenerateBundle(base) {
  let bundle = base;
  const replace = (name, before, after) => {
    assert.equal(bundle.split(before).length - 1, 1, `${name}: upstream anchor must occur exactly once`);
    bundle = bundle.replace(before, after);
  };
  replace("fallback canvas", 'document.querySelector("#glcanvas");',
    'document.querySelector("#glcanvas")||(()=>{const e=document.createElement("canvas");return e.id="glcanvas",e.tabIndex=1,(document.body||document.documentElement).appendChild(e),e})();');
  replace("bounded download copy",
    'fs_take_buffer:function(e,t,n){var s,i,o=FS.loaded_files[e];console.assert(o.length<=n);for(i=new Uint8Array(wasm_memory.buffer,t,n),s=0;s<o.length;s++)i[s]=o[s];delete FS.loaded_files[e]},',
    'fs_take_buffer:function(e,t,n){var i,o=FS.loaded_files[e];console.assert(o.length<=n),i=new Uint8Array(wasm_memory.buffer,t,n);/*zubspace:fs-take-buffer-memcpy*/i.set(o.length>n?o.subarray(0,n):o);delete FS.loaded_files[e]},');
  replace("module-aware import resolution",
    'function add_missing_functions_stabs(e){var t=WebAssembly.Module.imports(e);for(const e in t)importObject.env[t[e].name]==0[0]&&(console.warn("No "+t[e].name+" function in gl.js"),importObject.env[t[e].name]=function(){console.warn("Missed function: "+t[e].name)})}',
    'function add_missing_functions_stabs(e){var t=WebAssembly.Module.imports(e);for(const e in t){const n=t[e],s=importObject[n.module];s&&s[n.name]!==void 0||(console.warn("No "+n.module+"."+n.name+" function in gl.js"),importObject[n.module]=s||{},importObject[n.module][n.name]=function(){console.warn("Missed function: "+n.module+"."+n.name)})}}');
  replace("WASM response callback", 'function load(e){var t=fetch(e);',
    'function load(e,prepareResponse){var t=fetch(e);if(typeof prepareResponse==="function")t=t.then(function(response){try{return prepareResponse(response)||response}catch(error){console.warn("WASM response diagnostics unavailable");return response}});');
  replace("quiet audio unlock", 'n=new s;var t=function(){console.log("fix"),e.resume();',
    'n=new s;var t=function(){e.resume();');
  // The published 0.4.16 aggregate assigns this undeclared name under the
  // enclosing strict directive, throwing before any WASM can start.
  replace("scoped quad-net registrar", 'function(){function i(){}register_plugin=function(e){',
    'function(){function i(){}let register_plugin;register_plugin=function(e){');
  replace("audio load state storage", 'let e,n=new Map,t=[],o=1,m=1;',
    'let e,n=new Map,b=new Map,t=[],o=1,m=1;');
  replace("audio terminal decode state",
    'function r(t,s){let a=wasm_memory.buffer.slice(t,t+s),i=o;return o+=1,e.decodeAudioData(a,function(e){n.set(i,e)},function(e){console.error("Failed to decode audio buffer",e)}),i}',
    'function r(t,s){if(o>4294967295)throw new RangeError("audio source ids exhausted");let a=wasm_memory.buffer.slice(t,t+s),i=o++;b.set(i,0);const ready=function(e){if(b.get(i)!==0)return;n.set(i,e);b.set(i,1)},failed=function(e){if(b.get(i)!==0)return;b.set(i,-1);console.error("Failed to decode audio buffer",e)};try{const result=e.decodeAudioData(a,ready,failed);if(result&&typeof result.then==="function")result.then(ready,failed)}catch(e){failed(e)}return i}');
  replace("audio pending cancellation", 'function f(e){i(e),n.delete(e)}',
    'function f(e){i(e),n.delete(e),b.delete(e)}');
  replace("audio state extension import", 'e.env.audio_source_is_loaded=a,',
    'e.env.audio_source_is_loaded=a,e.env.audio_source_state=function(e){return b.has(e)?b.get(e):-2},');
  return bundle.replace(/\n?$/, "\n");
}

export function runtimeSources(repoRoot, assetRoot = repoRoot) {
  const metadata = JSON.parse(fs.readFileSync(path.join(assetRoot, "js/mq_js_bundle.provenance.json"), "utf8"));
  assert.equal(metadata.schemaVersion, 1, "unsupported browser runtime provenance schema");
  const base = fs.readFileSync(path.join(repoRoot, metadata.base.bundlePath), "utf8");
  assert.equal(sha256(base), metadata.base.bundleSha256, "upstream browser runtime checksum changed");
  const bundle = fs.readFileSync(path.join(assetRoot, "js/mq_js_bundle.js"), "utf8");
  assert.equal(bundle, regenerateBundle(base), "browser runtime is not reproducible; run node tools/regenerate_browser_runtime.mjs");
  assert.equal(sha256(bundle), metadata.bundleSha256, "generated browser runtime checksum changed");
  return { metadata, bundle };
}

export function validateRustPairing(lock, expected) {
  const packages = lock.split(/^\[\[package\]\]\s*$/m).slice(1).map((section) => {
    const fields = {};
    for (const match of section.matchAll(/^(name|version|source) = ("[^"\n]*")$/gm)) {
      fields[match[1]] = JSON.parse(match[2]);
    }
    return fields;
  });
  for (const [name, wanted] of Object.entries(expected)) {
    const found = packages.filter((pkg) => pkg.name === name);
    assert.equal(found.length, 1, `${name}: expected one resolved runtime package`);
    assert.equal(found[0].version, wanted.version, `${name}: Rust/runtime version mismatch`);
    assert.equal(found[0].source ?? null, wanted.source, `${name}: Rust/runtime source mismatch`);
  }
}

export function createRuntime({ bundle }, overrides = {}) {
  const context = vm.createContext({
    console, TextEncoder, TextDecoder, Uint8Array, Uint32Array, Int32Array,
    document: { querySelector: () => ({ focus() {} }) }, window: {}, ...overrides,
  });
  new vm.Script(bundle, { filename: "mq_js_bundle.js" }).runInContext(context, { timeout: 1000 });
  context.register_plugins(context.plugins);
  return context;
}

export function validateWasmContract(module, runtime, abi, requireAudio = true) {
  // Never call add_missing_functions_stabs: its no-op functions make a missing
  // import instantiate successfully while hiding the actual integration fault.
  const imports = WebAssembly.Module.imports(module);
  for (const entry of imports) {
    const implementation = runtime.importObject[entry.module]?.[entry.name];
    assert.notEqual(implementation, undefined, `missing WASM import implementation: ${entry.module}.${entry.name}`);
    assert.equal(entry.kind, "function", `unsupported WASM import kind: ${entry.module}.${entry.name} (${entry.kind})`);
    assert.equal(typeof implementation, "function", `invalid WASM import implementation: ${entry.module}.${entry.name}`);
  }
  const plugins = Object.fromEntries(runtime.plugins.map((plugin) => [plugin.name, plugin.version]));
  assert.deepEqual(plugins, abi.plugins, "browser plugin names/versions differ from the runtime contract");
  assert.equal(vm.runInContext("version", runtime), abi.miniquad, "Miniquad browser ABI mismatch");
  // Rust has no WASM start function here. Instantiate without running main and
  // invoke only the constant ABI exports; no graphics/audio/RTC imports run.
  const instance = new WebAssembly.Instance(module, runtime.importObject);
  assert.equal(typeof instance.exports.crate_version, "function", "Miniquad WASM ABI export is missing");
  assert.equal(instance.exports.crate_version(), abi.miniquad, "Miniquad Rust/browser ABI mismatch");
  for (const name of (requireAudio ? abi.requiredWasmPlugins : [])) {
    assert.equal(typeof instance.exports[`${name}_crate_version`], "function", `required WASM plugin is missing: ${name}`);
  }
  for (const [name, version] of Object.entries(plugins)) {
    const exported = instance.exports[`${name}_crate_version`];
    if (exported) assert.equal(exported(), version, `WASM plugin ABI mismatch: ${name}`);
  }
  return { imports: imports.length, plugins };
}

export function checkBrowserRuntime(repoRoot, wasmPath, requireAudio = true) {
  const sources = runtimeSources(repoRoot);
  const module = new WebAssembly.Module(fs.readFileSync(wasmPath));
  const runtime = createRuntime(sources);
  const contract = validateWasmContract(module, runtime, sources.metadata.abi, requireAudio);
  const extension = sources.metadata.extensions.zubspace_audio_source_state;
  assert.equal(extension.version, 1, "unsupported audio state extension version");
  assert.equal(extension.requiredByCurrentRust, true, "fork requires the audio state extension");
  assert.equal(typeof runtime.importObject.env.audio_source_state, "function", "audio state extension missing");
  if (requireAudio) {
    assert.ok(WebAssembly.Module.imports(module).some((entry) => entry.module === "env" && entry.name === "audio_source_state"),
      "paired fork must import runtime extension: audio_source_state");
  }
  return { ...contract, bundleSha256: sources.metadata.bundleSha256 };
}
