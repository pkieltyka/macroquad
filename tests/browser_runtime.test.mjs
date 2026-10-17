import assert from "node:assert/strict";
import fs from "node:fs";
import path from "node:path";
import os from "node:os";
import vm from "node:vm";
import test from "node:test";
import { fileURLToPath } from "node:url";
import {
  checkBrowserRuntime, createRuntime, regenerateBundle, runtimeSources,
  sha256, validateRustPairing, validateWasmContract,
} from "../tools/browser_runtime.mjs";

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const sources = runtimeSources(root);
const base = fs.readFileSync(path.join(root, sources.metadata.base.bundlePath), "utf8");

// Minimal real WASM modules expose the same constant ABI functions as Rust.
// Import failures and version mismatches exercise the production validator.
const uint = (value) => value < 128 ? [value] : [(value & 127) | 128, ...uint(value >>> 7)];
const text = (value) => [...uint(Buffer.byteLength(value)), ...Buffer.from(value)];
const section = (id, bytes) => [id, ...uint(bytes.length), ...bytes];
function wasm({ imports = [], miniquad = 2, audio = 1, audioExport = true, returnBytes = false } = {}) {
  imports = [["env", "webgl_selection_version"], ...imports];
  const exports = [["crate_version", miniquad], ...(audioExport ? [["macroquad_audio_crate_version", audio]] : [])];
  const code = exports.flatMap(([, value]) => {
    const body = [0, 0x41, value, 0x0b];
    return [...uint(body.length), ...body];
  });
  const bytes = Uint8Array.from([
    0, 97, 115, 109, 1, 0, 0, 0,
    ...section(1, [1, 0x60, 0, 1, 0x7f]),
    ...(imports.length ? section(2, [...uint(imports.length), ...imports.flatMap(([module, name]) => [...text(module), ...text(name), 0, 0])]) : []),
    ...section(3, [...uint(exports.length), ...exports.flatMap(() => [0])]),
    ...section(7, [...uint(exports.length), ...exports.flatMap(([name], index) => [...text(name), 0, ...uint(imports.length + index)])]),
    ...section(10, [...uint(exports.length), ...code]),
  ]);
  return returnBytes ? bytes : new WebAssembly.Module(bytes);
}

function listeners() {
  const events = new Map();
  return {
    addEventListener(name, fn) { events.set(name, [...(events.get(name) ?? []), fn]); },
    removeEventListener(name, fn) { events.set(name, (events.get(name) ?? []).filter((entry) => entry !== fn)); },
    emit(name) { for (const fn of events.get(name) ?? []) fn(); },
  };
}

function assertUnpatchedRegistrarDefect(bundle) {
  let runtime;
  try {
    runtime = createRuntime({ ...sources, bundle });
  } catch (error) {
    assert.equal(error.name, "ReferenceError");
    assert.equal(error.message, "register_plugin is not defined");
    return;
  }
  // Node 22's VM leaks the undeclared assignment; Node 26 rejects it instead.
  assert.equal(typeof runtime.register_plugin, "function");
}

test("vendored base and deterministic patches reproduce the checked-in bundle", () => {
  assert.equal(sha256(base), "4bf663a44a06c113bed92e0596c58139cd9db7f0d67ef2cc49ca06016e0b5ea0");
  assert.equal(regenerateBundle(base), sources.bundle);
  assert.throws(() => regenerateBundle(`${base}${base}`), /anchor must occur exactly once/);
  assert.throws(() => regenerateBundle(base.replace('console.log("fix")', 'console.log("changed")')), /quiet audio unlock/);
});

test("the complete strict-mode bundle boots and registers all plugins without leaking its registrar", () => {
  const runtime = createRuntime(sources);
  assert.deepEqual(runtime.plugins.map((plugin) => plugin.name).join(","), "macroquad_audio,sapp_jsutils,quad_net");
  assert.equal(runtime.register_plugin, undefined);
  assert.equal(typeof runtime.importObject.env.audio_init, "function");
  assert.equal(typeof runtime.importObject.env.ws_connect, "function");
  assertUnpatchedRegistrarDefect(base);
});

test("the upstream negative control rejects unrelated startup errors and a leak-free runtime", () => {
  for (const bundle of [
    'throw new Error("register_plugin is not defined");',
    'throw new ReferenceError("unrelated startup failure");',
    sources.bundle,
  ]) {
    assert.throws(() => assertUnpatchedRegistrarDefect(bundle), assert.AssertionError);
  }
});

test("a missing canvas is created, focusable, and appended before the bundle uses it", () => {
  const created = [], appended = [];
  let focused = 0;
  createRuntime(sources, { document: {
    querySelector: () => null,
    createElement: (tag) => { const element = { tag, focus: () => focused++ }; created.push(element); return element; },
    body: { appendChild: (element) => appended.push(element) },
  } });
  assert.equal(created.length, 1);
  assert.equal(created[0].tag, "canvas");
  assert.equal(created[0].id, "glcanvas");
  assert.equal(created[0].tabIndex, 1);
  assert.deepEqual(appended, created);
  assert.equal(focused, 1);
});

test("upstream focus handling emits one transition on blur/hidden and recovers on visible focus", () => {
  let focused = true;
  const changes = [];
  const canvas = { focus() {}, ...listeners() };
  const document = { querySelector: () => canvas, hasFocus: () => focused, visibilityState: "visible", ...listeners() };
  const window = { requestAnimationFrame() {}, ...listeners() };
  const runtime = createRuntime(sources, { document, window });
  runtime.wasm_exports = { focus: (value) => changes.push(value) };
  runtime.importObject.env.run_animation_loop(false);
  window.emit("focus");
  focused = false;
  window.emit("blur");
  window.emit("blur");
  focused = true;
  window.emit("focus");
  // A hidden document can retain document.hasFocus() in browser transitions.
  document.visibilityState = "hidden";
  document.emit("visibilitychange");
  document.emit("visibilitychange");
  document.visibilityState = "visible";
  document.emit("visibilitychange");
  assert.deepEqual(changes, [false, true, false, true]);
});

test("missing imports resolve in their own module without overwriting implemented imports", () => {
  const warnings = [];
  const runtime = createRuntime(sources, { console: { ...console, warn: (message) => warnings.push(message) } });
  const supplied = () => 7;
  runtime.importObject.other = { supplied };
  runtime.add_missing_functions_stabs(wasm({ imports: [["other", "supplied"], ["foreign", "missing"]] }));
  assert.equal(runtime.importObject.other.supplied, supplied);
  assert.equal(runtime.importObject.env.missing, undefined);
  runtime.importObject.foreign.missing();
  assert.deepEqual(warnings, ["No foreign.missing function in gl.js", "Missed function: foreign.missing"]);
});

test("the contract rejects missing imports before the loader can manufacture fallback stubs", () => {
  const runtime = createRuntime(sources);
  let stubCalls = 0;
  runtime.add_missing_functions_stabs = () => stubCalls++;
  for (const imports of [[["env", "unknown"]], [["foreign", "unknown"]]]) {
    assert.throws(() => validateWasmContract(wasm({ imports }), runtime, sources.metadata.abi), /missing WASM import implementation/);
  }
  assert.equal(stubCalls, 0);
  assert.equal(runtime.importObject.env.unknown, undefined);
});

test("the contract rejects absent plugins and Rust/browser ABI mismatches", () => {
  const runtime = createRuntime(sources);
  assert.throws(() => validateWasmContract(wasm({ miniquad: 3 }), runtime, sources.metadata.abi), /Miniquad Rust\/browser ABI mismatch/);
  assert.throws(() => validateWasmContract(wasm({ audio: 2 }), runtime, sources.metadata.abi), /WASM plugin ABI mismatch: macroquad_audio/);
  assert.throws(() => validateWasmContract(wasm({ audioExport: false }), runtime, sources.metadata.abi), /required WASM plugin is missing/);
  runtime.plugins[0].version = 2;
  assert.throws(() => validateWasmContract(wasm(), runtime, sources.metadata.abi), /browser plugin names\/versions/);
});

test("Rust pairing checks both version and source identity and rejects duplicate resolutions", () => {
  const lock = Object.entries(sources.metadata.rustPackages).map(([name, wanted]) => `[[package]]\nname = "${name}"\nversion = "${wanted.version}"\n${wanted.source ? `source = "${wanted.source}"\n` : ""}`).join("\n");
  validateRustPairing(lock, sources.metadata.rustPackages);
  assert.throws(() => validateRustPairing(lock.replace('version = "0.5.0"', 'version = "0.6.0"'), sources.metadata.rustPackages), /macroquad: Rust\/runtime version mismatch/);
  const changed = structuredClone(sources.metadata.rustPackages);
  changed.macroquad.source = "git+https://example.invalid/fork#abc";
  assert.throws(() => validateRustPairing(lock, changed), /macroquad: Rust\/runtime source mismatch/);
  assert.throws(() => validateRustPairing(`${lock}\n[[package]]\nname = "macroquad"\nversion = "0.4.16"\n`, sources.metadata.rustPackages), /expected one resolved/);
});

test("audio state extension reports terminal decode failures and cancellation without changing the legacy bool API", () => {
  const decodes = [], errors = [];
  let throwOnDecode = false;
  class AudioContext {
    constructor() { this.listener = {}; }
    decodeAudioData(bytes, resolve, reject) {
      if (throwOnDecode) throw new Error("synchronous decoder failure");
      decodes.push({ resolve, reject });
    }
  }
  const runtime = createRuntime(sources, {
    window: { AudioContext },
    document: { querySelector: () => ({ focus() {} }), ...listeners() },
    console: { ...console, error: (...args) => errors.push(args) },
  });
  runtime.wasm_memory = { buffer: new ArrayBuffer(8) };
  const env = runtime.importObject.env;
  env.audio_init();
  assert.equal(env.audio_source_state(12345), -2);
  assert.equal(env.audio_source_is_loaded(12345), false);
  const success = env.audio_add_buffer(0, 4);
  assert.equal(env.audio_source_state(success), 0);
  assert.equal(env.audio_source_is_loaded(success), false);
  decodes[0].resolve({});
  assert.equal(env.audio_source_state(success), 1);
  assert.equal(env.audio_source_is_loaded(success), true);
  env.audio_source_delete(success);
  assert.equal(env.audio_source_state(success), -2);
  assert.equal(env.audio_source_is_loaded(success), false);

  const failed = env.audio_add_buffer(0, 4);
  decodes[1].reject(new Error("invalid sound"));
  assert.equal(env.audio_source_state(failed), -1);
  assert.equal(env.audio_source_is_loaded(failed), false);
  decodes[1].resolve({});
  assert.equal(env.audio_source_state(failed), -1, "a late success cannot replace a terminal failure");
  env.audio_source_delete(failed);
  assert.equal(env.audio_source_state(failed), -2);

  const canceled = env.audio_add_buffer(0, 4);
  env.audio_source_delete(canceled);
  decodes[2].resolve({});
  decodes[2].reject(new Error("late cancellation error"));
  assert.equal(env.audio_source_state(canceled), -2, "late decoder callbacks cannot resurrect a canceled source");
  assert.equal(env.audio_source_is_loaded(canceled), false);
  assert.equal(errors.length, 1, "canceled callbacks must not report a new decode failure");

  throwOnDecode = true;
  const thrown = env.audio_add_buffer(0, 4);
  assert.equal(env.audio_source_state(thrown), -1);
  assert.equal(env.audio_source_is_loaded(thrown), false);
  assert.equal(errors.length, 2);
  assert.equal(sources.metadata.extensions.zubspace_audio_source_state.version, 1);
  validateWasmContract(wasm({ imports: [["env", "audio_source_state"]] }), runtime, sources.metadata.abi);
});

test("audio decode callbacks and returned Promises settle once without unhandled rejections", async () => {
  const decodes = [], errors = [], played = [];
  class AudioContext {
    constructor() { this.listener = {}; this.destination = {}; }
    decodeAudioData(bytes, success, failure) {
      return new Promise((resolve, reject) => decodes.push({ success, failure, resolve, reject }));
    }
    createBufferSource() {
      const source = { connect() {}, disconnect() {}, addEventListener() {}, removeEventListener() {}, start() {} };
      played.push(source);
      return source;
    }
    createGain() { return { gain: {}, connect() {}, disconnect() {} }; }
  }
  const runtime = createRuntime(sources, {
    window: { AudioContext },
    document: { querySelector: () => ({ focus() {} }), ...listeners() },
    console: { ...console, error: (...args) => errors.push(args) },
  });
  runtime.wasm_memory = { buffer: new ArrayBuffer(8) };
  const env = runtime.importObject.env;
  env.audio_init();
  const rejected = env.audio_add_buffer(0, 4);
  const failure = new Error("EncodingError: Unable to decode audio data");
  // Chrome calls the legacy error callback and independently rejects its
  // returned Promise. A callback alone does not handle that rejection.
  decodes[0].failure(failure);
  decodes[0].reject(failure);
  await new Promise(setImmediate);
  assert.equal(env.audio_source_state(rejected), -1);
  assert.equal(errors.length, 1, "callback and Promise rejection must report one failure");
  assert.equal(errors[0][1], failure);
  env.audio_source_delete(rejected);

  const success = env.audio_add_buffer(0, 4);
  const firstBuffer = {};
  decodes[1].success(firstBuffer);
  decodes[1].resolve({ duplicate: true });
  await new Promise(setImmediate);
  env.audio_play_buffer(success, 1, false);
  assert.equal(played.at(-1).buffer, firstBuffer, "Promise resolution cannot replace the callback's successful buffer");
  assert.equal(env.audio_source_state(success), 1);
  env.audio_source_delete(success);

  const canceled = env.audio_add_buffer(0, 4);
  env.audio_source_delete(canceled);
  decodes[2].failure(failure);
  decodes[2].reject(failure);
  await new Promise(setImmediate);
  assert.equal(env.audio_source_state(canceled), -2);
  assert.equal(errors.length, 1, "canceled callbacks and Promise rejection cannot revive or report the source");

  const promiseOnlyFailure = env.audio_add_buffer(0, 4);
  decodes[3].reject(failure);
  await new Promise(setImmediate);
  assert.equal(env.audio_source_state(promiseOnlyFailure), -1);
  assert.equal(errors.length, 2);
  env.audio_source_delete(promiseOnlyFailure);

  const promiseOnlySuccess = env.audio_add_buffer(0, 4);
  decodes[4].resolve({});
  await new Promise(setImmediate);
  assert.equal(env.audio_source_state(promiseOnlySuccess), 1);
  env.audio_source_delete(promiseOnlySuccess);
});



test("WASM checker requires the audio extension and permits explicit noaudio fixtures", () => {
  const fixture = fs.mkdtempSync(path.join(os.tmpdir(), "macroquad-runtime-"));
  try {
    const file = path.join(fixture, "fixture.wasm");
    fs.writeFileSync(file, wasm({ returnBytes: true }));
    assert.throws(() => checkBrowserRuntime(root, file), /must import runtime extension/);
    fs.writeFileSync(file, wasm({ audioExport: false, returnBytes: true }));
    assert.equal(checkBrowserRuntime(root, file, false).imports, 1);
    fs.writeFileSync(file, wasm({ imports: [["env", "audio_source_state"]], returnBytes: true }));
    assert.equal(checkBrowserRuntime(root, file).imports, 2);
  } finally {
    fs.rmSync(fixture, { recursive: true, force: true });
  }
});

test("the previous paired runtime is rejected before its loader can stub the selection extension", () => {
  const oldBundle = fs.readFileSync(new URL("fixtures/mq_js_bundle-pre-auto.js", import.meta.url), "utf8");
  assert.equal(sha256(oldBundle), "c4cc9799355e046d0fef61e1078e5524b08e0516fb914246b9808c0d80d367e9");
  const oldRuntime = createRuntime({ bundle: oldBundle });
  assert.throws(() => validateWasmContract(wasm(), oldRuntime, sources.metadata.abi), /missing WASM import implementation: env.webgl_selection_version/);
  const runtime = createRuntime(sources);
  runtime.importObject.env.webgl_selection_version = () => 2;
  assert.throws(() => validateWasmContract(wasm(), runtime, sources.metadata.abi), /selection extension version mismatch/);
});
