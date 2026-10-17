# Paired browser runtime

`mq_js_bundle.js` is the included, fork-owned runtime paired with this fork's
current development source for HTML5/WASM/WebGL games. Release-template
version fields remain**0.5.0** pending release preparation; current input/antialias changes
belong to planned0.5.1 and do not replace the immutable0.5.0 release asset. It requires no private Zubspace files.
It is generated from the published upstream **0.4.16** aggregate plus the
reviewed local fixes, including `audio_source_state` extension v1. Miniquad's
Rust dependency remains the locally patched **0.4.11** source.
The initializer and owned-mouse helper are taken from `vendor/miniquad/js/gl.js`, with an additive
`webgl_selection_version` extension v2. Auto (mode 0) tries WebGL2 then WebGL1
only on context-creation failure; explicit modes 1/2 are strict. Old runtime
bundles without extension v2 are rejected before context creation.
`sample_count <= 1` disables drawing-buffer antialiasing; larger values request
it with browser-selected technique/sample count. Alpha/composition remains unchanged.

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
