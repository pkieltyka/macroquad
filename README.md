# Macroquad for Zubspace

This is `pkieltyka/macroquad`, a deliberately narrower fork of
[Macroquad](https://github.com/not-fl3/macroquad) for **HTML5 browser games using
WASM and WebGL1/2**, including Zubspace. **Chrome and Firefox** are the primary
browser targets, intended to run on Windows, macOS, Linux and other modern
operating systems with the required WebAssembly, WebGL and browser audio capabilities.
The default selects WebGL2 when context creation succeeds and falls back to
WebGL1 otherwise; explicit WebGL1/2 requests remain strict overrides.
Building the Rust code requires **Rust 1.88+**.

Current browser fixtures run on Linux with Chrome and Firefox; they do not
establish testing across every OS, device or GPU, or Safari/mobile acceptance.
Native Linux X11/OpenGL is a secondary development and test path.
GitHub CI includes bounded real Chrome/Firefox WebGL1/2 and Auto/fallback graphics
smoke, alongside shared Rust correctness checks. Comprehensive browser/audio and
visual timing checks run locally between releases. Use `make build`, `make test-ci`,
`make test` or `make release-check`; [AGENTS.md](AGENTS.md) describes prerequisites,
coverage and evidence.
This tree prepares **0.5.1**, with its matching fork-owned JavaScript runtime;
[prepared notes and migration](docs/releases/zubspace-v0.5.1.md) describe the changes.
It has not been published. The historical [0.5.0 release](https://github.com/pkieltyka/macroquad/releases/tag/zubspace-v0.5.0)
and its runtime asset remain immutable and differ from this tree. Consumers need
no private Zubspace runtime files.
The upstream Rust/runtime base is 0.4.16; the patched vendored Miniquad is 0.4.11.
This GitHub-only fork retains the package name `macroquad` and disables crates.io
publication. Pin the full release commit from `release-manifest.json`:

The fork deliberately removes scene/storage/tween and raw backend APIs and uses
scoped UI access. It is not a drop-in replacement across upstream platforms.
Local remediation and regression evidence do not establish an official RustSec
patched release, a clean dependency audit, or a whole-frame/game FPS improvement. Qualified generated-scene CPU API
measurements are recorded in the prepared notes.
Automatic selection chooses capabilities, not a measured best-FPS configuration.

- [Supported APIs, build matrix, updates and rollback](docs/fork.md)
- [Pending browser priorities after 0.5.0](docs/browser-roadmap.md)
- [Security contracts and remaining boundaries](docs/security.md)
- [Upstream remediation explanation](docs/upstream-remediation.md)
- [Patch attribution and decisions](docs/patch-ledger.md)

## Adopt the release

Replace `RELEASE_COMMIT` below with the full commit in the GitHub release manifest.
Cargo fetches Rust sources and the patched Miniquad; copy JavaScript separately
from the same release and verify `SHA256SUMS`.

```toml
[target.'cfg(not(target_arch = "wasm32"))'.dependencies]
macroquad = { git = "https://github.com/pkieltyka/macroquad", rev = "RELEASE_COMMIT", default-features = false }

[target.'cfg(target_arch = "wasm32")'.dependencies]
macroquad = { git = "https://github.com/pkieltyka/macroquad", rev = "RELEASE_COMMIT", default-features = false, features = ["audio"] }
```

Serve the paired `mq_js_bundle.js` beside your HTML, then load your compiled WASM:

```html
<canvas id="glcanvas" tabindex="1"></canvas>
<script src="mq_js_bundle.js"></script>
<script>load("your_app.wasm");</script>
```

See [runtime generation and ABI checks](js/README.md),
[API migration and supported checks](docs/fork.md), and
[release preparation](docs/releases/README.md). Rust dependencies and JavaScript
must be upgraded or rolled back together. Downstream game acceptance is a separate
integration step; historical Zubspace acceptance does not validate a future pin.

Macroquad is originally by Fedor Logachev and contributors. Its original
[upstream documentation](https://github.com/not-fl3/macroquad) describes a broader
API and platform matrix. This fork preserves the original [MIT](LICENSE-MIT) and
[Apache-2.0](LICENSE-APACHE) licenses and [patch attribution](docs/patch-ledger.md).
