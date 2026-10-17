# Dependency audit and upgrade queue

Reviewed 2026-10-16. The initial audit below records the versions before the
image upgrade; its implementation and remaining validation are recorded last.
Scope: root and retained auxiliary manifests, root and Zubspace lockfiles,
and `cargo tree --locked --offline -e normal` for Linux native/no-audio,
WASM/audio and optional Linux native/audio. Latest stable versions and declared
Rust requirements were checked against the linked crates.io API records;
compatibility notes come from upstream sources. At the initial audit,
`cargo-outdated` and `cargo-audit` were unavailable and were not installed. No
dependency was changed for the initial audit.

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

The implemented upgrade in `7f51ace` moves image **0.24.9 → 0.25.10** to
remove Zubspace's actual duplicate image/PNG family while preserving the two
supported codecs. Defaults stay disabled with only PNG/TGA enabled, and the
fork declares Rust **1.88** as its minimum. The isolated resolved graph
contains image 0.25.10 and PNG 0.18.1, plus moxcms 0.8.1/pxfm 0.1.30; the
miniz_oxide 0.8.9/0.9.1 split remains. The root lockfile is intentionally
untracked; consuming applications must resolve and pin their own graph. The
combined Zubspace graph and measured artifact sizes are recorded below; neither
deduplication nor this dependency change alone establishes a performance benefit.

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

## Combined Zubspace graph and advisory checkpoint

Zubspace integration commit `725dd5d31ad480e296f3396e532faf2752df3af2`
pins Macroquad and vendored Miniquad to HTTPS Git revision
[`ea02ddd91439fb1c5024f0dc062c6a04b1f3d271`](https://github.com/pkieltyka/macroquad/tree/ea02ddd91439fb1c5024f0dc062c6a04b1f3d271).
The fork revision is published on `zubspace-reviewed-ea02ddd`; upstream and fork
`master` were not advanced by that publication. Zubspace's committed lockfile,
275 locked packages, `docs/third-party-dependencies.json` (274 third-party
notice entries), and
`docs/browser-runtime-provenance.md` identify the actual application graph.

Current compiled client `c975ae2634573605725107ebfcffc0785d632f3a`, fixture
delivery `6167ac530c2d5dfa963ad12cb5ecb1e9190ad049` and retained server
source `ec9090fdc50734b6edaef930b12d70a684fcaab1` do not change dependencies.
They retain the app64/fixture3875 selected graph and
lockfile SHA256
`a7c79561ddba709183c4090c89c131e5b1cd753d1635f55d93a97be30981d162`.
The current lockfile was read directly to reconfirm the versions below; fixture
and notice corrections did not introduce dependency upgrades.

Native/no-audio and WASM/audio now share one image 0.25.10 and PNG 0.18.1;
image 0.24.9/PNG 0.17 are absent. Application features combine BMP/GIF/JPEG/PNG
with the fork's PNG/TGA. Glam remains 0.27.0, fontdue 0.9.4 retains ttf-parser
0.25.1, miniz_oxide 0.8.9/0.9.1 remains duplicated, and nanoserde is absent from
the application graph. Optional native audio retains quad-snd 0.2.8 and
quad-alsa-sys 0.3.2; normal native builds exclude them. The evaluated deferrals
above stand; no unrelated major upgrade is required by this checkpoint.

The recorded re-audit used cargo-audit 0.22.2 and RustSec database revision
`64f92cdc64401fab47baa10e2e11fe382aa57155` (1,334 advisories). The Git-pinned
lockfile has no vulnerability-table entries and retains informational
RUSTSEC-2026-0192 for unmaintained ttf-parser. Registry Macroquad 0.4.16 also
reports informational RUSTSEC-2025-0035. Its omission for the Git source does
not establish an official patched version or advisory closure. No ignore rules
or advisory suppressions were added, and this is not an audit-clean claim.

Application checks used Rust 1.98.1/Cargo 1.98.0; its configured compiler is
1.97.1. The fork's separately tested Rust 1.88 minimum does not apply to the
whole application: the inherited native networking graph includes uuid with a
declared Rust 1.89 minimum.

The following are intermediate, pre-respawn-repair observations of the
`725dd5d` application checkpoint, not final shipping artifacts. Matched
optimized-release artifacts use the same Rust 1.98.1 producer. WASM
grew from 6,440,789 to 6,576,012 bytes (+135,223; +2.10%); paired JavaScript
stays byte-identical at 38,256 bytes. Application API migrations and ownership
changes contribute too, so the difference is not attributed solely to image.
One startup sample per browser supports no speedup/regression inference.
Pre-repair browser runs, earlier container images and cadence/startup captures
remain intermediate. The archived app64 source
`64c8f58ca4c58c5b30baf03f55fd4510f2f8c174` produced a host release WASM of
6,573,834 bytes (+133,045/+2.07% against the 6,440,789-byte baseline); its
JavaScript was 38,256 bytes. Fixture/notice head3875 preserved nine app64 artifact
hashes, including the lockfile. Those sizes, its accepted image pair/Compose
checkpoint and17 passing continuation invocations are historical after the
reviewed Ship/spectator repair `fb3fbd3`, not measurements of its new binaries.
[The patch ledger](patch-ledger.md#intermediate-application-acceptance-checkpoints)
records both product failures, independent repairs and exact evidence roots.

The fb3 host release/package checkpoint produced WASM 6,573,234 bytes;
its former973a646 client image is historical after the c975 client repair.
The current c975 host release WASM is 6,573,379 bytes, SHA256
`2359c9e9e1fee912b5e51ffa0f310fe41b4167164606d39d0999e52d7aeed0c0`,
with unchanged 38,256-byte paired JavaScript. This is +132,590 bytes (+2.06%)
against the same-producer 6,440,789-byte baseline, an artifact-size observation
rather than a causal dependency or performance claim. The ec909 debug/release
server, lockfile, paired runtime and metadata remain unchanged. Current client
image f2f39d8 uses a different Rust producer; its 6,524,236-byte WASM must not
replace the host measurement above.

Objective-first and targeted Ship renewals passed. The c975 reorder retry and
Chrome hostile two-bot run passed with actual observed child-explosion proof;
the new client/retained-server image pair passed isolated Compose. Firefox
two-bot acceptance subsequently passed its full13-file/one-QA audit, primary-only;
a separate genuine-input Firefox primitive-JSON upload/GET probe passed exact
whole-JSON/raw-byte equality. Earlier nonlethal/identity REDs remain retained;
fixture-only corrections preserve all ten compiled artifact identities.
Private latest-user integration2008 passed its four bounded browser cells after
a reviewed external-adapter renewal; its separate graph/artifacts are not the
delivery graph above. The final five focused delivery outcomes and eight cadence/two startup cells
also passed independent verification. Chrome Extreme nevertheless measured
51.317 versus60.021Hz (about14.5% lower) in one comparable software-renderer pair;
it remains a material concern, with workload equivalence not established
(candidate accepted a much earlier death/respawn, baseline rejected respawn).
The completed measure-and-report outcome establishes no isolated library
regression or benefit. Final documentary review is a separate handoff step.
[The ledger](patch-ledger.md#final-release-measurement-checkpoint) records all
four n=1 cadence pairs, exact source/image tuples and proof limits. Dependency
deferrals and advisory limitations above stand; these observations establish
no official advisory closure or performance win.
