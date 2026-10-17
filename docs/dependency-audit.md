# Dependency audit and upgrade queue

Reviewed 2026-10-16. The initial audit below records the versions before the
image upgrade; its implementation and remaining validation are recorded last.
Scope: root and retained auxiliary manifests, root and Zubspace lockfiles,
and `cargo tree --locked --offline -e normal` for Linux native/no-audio,
WASM/audio and optional Linux native/audio. Latest stable versions and declared
Rust requirements were checked against the linked crates.io API records;
compatibility notes come from upstream sources. `cargo-outdated` and
`cargo-audit` were unavailable and were not installed. No dependency was changed
for the initial audit.

## Direct dependencies at the initial audit

All resolved versions below are already the latest allowed by their current
requirements. A lockfile refresh alone cannot adopt the newer version families.

| Dependency | Manifest requirement | Resolved / latest in range | Latest stable | Decision |
| --- | --- | --- | --- | --- |
| [miniquad](https://crates.io/api/v1/crates/miniquad) | `=0.4.11`, vendored | 0.4.11 + local patch | 0.4.11 | Keep the correctness patch and provenance in [vendor/README.md](../vendor/README.md). |
| [quad-rand](https://crates.io/api/v1/crates/quad-rand) | `0.2.3` | 0.2.3 | 0.2.3 | Current. |
| [glam](https://crates.io/api/v1/crates/glam) | `0.27`, `scalar-math` | 0.27.0 | 0.34.1 | Evaluate a separate public API migration. |
| [image](https://crates.io/api/v1/crates/image) | `0.24`, defaults off, PNG/TGA | 0.24.9 | 0.25.10 | First upgrade batch. |
| [macroquad_macro](https://crates.io/api/v1/crates/macroquad_macro) | `0.1.8`, local path | 0.1.8 | 0.1.8 | Current; preserve local source changes. |
| [fontdue](https://crates.io/api/v1/crates/fontdue) | `0.9` | 0.9.4 | 0.9.4 | Current, but its parser requires a maintenance decision. |
| [backtrace](https://crates.io/api/v1/crates/backtrace), optional | `0.3.60` | 0.3.76 | 0.3.76 | Current; declares Rust 1.82. |
| [log](https://crates.io/api/v1/crates/log), optional | `0.4` | 0.4.34 | 0.4.34 | Current; declares Rust 1.71. |
| [quad-snd](https://crates.io/api/v1/crates/quad-snd), optional | `0.2` | 0.2.8 | 0.2.8 | Current published version. |
| [nanoserde](https://crates.io/api/v1/crates/nanoserde), tiled/particles | `0.1` | 0.1.37 | 0.2.1 | Evaluate auxiliary-crate migration separately. |

The root lockfile resolves nanoserde through `tiled`; particles has the same
requirement but is outside that root graph. Platformer/profiler depend on the
local Macroquad; the procedural-macro crate has no third-party dependencies.
Vendored Miniquad example-only dev dependencies are outside these runtime
graphs and are not upgrade targets in this batch.

## Bounded upgrade decisions

**Image first:** migrate to 0.25.10 while preserving disabled defaults and
PNG/TGA support. Zubspace already directly uses 0.25.10, so its lockfile currently
contains image 0.24.9/0.25.10 and PNG 0.17.16/0.18.1. Consolidating those families
is a concrete reason to upgrade; smaller WASM or faster startup remains a
measurement, not a promised result. Image 0.25.10 also adds unconditional
`moxcms`; application feature unification must be inspected.

Image 0.25.10 requires Rust 1.88, versus 1.63 for 0.24.9. This does not increase
Zubspace's existing image requirement, but the fork has no declared Rust minimum
and must document its supported toolchain. Macroquad publicly exposes image's
`ImageFormat` and error types. The image-invariant PNG regression uses the
removed `ImageOutputFormat`; migrate it and review decoding/export APIs,
malformed-image behavior and codec features. The
[image changelog](https://github.com/image-rs/image/blob/v0.25.10/CHANGES.md)
documents API changes, TGA fixes and the Rust requirement. Validate native and
WASM before pinning the upgraded fork in Zubspace.

**Glam: evaluation required; defer a blanket bump.** Version 0.34.1 requires
Rust 1.85 and removes camera matrix constructors used by this fork, including
`Mat4::look_at_rh`, `perspective_rh_gl` and `orthographic_rh_gl`; replacements
live in camera modules. Macroquad reexports glam types, so this is a consumer
API migration, with math and uniform-layout regression implications. Preserve
`scalar-math` during evaluation. Zubspace currently has one glam version, so
there is no demonstrated duplicate cleanup or browser performance benefit.
Adopt only after a focused compatibility/benefit review; otherwise record an
explicit deferral. See the
[glam changelog](https://github.com/bitshifter/glam-rs/blob/0.34.1/CHANGELOG.md).

**Nanoserde: auxiliary evaluation; defer from the first upgrade batch.**
Published 0.2.1 requires Rust 1.81. Tiled has a handwritten `DeJson`
implementation; particles uses JSON derives/proxies. Review their compatibility
and JSON round trips, selecting only `json`/`std` features where supported.
Neither crate is used by Zubspace today. An evaluated deferral is acceptable
when the migration provides no supported-use benefit; do not adopt unpublished
upstream master merely because its manifest has a newer version. See the
[published record](https://crates.io/api/v1/crates/nanoserde/0.2.1) and
[format feature documentation](https://github.com/not-fl3/nanoserde#crate-features).

## Transitive and advisory boundaries

Fontdue 0.9.4 unconditionally uses ttf-parser 0.25.1 in both supported runtime
graphs. That is the latest parser release, but
[RUSTSEC-2026-0192](https://rustsec.org/advisories/RUSTSEC-2026-0192.html)
classifies it as **unmaintained**, with no patched versions. This is a maintenance
advisory, not evidence of a new memory-safety vulnerability. Evaluate a maintained
parser/backend or a supported font-feature boundary and record adoption or
deferral. Latest fontdue does not clear the advisory. Zubspace uses bitmap/HTML
text in the browser, but Macroquad still initializes its default font; lack of
dynamic text calls does not remove the dependency.

WASM/audio has quad-snd but no Audrey decoding chain. Native/no-audio has no
quad-snd. Optional native/audio adds
`quad-snd -> audrey 0.3.0 -> lewton 0.9.4 -> ogg 0.7.1 / smallvec 0.6.14`.
Audrey is latest published and constrains lewton to `^0.9`; latest lewton is
0.10.2. This requires parent-crate/backend work, not a forced lockfile update,
and is deferred without a native-audio need. See
[quad-snd dependencies](https://crates.io/api/v1/crates/quad-snd/0.2.8/dependencies)
and [Audrey dependencies](https://crates.io/api/v1/crates/audrey/0.3.0/dependencies).

Other duplicate families are miniz_oxide 0.8.9/0.9.1 in the root graph and
hashbrown 0.15.5/0.17.1 plus smallvec 0.6.14/1.16.3 in Zubspace. The older
families come through different parent requirements; do not force incompatible
majors merely to deduplicate. Zubspace's unrelated networking dependency
duplicates were outside this audit.

A finite package-name comparison against the current
[RustSec database](https://github.com/RustSec/advisory-db) found historical
advisories for resolved image, hashbrown, smallvec and optional zlib-rs; their
resolved versions satisfy those published patched ranges. Macroquad's
[RUSTSEC-2025-0035](https://rustsec.org/advisories/RUSTSEC-2025-0035.html) still has
no published patched version; local remediation evidence is tracked separately
in [security.md](security.md). The parser maintenance advisory also remains.
This manual check is not a clean `cargo-audit` result or a complete Zubspace
security audit. Do not suppress advisories to report success.

After each adopted upgrade, record the exact manifests/lockfiles, enabled
features, toolchain and advisory DB revision; recheck native/no-audio and
WASM/audio, optional native-audio compatibility, public API regressions, and
Zubspace's resolved duplication. Preserve the vendored Miniquad fix. Review and
commit each coherent batch before the next upgrade; keep an evidence-backed
decision for candidates that remain deferred.

## Image upgrade implementation

The fork now requires image `0.25.10`, with defaults disabled and only PNG/TGA
enabled, and declares Rust **1.88** as its minimum. The isolated resolved graph
contains image 0.25.10 and PNG 0.18.1, plus moxcms 0.8.1/pxfm 0.1.30; the
miniz_oxide 0.8.9/0.9.1 split remains. The root lockfile is intentionally
untracked; consuming applications must resolve and pin their own graph. Exact
Zubspace image/PNG consolidation and packaged size/startup measurements remain
part of fork integration, with no performance claim from this batch.

The removed `ImageOutputFormat` test API is migrated to `ImageFormat`.
Macroquad's public image-format parameters/reexport and `Error::ImageError`
now refer to image 0.25 types: callers that separately depend on image 0.24
must migrate those types together. PNG/TGA decoding and native PNG export keep
their existing contract, including export's row flip. Filesystem PNG export
remains unsupported on WASM; the browser fixture exercises in-memory PNG
encoding and both supported decoders instead.

Regression entry points are `cargo test --test image_codecs`, the existing
image-dimension library tests and GPU transfer example, and
`examples/image_codec_regression.rs` with `tests/image_codec_regression.html`.
The new tests cover known pixel channels/alpha, top-left TGA orientation,
inferred/explicit PNG format, truncated/invalid/wrong-format rejection and
native export round trips. Actual Rust 1.88.0 checks cover all targets under
native/no-audio, native/audio and WASM/audio, and the new native codec tests pass
with that compiler. Its WASM/audio fixture also passes in Chrome and Firefox.
CI retains native/no-audio and WASM/audio minimum-compiler checks alongside
stable-toolchain coverage.

Decisions for the current Zubspace integration:

- Defer glam's breaking migration: the current graph has one version, scalar
  math remains intentional, and no measured browser benefit offsets the camera,
  public-type and uniform compatibility work. Revisit for a concrete math defect
  or a demonstrated performance need.
- Defer nanoserde's auxiliary migration: Zubspace uses neither auxiliary crate,
  and no failing supported JSON case was identified. Keep 0.1.37 rather than
  undertake parser/derive changes solely for a newer version number; revisit for
  an auxiliary-format defect or a supported consumer's requirement.
- Retain fontdue's current parser/API for this integration and record the
  unmaintained advisory as outstanding. Latest fontdue offers no maintained
  parser upgrade, while replacing its parser/backend requires separate font
  metrics, custom-font and rendering compatibility work. Revisit an upstream
  maintained-parser release or a dedicated backend migration; bitmap gameplay
  and a shipped default font do not resolve the advisory. No capability is
  removed or advisory suppressed by this decision.

Final accumulated-code and Zubspace graph/advisory re-audits remain pending;
these dependency decisions do not establish security or release acceptance.
