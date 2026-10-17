import assert from "node:assert/strict";
import fs from "node:fs";
import test from "node:test";
import { fileURLToPath } from "node:url";
import { createRuntime, runtimeSources } from "../tools/browser_runtime.mjs";

const bundles = {
  aggregate: runtimeSources(fileURLToPath(new URL("..", import.meta.url))).bundle,
  vendor: fs.readFileSync(new URL("../vendor/miniquad/js/gl.js", import.meta.url), "utf8"),
};
function fixture(bundle, blocked = [], throws = []) {
  const requests = [], extensions = [], attributeRequests = [];
  const contexts = Object.fromEntries(["webgl", "webgl2"].map(name => [name, {
    getExtension(extension) { extensions.push([name, extension]); return {}; },
  }]));
  const runtime = createRuntime({ bundle }, { document: { querySelector: () => ({
    focus() {}, getContext(name, attributes) {
      requests.push(name);
      attributeRequests.push({ name, attributes });
      if (throws.includes(name)) throw new Error("Context creation denied");
      return blocked.includes(name) ? null : contexts[name];
    },
  }) } });
  assert.equal(runtime.importObject.env.webgl_selection_version(), 2);
  return { initialize: runtime.importObject.env.init_webgl, requests, extensions, attributeRequests };
}

for (const [name, bundle] of Object.entries(bundles)) {
  test(`${name}: Auto prefers 2 and initializes 1 extensions only on a genuine fallback`, () => {
    const preferred = fixture(bundle);
    preferred.initialize(0, 0);
    assert.deepEqual(preferred.requests, ["webgl2"]);
    assert.deepEqual(preferred.extensions, []);
    for (const failure of ["null", "throw"]) {
      const fallback = fixture(bundle, failure === "null" ? ["webgl2"] : [], failure === "throw" ? ["webgl2"] : []);
      fallback.initialize(0, 0);
      assert.deepEqual(fallback.requests, ["webgl2", "webgl"]);
      assert.ok(fallback.extensions.some(([context, extension]) => context === "webgl" && extension === "WEBGL_depth_texture"));
    }
  });
  test(`${name}: explicit requests remain strict and fail before extension/GPU access`, () => {
    const one = fixture(bundle);
    one.initialize(1, 0);
    assert.deepEqual(one.requests, ["webgl"]);
    const two = fixture(bundle);
    two.initialize(2, 0);
    assert.deepEqual(two.requests, ["webgl2"]);
    for (const version of [1, 2]) {
      const failed = fixture(bundle, ["webgl", "webgl2"]);
      assert.throws(() => failed.initialize(version, 0), new RegExp(`Unable to initialize required WebGL${version}`));
      assert.deepEqual(failed.requests, [version === 1 ? "webgl" : "webgl2"]);
      assert.deepEqual(failed.extensions, []);
    }
  });
  test(`${name}: both unavailable and unsupported modes fail clearly without GPU work`, () => {
    const unavailable = fixture(bundle, ["webgl", "webgl2"]);
    assert.throws(() => unavailable.initialize(0, 0), /Unable to initialize WebGL2 or WebGL1/);
    assert.deepEqual(unavailable.requests, ["webgl2", "webgl"]);
    assert.deepEqual(unavailable.extensions, []);
    const invalid = fixture(bundle);
    for (const mode of [-1, 3, NaN, "0"]) assert.throws(() => invalid.initialize(mode, 0), /Unsupported WebGL request/);
    assert.deepEqual(invalid.requests, []);
  });
}

for (const [name, bundle] of Object.entries(bundles)) {
  test(`${name}: every context attempt carries the requested antialias boolean`, () => {
    for (const antialias of [0, 1]) {
      for (const version of [0, 1, 2]) {
        const state = fixture(bundle, version === 0 ? ["webgl2"] : []);
        state.initialize(version, antialias);
        assert.ok(state.attributeRequests.length > 0);
        for (const { attributes } of state.attributeRequests) {
          assert.ok(attributes, "missing context attributes");
          assert.deepEqual(JSON.parse(JSON.stringify(attributes)), { antialias: antialias === 1 });
        }
      }
    }
  });
  test(`${name}: invalid antialias arguments fail before context access`, () => {
    const state = fixture(bundle);
    for (const antialias of [undefined, -1, 2, NaN, "0", false]) {
      assert.throws(() => state.initialize(1, antialias), /Unsupported WebGL antialias request/);
    }
    assert.deepEqual(state.requests, []);
  });
}
