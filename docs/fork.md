# Maintaining the Zubspace fork

This fork serves Zubspace rather than preserving Macroquad's full upstream API
and platform matrix. The GitHub-only fork package is Macroquad 0.5.0, based on
upstream Macroquad 0.4.16 with vendored Miniquad 0.4.11. Pin the full release
Git revision and paired JavaScript, rather than a registry version. Rust 1.88 is
the declared minimum, including the image 0.25.10 dependency. See
[security contracts](security.md), [patch attribution](patch-ledger.md),
[dependency decisions](dependency-audit.md) and [backend boundaries](backend-boundaries.md).

## Supported configurations

| Configuration | Contract |
| --- | --- |
| Linux X11/OpenGL, no default features | Primary native configuration, including Zubspace game/server tests. Audio uses the dummy backend. |
| Linux X11/OpenGL, `audio` | Must compile; native audio decoding remains quad-snd's behavior. This is not a claim of full hardware/audio-device coverage. |
| `wasm32-unknown-unknown`, `audio`, WebGL1/2 | Primary browser configuration. Requires the paired runtime, genuine imports and browser acceptance below. |
| Other operating systems, Metal, Wayland or alternate native backends | Outside this fork's supported matrix. Retained upstream source is not a support promise. |

Window startup permits one initialization attempt per process, and rejects
unsupported platforms and Linux configurations other than X11. Native graphics
tests use fresh processes rather than resetting Miniquad's global display.
Browser WebGL1 omits MSAA resolves; WebGL2 and native GL exercise them where
supported. Software-rendered/Xvfb fixtures do not establish physical-GPU coverage
or graphics-context restoration.

## Deliberate API changes

- Construct CPU images with `Image::from_rgba8(width, height, bytes)?`; use
  `width()`, `height()`, `bytes()` and fixed-length pixel/byte mutation methods.
  Public image fields are private, dimensions remain `u16`, and sub-image
  rectangles must be finite nonnegative integers inside the source image.
  The public `ImageFormat` and image error payload now use image 0.25 types;
  consumers using image directly must align those types.
- `experimental::{scene, collections, state_machine}` and coroutine tweens are
  removed. Keep application state in application-owned structures.
- Raw renderer/backend access, raw texture import/export, raw render-pass export
  and public `QuadGl`/`GlPipeline` are removed. The `miniquad` reexport retains
  selected value types and `date::now`, not backend construction. Use
  `window::rendering_backend()` for shader selection and `load_material` for
  custom shaders. A separate direct Miniquad dependency is outside these API
  guarantees.
- Use `root_ui(|ui| { ... })`; neither the UI nor a reference into it can escape
  that closure. Recursive access to the same UI is rejected. `Ui::new` is
  internal; hidden UI camera/atlas-reset entry points are removed.
- GPU resources, fonts, audio and coroutine handles belong to their originating
  thread and are not `Send`/`Sync`. Retained draw state keeps managed resources
  alive. Render-pass deletion releases FBOs; attachment owners perform texture
  cleanup after the last owner releases them.
- Coroutine handles remain detached: copying or dropping a handle does not
  cancel its task. Explicitly stop tasks and unclaimed completed results when
  replacing Zubspace sound queues. Audio context replacement is not public.
- Uniform uploads require the documented unsafe `ToBytes` contract. Use supported
  arrays/scalars/glam types, or an explicitly reviewed implementation; arbitrary
  application structures no longer qualify implicitly.
- Telemetry returns owned CPU snapshots rather than references into engine
  state. Raw drawcall texture IDs and scene-memory metrics are removed. GPU
  timing is unavailable in this fork: `gpu_timing_supported()` returns false
  and query samples are empty.

The upstream particle crate and examples depending on raw rendering are outside
the supported build. Retained tiled/platformer/profiler development crates are
compatibility build targets; Zubspace does not thereby depend on them.

## Pin Rust and JavaScript together

In Zubspace, set both target-specific Macroquad dependencies to
`git = "https://github.com/pkieltyka/macroquad"` and the same reviewed full `rev`.
Keep `features = ["audio"]` for WASM and no audio feature for native. Commit
Zubspace's resulting lockfile and verify its source identifies that exact Git
revision. Macroquad's local Miniquad path dependency carries its vendored fixes
to Git consumers; a dependency's own `[patch.crates-io]` cannot substitute for
that path dependency.

Cargo fetches Rust sources only. The matching `js/mq_js_bundle.js` is now
maintained in this fork, together with its upstream 0.4.16 base, deterministic
transforms, notices and provenance. Use this source tree's bundle or the matching
GitHub release asset, verifying its `SHA256SUMS` and `release-manifest.json`.
The latter records the final release commit and source/runtime input hashes.
See [runtime provenance](browser-runtime.md) and the [adoption snippet](../README.md).
From this fork checkout, with a resolved local lockfile:

```sh
node tools/regenerate_browser_runtime.mjs --check
node tools/check_browser_runtime.mjs --source-only
node --test tests/browser_runtime.test.mjs
CARGO_TARGET_WASM32_UNKNOWN_UNKNOWN_RUSTFLAGS="-C link-arg=--allow-undefined" cargo build --example audio_lifecycle_regression --target wasm32-unknown-unknown --features audio
node tools/check_browser_runtime.mjs --wasm target/wasm32-unknown-unknown/debug/examples/audio_lifecycle_regression.wasm
```

The source provenance describes local path resolution (null Git sources), while
the release manifest binds the final immutable Git commit for downstream pins.
A consumer must verify both Macroquad and its vendored Miniquad resolve from
that same Git revision in its own lockfile. Runtime regeneration needs no private
Zubspace directories or application assets. Zubspace's existing application gate
and provenance remain application-owned; update those in its separate adoption
thread using the release commit and unchanged runtime output checksum.

The import gate must resolve real implementations, without fallback stubs, and
record the actual Rust package source and full revision. For this fork, require
`audio_source_state` extension v1 while preserving audio plugin ABI 1: pending
`0`, ready `1`, failed `-1`, removed `-2`. Runtime provenance must mark the
extension as required by the current Rust build. Source bytes must be copied
before asynchronous decode, late completions must not resurrect deleted sources,
and both callback and Promise failure must be handled. See the lifecycle
contracts in [security.md](security.md).

After pinning, run Zubspace's release acceptance for genuine input/focus recovery,
audio unlock/loading/playback/failure, arena and LVZ replacement, retained sound
queue cancellation, WASM identity and packaged assets. Follow its `AGENTS.md`
for the acceptance commands and artifacts. Source-only checks and small fixtures
do not replace that game integration checkpoint.

## Reproduce the fork checks

The authoritative automation is [the workflow](../.github/workflows/rust.yml).
It runs on pushes (including tags) and pull requests. To request the same checks
manually, use GitHub Actions → Supported fork checks → Run workflow, or:

```sh
gh workflow run rust.yml --repo pkieltyka/macroquad --ref master
```

Manual dispatch becomes available once this workflow is on the default branch;
`--ref` selects the branch or tag to check. These events run the validation jobs.
Install Linux X11/GL development libraries, ALSA development libraries for native
audio, Xvfb and the WASM Rust target. Browser fixtures additionally require
Chrome/chromedriver and Firefox/geckodriver with a working graphics display.
Use Python 3.12+ and Git for vendor reconstruction. From this repository:

```sh
python3 tools/vendor_miniquad.py
cargo check --all-targets --no-default-features
cargo check --all-targets --features audio
cargo check --all-targets --target wasm32-unknown-unknown --features audio
xvfb-run -a cargo test --tests --no-default-features
xvfb-run -a cargo test --doc --no-default-features
CARGO_TARGET_DIR=target/miniquad-tests cargo test --manifest-path vendor/miniquad/Cargo.toml --locked --lib
CARGO_TARGET_DIR=target/miniquad-tests xvfb-run -a cargo test --manifest-path vendor/miniquad/Cargo.toml --locked --lib live_x11_properties_and_clipboard_roundtrip -- --ignored
xvfb-run -a env CARGO_TARGET_DIR=target/texture-cache-native cargo run --manifest-path tests/texture_cache/Cargo.toml
xvfb-run -a python3 tests/texture_cache/browser.py
xvfb-run -a python3 tests/texture_cache/browser.py --no-build --runtime js/mq_js_bundle.js
```

CI also checks actual Rust 1.88 native/WASM compatibility, scoped formatting,
codec behavior and pinned nightly Miri on x86_64/i686. Use the workflow's exact
test filters and nightly rather than treating Miri as a graphics/audio FFI test.
Its native GPU and browser fixtures complement the interpreter. Paired audio
fixtures use this fork's `js/mq_js_bundle.js` runtime and the intentional WASM linker
setting `-C link-arg=--allow-undefined`; the import gate must still reject
missing implementations. [Security documentation](security.md) identifies the
ownership/audio fixture entry points and remaining evidence boundaries.

## Update and roll back coherently

Before changing either repository, record the full fork revision, Zubspace
revision and lockfile, runtime input/output hashes, provenance and release
artifact identity. Reassess the issue/PR ledger against the selected upstream
revision, preserving attribution. Change one coherent batch, review it, run its
focused matrix and then the combined acceptance checkpoint.

For Miniquad updates, refresh the published archive checksum and source revision,
rebase or drop each local patch independently, regenerate `vendor/miniquad.patch`
and verify every source file using [the reconstruction procedure](../vendor/README.md).
Keep original licenses. For other dependencies, re-audit both supported target
graphs and public type compatibility: latest fontdue still retains the parser
maintenance advisory; glam and auxiliary nanoserde upgrades have explicit
evidence-backed deferrals in [the dependency audit](dependency-audit.md).

Roll back the Rust revision/lockfile, runtime sources/generated bundle/provenance
and packaged release together to the recorded compatible set, then repeat the
import/identity/package checks. Rebuilding only Rust or copying an old JavaScript
file independently can leave an incompatible pair. Do not suppress advisories
or infer performance gains from fewer dependency versions; report actual graph,
artifact-size and representative startup/frame measurements separately.
