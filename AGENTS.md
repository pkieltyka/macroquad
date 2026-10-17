# Macroquad fork development and validation

This fork prioritizes HTML5/WASM/WebGL games in Chrome and Firefox across capable
modern operating systems. Linux native GL is a development/shared-code test path.
The commands below are verified on Linux; they do not establish Windows/macOS,
mobile, physical-GPU or hardware-audio acceptance.

## Commands

Run from the repository root. Make serializes recipes, including under `make -j`,
so only one owned browser/heavy job runs at a time.

| Command | Coverage |
| --- | --- |
| `make build` | Checks all targets for WASM with no audio and with audio. Browser commands separately link executable fixtures. |
| `make test-ci` | WASM checks, scoped formatting, runtime/import/negative/asset tests, vendor reconstruction, and bounded real Chrome/Firefox graphics smoke. |
| `make test` | Core WASM/runtime/vendor checks and the complete strict local browser suite: audio, lifecycle, ownership, UI, codecs, selection, generated scene and diagnostic timing trials. |
| `make release-check` | Formatting, core checks, shared Rust/native/backend CPU/X11 tests and doctests, and the complete local browser suite. Run before a release. |

Helpers `check-runtime`, `check-vendor`, `check-format`, `check-shared`,
`browser-smoke` and `browser-full` compose those commands. Use focused helpers for
small changes; use the full local gate between releases and when affected browser
behavior needs it. Required tools or failed assertions cause failure; never turn
missing prerequisites into a reported pass.

## Prerequisites and evidence

- Rust 1.88+ with `wasm32-unknown-unknown` and rustfmt, Node22+, Python3.12+,
  GNU Make, Git and standard shell utilities. Vendor verification downloads the
  pinned published crate and checks its hash; it needs network access.
- Browser commands need Xvfb, Openbox, Chrome with matching ChromeDriver, Firefox,
  GeckoDriver and Linux graphics libraries. Public CI pins Chrome/ChromeDriver
  155.0.8059.79, Firefox158.0 and GeckoDriver0.37.1. Override `CHROME_BIN`,
  `CHROMEDRIVER_BIN`, `FIREFOX_BIN`, `GECKODRIVER_BIN` when binaries are not on PATH
  under their normal names. See [setup details](docs/fork.md#reproduce-the-fork-checks).
- The full local audio gate requires a functioning browser audio output backend.
  It observes the real runtime AudioContext, controlled suspension, trusted
  WebDriver gesture, runtime resume and subsequent running state. Keep these
  assertions and the source cancellation/deletion checks. Do not bypass autoplay,
  fake context state or replace the gate with decode-only success.
- Shared native checks need X11/GL and ALSA development libraries. MSRV and pinned
  nightly Miri checks remain separate GitHub jobs; `make test-ci` is the portable
  browser/core gate, not a claim to reproduce every workflow job locally.

Browser commands own an isolated 1280×960/96dpi Xvfb display and an Openbox child,
then terminate owned processes. Driver/server cleanup and fixture deadlines are
bounded. Chrome uses SwiftShader and Firefox uses software GL. Never stop an
existing user/game/browser/audio service to run these checks.

Smoke evidence is in `target/browser-smoke/`; full evidence is in
`target/browser-release/`. Both record actual tool versions, artifact hashes,
GL/pixel results and JSON/driver logs. Full scene evidence includes raw repeated
CPU/cadence samples, renderer/source/build identity and PNGs. GPU timing is
unsupported/null; timings are diagnostic with no machine-dependent threshold or
automatic FPS-improvement claim. See [scene limits](docs/browser-scene.md).

## CI and release policy

GitHub CI deliberately runs a bounded browser graphics gate without requiring an
audio device or running full lifecycle/scene timing acceptance. It retains real
Chrome/Firefox explicit WebGL1/2, Auto/fallback, clear failure/old-runtime,
pixel/pass/depth/GL/resource checks. The complete strict suite remains the local
pre-release gate; CI success alone does not claim that suite passed.

Before publication, require the full local release evidence, green public CI on
the exact reviewed commit (including supported/MSRV/Miri), and commit-bound asset
checks. Keep Rust and the included reproducible JavaScript runtime paired. No
Make target commits, pushes, tags, publishes crates/releases or deploys a game.
See the [release notes](docs/releases/zubspace-v0.5.0.md) and
[pairing instructions](docs/fork.md#pin-rust-and-javascript-together).

Preserve upstream notices, verified vendoring and the complete local patch.
`PUNCHLIST.md`, Cargo lockfiles and local evidence retain their existing ignore
policy; never force-add them. Keep historical application evidence distinct from
new library fixtures. Do not modify/build/deploy Zubspace or copy its assets as
part of this fork's validation. Keep the checked scope focused; runtime/code
changes require their own meaningful tests and review.
