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

`miniquad.patch` is the complete delta from that artifact: the source PR's
texture-cache invalidation in `src/graphics/gl.rs` and
`src/graphics/gl/cache.rs`, plus local CPU regressions in the latter file.
There are no other changes inside the vendored crate. In particular, this is
not a general Miniquad security fork or a JavaScript/runtime update.

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
python3 tests/texture_cache/browser.py
```

The browser test requires Chrome/chromedriver and Firefox/geckodriver. Firefox
requires a working graphics display; an isolated Xvfb display is sufficient.
It builds both WebGL versions, checks pixel readbacks and GL errors through
32 rounds of deletion/live updates/replacement, and rejects a stale deleted-ID
binding even when pixels remain correct. Its loader rejects missing imports.
Results are written beneath `target/texture-cache-web/`.

To check the original failure when updating this patch, reconstruct the clean
crate in a disposable checkout, build the same fixture there before applying
`miniquad.patch`, and run `browser.py --expect-stale`. The original 0.4.11
source reports stale bindings in both WebGL versions. This is a diagnostic
reproduction; it does not establish that every #1006 text corruption report or
mobile crash has the same cause.

When updating Miniquad, refresh the source artifact/hash/revision together,
reassess the patch against upstream, run the CPU and native/WebGL regression
matrix, and update `docs/upstream-issues.md` and the patch ledger. Drop the patch
once the selected upstream version includes equivalent cache invalidation.
The vendor's JavaScript file is retained as source/provenance; Zubspace's actual
bundled runtime still follows its separately documented pairing procedure.
