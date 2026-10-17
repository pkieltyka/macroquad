# Macroquad for Zubspace

This is `pkieltyka/macroquad`, a deliberately narrower fork of
[Macroquad](https://github.com/not-fl3/macroquad) for Zubspace. It requires
**Rust 1.88+** and supports **Linux X11/OpenGL** and **browser WASM/WebGL1/2**.
Release **0.5.0** is distributed through [GitHub releases](https://github.com/pkieltyka/macroquad/releases/tag/zubspace-v0.5.0),
with its paired JavaScript runtime included in this source tree and as a release asset.
The upstream Rust/runtime base is 0.4.16; the patched vendored Miniquad is 0.4.11.
This GitHub-only fork retains the package name `macroquad` and disables crates.io
publication. Pin the full release commit from `release-manifest.json`:

The fork deliberately removes scene/storage/tween and raw backend APIs and uses
scoped UI access. It is not a drop-in replacement across upstream platforms.
Local remediation and regression evidence do not establish an official RustSec
patched release, a clean dependency audit, or a measured performance improvement.

- [Supported APIs, build matrix, updates and rollback](docs/fork.md)
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
