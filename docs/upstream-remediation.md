# Upstream remediation explanation

This is a reviewable explanation for upstream discussion, not a published issue
or pull request. The fork starts at Macroquad
`8d602d3f8ebb8a3a582ee15d92983bbb81ee3ef6` and serves Zubspace's Linux X11/GL
and browser WebGL use. [The patch ledger](patch-ledger.md) records reviewed PR
heads and attribution; [the issue shortlist](upstream-issues.md) distinguishes
reproductions from stale reports and measurement candidates.

## Reusable fixes and their boundaries

| Change and source | Local implementation and review evidence |
| --- | --- |
| [Macroquad #1074](https://github.com/not-fl3/macroquad/pull/1074), Benoît Rouleau: lazy UI material | `8173622` renders UI commands before material/atlas work and preserves default state even for empty frames. Native/browser regressions check lazy initialization, reuse and texture/material reset. No measured Zubspace startup/frame speedup is claimed. |
| [Macroquad #956](https://github.com/not-fl3/macroquad/pull/956), Maja Kądziołka: uniform byte representation | `8173622` adapts the unsafe `ToBytes` contract to actual arrays/slices, then locally validates scalar/array lengths, uniform metadata, initialized storage and alignment. CPU/Miri and compile-fail regressions cover these assumptions. This is broader hardening than simply applying the PR. |
| [Miniquad #691](https://github.com/not-fl3/miniquad/pull/691), ricande: deleted-texture cache | `1414d74` invalidates current/saved bindings for deleted textures without disturbing unrelated state. Original 0.4.11 reproduces stale deleted-ID binds even when pixels remain correct. Native and Chrome/Firefox WebGL1/2 regressions check pixels, GL errors and cache behavior. This is relevant to [Macroquad #1006](https://github.com/not-fl3/macroquad/issues/1006), not proof that every reported text artifact shares the cause. |
| [Macroquad #973](https://github.com/not-fl3/macroquad/pull/973), kampffrosch94: independent audio lifetime | `9b642df` adapts the isolation direction, with locally retained owners, deferred cleanup and cancellation guards. The scheduler independently releases borrows before polling or destroying user futures/results. CPU/Miri and paired WebAudio regressions exercise cancellation, reentry, shutdown and late completion. The PR alone was not accepted as an advisory-wide fix. |
| Local image/transfer validation | `1c8a686` makes image fields private and validates dimensions, byte lengths, rectangles and transfer arithmetic. Checked constructors and fixed-length mutation prevent invalid safe images reaching raw transfers. Image 0.25.10/MSRV 1.88 migration is separately recorded in `7f51ace`; neither change proves a size/startup win. |
| Local X11 property boundaries | `f347a86` uses Xlib's native char/short/long layout for formats 8/16/32; format16 previously overread using int size. Checked replies and scoped cleanup cover INCR stages. TLS snapshots replace mutable global clipboard storage without a borrow crossing FFI. Actual-helper Miri tests run on x86_64/i686; live Xvfb checks properties and self-selection, not multi-client INCR timing. |
| Local GL render-pass/depth/capability contracts | `f347a86` makes pass deletion non-owning for attachments, matching Metal. Native/WebGL regressions delete/recreate passes while retaining color/depth/resolve owners. WebGL1 vendor-suffixed strings no longer advertise resolves, WebGL2 standard depth creation/resize uses its required sized format, and the initial GL version copy accepts null. This does not implement context restoration or validate every format/GPU. |

The Miniquad archive, checksum, original licenses, complete delta and exact
reconstruction are in [vendor/README.md](../vendor/README.md). Its local changes
are explicitly distinguished from the attributed community patch. A reusable
upstream submission should retain the focused regression and contract for each
fix rather than present the whole Zubspace fork as a compatible patch.

Browser runtime fixes belong with their actual JavaScript sources and provenance
in Zubspace: strict-mode-safe registrar declaration ([#1055](https://github.com/not-fl3/macroquad/issues/1055)),
focus/input recovery, real import resolution, bounded file copies and terminal
audio decode/deletion state. The audio state extension intentionally pairs Rust
and JavaScript. Applying only one side is not a valid upstream or game release.
The maintenance procedure is in [fork.md](fork.md).

## Fork-specific API pruning

The Zubspace fork removes scene/collections/state-machine/tween APIs, raw renderer
and texture/render-pass escape hatches, replaceable engine/audio contexts and
unsupported examples. Scoped `root_ui` prevents a UI reference escaping its
owner. Thread-bound resource handles and retained attachment owners replace
unconstrained aliases; owned telemetry snapshots replace references into engine
state. The supported surface and migration choices are in [fork.md](fork.md),
with detailed contracts and current remediation status in [security.md](security.md).

Those choices are intentional breaking changes for this application. They
should not be described as preserving upstream scene, plugin, multithreaded,
Metal, mobile or Wayland behavior. A general upstream remedy may need different
ownership APIs and wider platform coverage. Warnings or deprecations alone do
not repair a safe API that can still execute undefined behavior.

## Remaining decisions and reporting discipline

Font-atlas PRs, scroll policy and additional material capacity were evaluated
against actual Zubspace use rather than accumulated indiscriminately. Camera
flush cost, texture metadata growth and arena/GPU residency need separate
measurements; the cache fix does not establish their resolution. See the
[ledger](patch-ledger.md) and [issue decisions](upstream-issues.md) for sources,
reviewed heads, stale/already-fixed reports and concrete deferral reasons.

[RUSTSEC-2025-0035](https://rustsec.org/advisories/RUSTSEC-2025-0035.html)
has no published patched version in the reviewed advisory. Local fixes and
bounded interpreter/native/browser evidence do not change official advisory
metadata. The implicated-issue inventory, remaining unsafe boundaries and
accumulated review/integration status belong in [security.md](security.md).
Do not infer advisory closure from one PR or from removing APIs Zubspace does
not use. No clean `cargo-audit` result or universal-platform safety is claimed.
The distinct ttf-parser maintenance advisory and evaluated dependency deferrals
remain recorded in [dependency-audit.md](dependency-audit.md).
