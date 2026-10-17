# Zubspace Macroquad 0.5.0

GitHub-only release of `pkieltyka/macroquad`, package `macroquad` 0.5.0,
tag `zubspace-v0.5.0`, source commit `@COMMIT@`. No crates.io publication.
Based on upstream Macroquad 0.4.16; retains vendored Miniquad 0.4.11 plus the
reviewed texture/framebuffer/X11 patches and original MIT/Apache licenses.
The fork's Rust source base is `8d602d3f8ebb8a3a582ee15d92983bbb81ee3ef6`;
the published runtime aggregate has its separate 0.4.16 crate VCS provenance.

This release names the constrained fork API and makes its paired browser runtime
public and reproducible for HTML5 browser games. The matching runtime is included
in the fork and release assets; private Zubspace JavaScript is not required.
It includes the reviewed image invariants/codec update,
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

Runtime SHA-256: `501cbd90f2494eda6d05407e8328f7a47fa08c5c8c802a9c056d036bbd2c4dac`
(40,664 bytes), upstream runtime base 0.4.16, Miniquad ABI2 and audio plugin ABI1,
plus required `audio_source_state` and `webgl_selection_version` extensions v1.
The context initializer is reproducibly sourced from the patched vendored JS.
Do not substitute the earlier `c4cc…67e9` or an upstream hosted runtime bundle.
Upgrade/roll back the Git pin, lockfile and runtime together.

Default WebGL selection is now `Auto`: try WebGL2, then WebGL1 only if creating
the WebGL2 context fails. Explicit `WebGL1`/`WebGL2` overrides remain strict.
Capabilities reflect the actual context, and unavailable required contexts fail
clearly before GPU allocation. This selects available capabilities, not proven
best performance; no rendering optimization or FPS gain is claimed.

Requires Rust 1.88+. The primary product target is HTML5 games using
`wasm32-unknown-unknown`, WebGL1/2 and browser audio in Chrome/Firefox, intended
across Windows, macOS, Linux and other modern OS environments with those browser
capabilities. Actual browser fixtures ran on Linux hosts; cross-OS, Safari and
mobile acceptance are not established by those runs. Linux X11/OpenGL without
default features or with optional audio remains a secondary development/test
path. Other native backends are outside the current native support matrix.
Finite regressions validate library behavior; hardware audio/GPU coverage is limited.
The public release workflow includes a serial real-browser WASM job for
Chrome/Firefox: audio, ownership, UI and codecs using Auto, plus texture/pass
regressions for Auto→2, forced fallback→1 and strict explicit1/2. Both-unavailable,
strict2-unavailable and old-runtime cells must fail clearly before GPU allocation.
The same job checks a self-contained generated tile/sprite/bitmap-HUD scene in
explicit WebGL1/2: camera-switch and screen-space-label variants must have
equivalent pixels, correct independent landmarks and clean GL/resource behavior.
Three matched release-WASM trials record CPU issuance and RAF cadence; timing
is diagnostic, GPU duration is unavailable and no performance gain is claimed.
Native/MSRV/Miri checks support shared-code correctness. This does not claim
new cross-OS, Safari/mobile or game acceptance.
GPU timing and graphics-context restoration are unavailable.
The source's `docs/browser-roadmap.md` records pending post-0.5.0 browser
qualification and robustness work; this release does not claim those outcomes.

This is local remediation, not an official RustSec patched release or a clean
audit claim. The ttf-parser maintenance advisory remains; dependency deferrals
are documented. Historical Zubspace acceptance used library revision `ea02ddd`
and separately identified application/runtime artifacts, not a new 0.5.0 game pin.
Its Chrome Extreme comparison was a combined n=1 measurement with observed lower
cadence and unmatched rendering-work limits; no performance improvement or isolated
library regression is claimed. Downstream adoption/acceptance is a separate step.
