# Paired browser runtime

`mq_js_bundle.js` is the runtime paired with Zubspace Macroquad **0.5.0**.
It is generated from the published upstream **0.4.16** aggregate plus the
reviewed local fixes, including `audio_source_state` extension v1. Miniquad's
Rust dependency remains the locally patched **0.4.11** source.

With Node.js 22+ from this checkout (no network or minifier required):

```sh
node tools/regenerate_browser_runtime.mjs
node tools/regenerate_browser_runtime.mjs --check
node --test tests/browser_runtime.test.mjs
```

After Cargo resolves this checkout's packages, check local Rust pairing and
an actual WASM binary's imports/constant ABI exports before runtime startup:

```sh
node tools/check_browser_runtime.mjs --source-only
node tools/check_browser_runtime.mjs --wasm target/wasm32-unknown-unknown/debug/examples/audio_lifecycle_regression.wasm
```

The WASM check requires retained audio imports by default; `--no-audio` permits
fixtures without audio code (even if the audio feature was enabled at build time).
Missing imports fail before the loader can manufacture fallback
stubs. The source-only check describes this checkout's local path dependencies;
downstream Git pins must use the concrete commit in the release manifest.

Cargo does **not** install or regenerate this file. Copy it beside your HTML
or use the matching GitHub asset and verify `SHA256SUMS`. The runtime tarball
also contains the base, tools, provenance and licenses needed to regenerate it.
The original upstream hosted bundle is not a substitute. See
[provenance and limitations](../docs/browser-runtime.md) and
[adoption and migration](../docs/fork.md).
