# Miniquad source and local patch

`miniquad/` contains the published **0.4.11** crate without a version upgrade.
It is pinned by Macroquad's direct path dependency so a consumer of this git
fork also gets the patch; Cargo ignores a dependency's own `[patch]` section.
The upstream API, features, JavaScript source, and platform sources are retained.

- Published artifact: <https://static.crates.io/crates/miniquad/miniquad-0.4.11.crate>
- Artifact SHA256: `dcbaea978d741f3b60ced666b06a2a1e4a2bc81cb8b9dd29f4a40965e1f59e65`
- Upstream source revision from `.cargo_vcs_info.json`:
  `4f13d4a70caf6a8cbf0187a95461658e7812b566`.
- Licenses: the original `miniquad/LICENSE-MIT` and `miniquad/LICENSE-APACHE`.
- Adapted patch: [Miniquad #691](https://github.com/not-fl3/miniquad/pull/691),
  source commit `fbc59729b24dd6c3c4052a729431fa54ffe42b0d`,
  ricande `<ricande@gmail.com>`. Open/unmerged when reviewed 2026-10-16.

`miniquad.patch` is the complete delta from that artifact:

- The attributed texture-cache fix and local CPU regressions in
  `src/graphics/gl.rs` and `src/graphics/gl/cache.rs`.
- Local OpenGL pass deletion releases framebuffer objects without deleting
  caller-owned color/depth/resolve textures, matching Metal's ownership contract.
  WebGL1 version detection accepts the vendor suffix so unsupported MSAA resolves
  are not advertised. WebGL2's standard 16-bit depth textures use a sized
  internal format on creation and resize. GL version copying accepts a null
  result without constructing a C string; format/capability checks reuse the
  cached owned version string.
- Local X11 clipboard/property hardening in
  `src/native/linux_x11/clipboard.rs`: checked native char/short/long buffer
  lengths, scoped XFree cleanup across INCR stages, and thread-local immutable
  message snapshots before FFI. Tests exercise the actual reply/transfer helpers.
- Local X11 drag-and-drop initialization in `src/native/linux_x11/drag_n_drop.rs`:
  `XdndAware` format-32 writes use a native `c_long` item, replacing an inferred
  four-byte integer that Xlib overread on LP64. The production writer is covered
  by native-long contract reads under Miri and a live Xlib property roundtrip.

The latter changes are local fixes, not imported community patches. There are
no other changes inside the vendored crate and no JavaScript/runtime update.
[Backend boundary notes](../docs/backend-boundaries.md) describe the contracts,
primary Xlib sources, intentional ownership migration and validation limits.

Verify or reconstruct it with Python 3.12+ and Git:

```sh
python3 tools/vendor_miniquad.py
python3 tools/vendor_miniquad.py --crate /path/miniquad-0.4.11.crate --output /tmp/miniquad-rebuilt
```

The default command verifies the source archive checksum, applies the patch in
a temporary directory, and compares every vendored file. It leaves this checkout
unchanged. The published crate's `Cargo.lock` is intentionally retained even
though the parent repository ignores lockfiles; include it explicitly when
staging the vendor directory.

Run CPU/cache tests from the fork root with build artifacts outside the verified
source tree, and run the standalone finite browser test:

```sh
CARGO_TARGET_DIR=target/miniquad-tests cargo test --manifest-path vendor/miniquad/Cargo.toml --locked --lib texture_deletion_tests
CARGO_TARGET_DIR=target/miniquad-tests cargo test --manifest-path vendor/miniquad/Cargo.toml --locked --lib property_tests
CARGO_TARGET_DIR=target/miniquad-tests xvfb-run -a cargo test --manifest-path vendor/miniquad/Cargo.toml --locked --lib live_x11_ -- --ignored
python3 tests/texture_cache/browser.py
```

The browser test requires Chrome/chromedriver and Firefox/geckodriver. Firefox
requires a working graphics display; an isolated Xvfb display is sufficient.
It builds both WebGL versions, checks pixel readbacks and GL errors through
32 rounds of deletion/live updates/replacement, and rejects a stale deleted-ID
binding even when pixels remain correct. It also deletes/recreates passes while
owners retain color/depth/resolve attachments: non-MSAA in both WebGL versions,
four-sample MSAA in WebGL2, with and without depth. It checks the advertised
resolve capability against the selected WebGL version. Its loader rejects missing imports.
Results are written beneath `target/texture-cache-web/`.

Xlib's [property manual](https://raw.githubusercontent.com/mirror/libX11/master/man/XGetWindowProperty.man)
requires native long arrays for format-32 writes, and
[XChangeProperty's implementation](https://raw.githubusercontent.com/mirror/libX11/master/src/ChProp.c)
passes those items to `Data32` through a `long` pointer. The XDND wire format is
still 32 bits; native storage is eight bytes on LP64 and four bytes on ILP32.
`xdnd_property_tests` checks the actual production writer's pointer, metadata and
native-long value; its ignored `live_x11_xdnd_awareness` test writes to a real
X window and verifies type/format/count/version with XGetWindowProperty. The
live test checks initialization, not a full external drag-and-drop session.

To check the original failure when updating this patch, reconstruct the clean
crate in a disposable checkout, build the same fixture there before applying
`miniquad.patch`, and run `browser.py --expect-stale`. The original 0.4.11
source reports stale bindings in both WebGL versions. This is a diagnostic
reproduction; it does not establish that every #1006 text corruption report or
mobile crash has the same cause.

When updating Miniquad, refresh the source artifact/hash/revision together,
reassess the patch against upstream, run the CPU and native/WebGL regression
matrix, and update `docs/upstream-issues.md` and the patch ledger. Drop the patch
once the selected upstream version includes equivalent behavior for each fix.
The vendor's JavaScript file is retained as source/provenance; Zubspace's actual
bundled runtime still follows its separately documented pairing procedure.
