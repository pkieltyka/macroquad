# Browser runtime provenance

Fork release 0.5.0 pairs its Rust source with `js/mq_js_bundle.js`, SHA-256
`501cbd90f2494eda6d05407e8328f7a47fa08c5c8c802a9c056d036bbd2c4dac`
(40,664 bytes). It adds automatic WebGL selection to the earlier reviewed
runtime. Historical acceptance used
`ea02ddd91439fb1c5024f0dc062c6a04b1f3d271` and runtime SHA-256
`c4cc9799355e046d0fef61e1078e5524b08e0516fb914246b9808c0d80d367e9`
(38,256 bytes), retained only as a negative-test input at
`tests/fixtures/mq_js_bundle-pre-auto.js`. Those old game results do not validate
this new Auto runtime or a new 0.5.0 game pin.

The unchanged published Macroquad **0.4.16** aggregate is committed at
`vendor/browser-runtime/macroquad-0.4.16.js`. Its crate VCS revision is
`5e9b5ca912ac65962c05c0da842a4a70eaae34b9`, archive SHA-256
`1cbd54d99d4d3ac59deed7ee4927bbbf4eb33a55635530af3be9395a55746090`,
and base bundle SHA-256
`4bf663a44a06c113bed92e0596c58139cd9db7f0d67ef2cc49ca06016e0b5ea0`.
The archive is https://static.crates.io/crates/macroquad/macroquad-0.4.16.crate.
The aggregate includes Miniquad, quad-snd, sapp-jsutils and quad-net JavaScript;
the archive pins that aggregate, not independent historical revisions of every
component. Preserve its original copyright notice in
`vendor/browser-runtime/LICENSE-MIT`, also included in the runtime release asset.
The source repository retains both upstream license files.

`tools/browser_runtime.mjs` retains the reviewed deterministic transformations.
Each input anchor must occur exactly once. They add a focusable fallback canvas,
bounded download copying, import lookup by module, an optional WASM response
callback, quiet audio unlock, a scoped quad-net registrar (fixing upstream's
strict-mode startup exception), a terminal newline and the audio state extension.
The base retains upstream's combined document focus/visibility handling.

The context initializer is copied deterministically from the patched vendored
`vendor/miniquad/js/gl.js`; its full input hash is recorded in provenance and
the runtime asset includes that source and Miniquad's original license files.
Auto requests WebGL2 first and falls back to WebGL1 only when creation returns
null or throws. Explicit1/2 do not fall back. Unsupported modes and unavailable
required contexts fail before extension/GPU calls. Only the actual WebGL1 path
initializes legacy extensions; Rust capabilities follow the actual context.
`env.webgl_selection_version` returns extension version1, required by current
Rust before startup. Miniquad ABI2 and existing plugin/audio ABI1 remain intact;
the strict import gate rejects the old bundle before fallback stubs can hide
the missing extension. This does not claim faster frames or context restoration.

Audio plugin ABI remains 1. `env.audio_source_state` extension v1 reports pending
0, loaded 1, failed -1 and unknown/deleted -2. It copies bytes before decode,
cancels pending handles, rejects wrapped IDs and prevents late callback or
Promise completion from resurrecting deleted sources. Callback/Promise duplicate
settlement reports a single terminal result. Current fork Rust with `audio`
requires this extension; the old boolean API remains implemented.

`js/mq_js_bundle.provenance.json` describes upstream input/output hashes, plugin
ABIs and local Rust package resolution. Its null sources refer to this checkout's
path packages, not a registry release. Final release identity lives in the
generated `release-manifest.json` (including concrete `rustPairing` Git sources),
avoiding a self-referential committed SHA. The Rust audio dependency is pinned
to quad-snd 0.2.8, matching this runtime's reviewed plugin implementation.
The final manifest binds the exact Git commit/tag, source archive, vendored patch,
runtime inputs and assets. See [generation/check commands](../js/README.md).

The build gate evaluates the real runtime and checks every import before fallback
stubs, instantiates without calling main, and checks constant ABI exports. It does
not replace real-browser graphics/audio fixtures or downstream application
acceptance. The optional response hook does not supply application identity,
RTC/loading or game-specific audio-unlock bridges; those remain application code.
WebGL1 has no MSAA resolves; this fork does not advertise them. No context
restoration, GPU timing or physical-GPU coverage is implied.
