# Maintaining the Zubspace fork

This fork prioritizes HTML5 browser games, including Zubspace, using WASM and
WebGL1/2. Chrome and Firefox are the primary browser targets across modern
operating systems: Windows, macOS, Linux and other OS environments providing
the required WebAssembly, WebGL and browser audio capabilities. Native Linux is
a secondary development/test path. The fork retains a narrower API and native
platform matrix than upstream. The GitHub-only package is Macroquad 0.5.0, based on
upstream Macroquad 0.4.16 with vendored Miniquad 0.4.11. Pin the full release
Git revision and paired JavaScript, rather than a registry version. Rust 1.88 is
the declared minimum, including the image 0.25.10 dependency. See
[security contracts](security.md), [patch attribution](patch-ledger.md),
[dependency decisions](dependency-audit.md) and [backend boundaries](backend-boundaries.md).

## Supported configurations

| Configuration | Contract |
| --- | --- |
| `wasm32-unknown-unknown`, `audio`, WebGL1/2 | Primary product target: HTML5 games in Chrome/Firefox across capable operating systems. Uses this fork's included matching runtime and genuine imports. |
| Linux X11/OpenGL, no default features | Secondary native development/test configuration, including historical Zubspace game/server tests. Audio uses the dummy backend. |
| Linux X11/OpenGL, `audio` | Secondary compile target; native audio decoding remains quad-snd's behavior. Full hardware/audio-device coverage is not established. |
| Other native operating systems, Metal, Wayland or alternate native backends | Outside the current native support matrix. This does not restrict the operating systems hosting browser WASM. Retained upstream source is not a native support promise. |

Browser fixtures and the historical application acceptance documented here ran
on Linux hosts with Chrome/Firefox. Cross-OS browser intent does not establish
Windows/macOS, Safari, mobile or every device/driver as independently validated.
The [browser roadmap](browser-roadmap.md) records pending qualification,
lifecycle/input and measurement priorities after 0.5.0.

Window startup permits one initialization attempt per process, and rejects
unsupported native platforms and native Linux configurations other than X11.
Those native guards do not impose an OS restriction on browser WASM. Native graphics
tests use fresh processes rather than resetting Miniquad's global display.
Browser WebGL1 omits MSAA resolves; WebGL2 and native GL exercise them where
supported. Software-rendered/Xvfb fixtures do not establish physical-GPU coverage
or graphics-context restoration.

## WebGL selection

The default `WebGLVersion::Auto` requests a WebGL2 context first. Only a null
result or an exception during that context's creation triggers a WebGL1 attempt.
It does not switch contexts after a shader, capability or rendering failure.
Explicit `WebGL1` and `WebGL2` requests try only that version and fail clearly
if unavailable; if neither automatic attempt succeeds, startup fails before
extension setup or GPU resources are created. To require a particular version:

```rust
use macroquad::miniquad::conf::WebGLVersion;
use macroquad::prelude::Conf;

fn window_conf() -> Conf {
    let mut conf = Conf::default();
    conf.platform.webgl_version = WebGLVersion::WebGL1; // Or WebGL2; Auto is default.
    conf
}
```

Rust derives its GL version, formats and MSAA capabilities from the actual
context. A fallback WebGL1 context does not advertise WebGL2/MSAA resolves.
The paired runtime's `webgl_selection_version` extension v1 is checked before
context creation while the legacy Miniquad ABI stays 2 and audio ABI stays 1.
The earlier `c4cc…67e9` runtime is incompatible with this new Rust build; use the
included regenerated runtime and upgrade Rust/JavaScript together. Auto is a
capability preference, not a performance measurement or context-restoration feature.
Exhaustive matches over `WebGLVersion` must handle the new `Auto` variant.
Applications requiring WebGL2-only shaders/features should request `WebGL2`
explicitly; Auto's WebGL1 fallback requires a compatible rendering path.

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
The workflow's `browser` job is a bounded real-browser graphics smoke gate. It
does not require an audio output device or run the comprehensive local suite.
On Ubuntu 24.04 it
provisions Chrome/ChromeDriver 155.0.8059.79, Firefox 158.0 and GeckoDriver 0.37.1
through setup Actions pinned by commit, Node 22, stable Rust with the WASM target,
Xvfb and Openbox. The serial runner records actual tool versions and artifact
hashes beneath `target/browser-smoke/`; CI uploads its finite JSON/log evidence.
Chrome uses SwiftShader; Firefox runs on the isolated display with software GL.
This is Linux-hosted library validation, not cross-OS or physical-GPU acceptance.

The [Makefile](../Makefile) composes the commands actually used by CI. With those
browser/driver/display tools, Python3.12+, Node22+, GNU Make, rustfmt and the
WASM Rust target available locally, run from the fork root:

```sh
make build          # WASM no-audio/audio checks
make test-ci        # Core checks, formatting, bounded browser smoke
make test           # Core checks and comprehensive local browser suite
make release-check  # Full local pre-release gate, including shared Rust checks
```

`make test-ci` checks runtime regeneration/import contracts, negative/asset tests,
vendoring, formatting and both WASM configurations. Its smoke helper builds only
the small Miniquad texture/pass fixture: actual Chrome/Firefox Auto→2, forced
fallback→1 and strict explicit1/2, plus both-unavailable/strict2-unavailable and
old-runtime failures. Existing pixel/pass/depth, GL and texture/FBO cleanup
assertions remain mandatory. `make browser-smoke` runs that bounded browser portion
alone; `tests/browser_release.sh --smoke` is its underlying script mode.

`make test` preserves the comprehensive local gate. It builds the finite WASM
examples, checks genuine imports/ABIs, then runs
audio resumption from a controlled suspended context after a trusted WebDriver
gesture, decode/cancellation, graphics ownership, UI and codec fixtures in
Chrome and Firefox. Those runtime fixtures use the new Auto default. The
texture/pass fixture checks Auto→2, forced fallback→1, strict explicit1/2,
both-unavailable and strict2-unavailable failure, and old-runtime rejection.
Successful cells check actual GL version/capabilities, pixels, GL errors and
texture/FBO cleanup; failures require the expected error before resources are
created. Both WebGL paths remain supported. Audio evidence
observes the actual decoding context's runtime `resume()` call and later running
state; it does not qualify every browser autoplay-policy setting or audio device.

The full local gate also runs a [generated tile/sprite/bitmap-HUD scene](browser-scene.md)
in explicit WebGL1/2. Its ordered camera-switch and screen-space-label variants
must satisfy independent pixel witnesses and full-frame equivalence, with GL
and texture cleanup checks. Repeated release-WASM CPU/cadence trials record
diagnostics, not timing thresholds, GPU time or a game FPS improvement.

Full evidence is in `target/browser-release/`. It requires a working browser
audio output backend and retains actual running-state proof; a trusted click or
successful decode alone cannot pass it. Missing tools/backends must be diagnosed,
not converted to skipped success. Neither the full audio/lifecycle suite nor the
scene timing matrix is an obligation of the lighter GitHub browser job. Before
publication require full local evidence plus green public CI on the exact reviewed
commit and final asset checks. [AGENTS.md](../AGENTS.md) records the coverage split,
dependencies and process-cleanup rules. `make release-check` also runs the shared
CPU/doctest/backend checks below; CI's separate MSRV/Miri jobs remain required.

The existing native, MSRV and Miri jobs provide supporting shared-Rust/backend
correctness evidence. For those checks install Linux X11/GL development libraries,
ALSA development libraries for native audio, Xvfb and Git for vendor reconstruction:

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
