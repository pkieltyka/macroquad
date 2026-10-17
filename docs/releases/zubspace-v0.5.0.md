# Zubspace Macroquad 0.5.0

GitHub-only release of `pkieltyka/macroquad`, package `macroquad` 0.5.0,
tag `zubspace-v0.5.0`, source commit `@COMMIT@`. No crates.io publication.
Based on upstream Macroquad 0.4.16; retains vendored Miniquad 0.4.11 plus the
reviewed texture/framebuffer/X11 patches and original MIT/Apache licenses.
The fork's Rust source base is `8d602d3f8ebb8a3a582ee15d92983bbb81ee3ef6`;
the published runtime aggregate has its separate 0.4.16 crate VCS provenance.

This release names the constrained fork API and makes its paired browser runtime
public and reproducible. It includes the reviewed image invariants/codec update,
scoped graphics/UI ownership, resource and coroutine lifetimes, terminal audio
decode/cancellation, and CPU telemetry changes. See the source tree's
`docs/fork.md`, `docs/security.md` and `docs/patch-ledger.md` for migrations,
security boundaries and attribution. Scene/storage/tween and raw backend APIs
are removed; UI access is scoped through `root_ui(|ui| { ... })`.

Pin the exact source revision for both targets:

```toml
[target.'cfg(not(target_arch = "wasm32"))'.dependencies]
macroquad = { git = "https://github.com/pkieltyka/macroquad", rev = "@COMMIT@", default-features = false }

[target.'cfg(target_arch = "wasm32")'.dependencies]
macroquad = { git = "https://github.com/pkieltyka/macroquad", rev = "@COMMIT@", default-features = false, features = ["audio"] }
```

Cargo fetches Rust sources including the patched Miniquad; it does not install
JavaScript. Copy the attached `mq_js_bundle.js` into your browser assets and serve
it locally before `load("your_app.wasm")`. Verify downloads against `SHA256SUMS`.
`release-manifest.json` binds this commit/tag to source/runtime hashes and supported
targets. The source tarball is a deterministic Git archive of this commit; the
runtime tarball contains public base/transforms/provenance/licenses and can be
regenerated with `node tools/regenerate_browser_runtime.mjs`.

Runtime SHA-256: `c4cc9799355e046d0fef61e1078e5524b08e0516fb914246b9808c0d80d367e9`
(38,256 bytes), upstream runtime base 0.4.16, audio plugin ABI 1 plus required
`audio_source_state` extension v1. Do not substitute an upstream hosted bundle.
Upgrade/roll back the Git pin, lockfile and runtime together.

Requires Rust 1.88+. Supported scope: Linux X11/OpenGL without default features
or with optional audio, and browser WASM/WebGL1/2 with audio. Other retained
platform sources are outside the support matrix. Finite native and browser
regressions validate library behavior; hardware audio/GPU coverage is limited.
GPU timing and graphics-context restoration are unavailable.

This is local remediation, not an official RustSec patched release or a clean
audit claim. The ttf-parser maintenance advisory remains; dependency deferrals
are documented. Historical Zubspace acceptance used library revision `ea02ddd`
and separately identified application/runtime artifacts, not a new 0.5.0 game pin.
Its Chrome Extreme comparison was a combined n=1 measurement with observed lower
cadence and unmatched rendering-work limits; no performance improvement or isolated
library regression is claimed. Downstream adoption/acceptance is a separate step.
