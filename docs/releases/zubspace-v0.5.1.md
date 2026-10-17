# Zubspace Macroquad 0.5.1

Prepared GitHub-only release of package `macroquad` 0.5.1, tag
`zubspace-v0.5.1`, source commit `@COMMIT@`. This source template does not publish
or create a tag. Publication requires final local acceptance, green public CI
on the exact reviewed commit and a separate release decision. The immutable
0.5.0 release, notes, assets and historical game acceptance remain unchanged.
Upstream Macroquad stays 0.4.16; vendored Miniquad stays 0.4.11. Rust 1.88+ is the
compiler requirement, separate from the runtime extension versions below.
Original licenses and the complete reconstructible vendor patch are retained.

## Upgrade Rust and JavaScript together

Pin the full commit from the generated `release-manifest.json` and use its
matching `mq_js_bundle.js`; Cargo does not install JavaScript:

```toml
[target.'cfg(not(target_arch = "wasm32"))'.dependencies]
macroquad = { git = "https://github.com/pkieltyka/macroquad", rev = "@COMMIT@", default-features = false }

[target.'cfg(target_arch = "wasm32")'.dependencies]
macroquad = { git = "https://github.com/pkieltyka/macroquad", rev = "@COMMIT@", default-features = false, features = ["audio"] }
```

Verify asset checksums, copy the paired runtime locally before `load("app.wasm")`,
and upgrade/roll back the Rust pin, resolved lockfile and JS together. The runtime
SHA-256 is `ceb016d96bec8b2da6172f13902c71bc2ca8065fa250964a123706a5f4cb15fc`.
Source provenance keeps local package sources null; the generated manifest binds
concrete Git sources and the exact commit without a self-referential source SHA.
The source/runtime archives retain the generator, upstream base and licenses.

Run `node tools/check_browser_runtime.mjs --wasm path/to/app.wasm` against the
resolved lockfile before startup (`--no-audio` only if actual audio imports are
absent). The offline checker validates the exact `env.init_webgl(i32,i32)->void`
initializer signature, implementations and ABI exports. The WebGL selection
extension is now v2; Miniquad ABI2 and plugin/audio ABI1 remain unchanged. Current
Rust paired with a v1 runtime fails explicitly before context/GPU creation.
The reverse direction differs: already compiled 0.5.0 WASM checks for extension 1
before calling its initializer, so pairing it with this v2 runtime traps in that
old guard; this cannot acquire a new diagnostic from the new initializer. The
offline checker rejects that stale initializer with an actionable error. Browser
"old-v1" negatives use current WASM with a v1 runtime guard override and test the
first direction; they do not establish the old-binary diagnostic in reverse.

## Rendering and capacity migration

`sample_count <= 1` now explicitly requests drawing-buffer antialiasing off,
including the default 1; values >1 request it on. This corrects the previously
ignored setting and can change edge appearance. The browser chooses actual
technique/sample count and may decline AA. Auto still tries WebGL2 then WebGL1
only on context-creation failure; explicit versions stay strict. Alpha,
premultiplication and other context defaults are unchanged. No FPS gain is
inferred from this request change.

Public mesh submissions remain atomic. Oversized calls or local indices outside
the vertex slice panic with a semantic message in debug; release rejects the
whole call and warns once per renderer lifetime with submitted counts, effective
limits and remediation. No topology is clipped. Empty indices are a no-op.
Effective vertex ceilings are 65,535 on actual WebGL2 (index 65,535 is fixed
primitive restart), otherwise 65,536; index count remains independently bounded.
Submit smaller valid meshes or increase configured capacities within those limits.

Oversized built-in 2D triangles split privately only with the default pipeline,
preserving every Vertex attribute/order. Arc chunks are generated before u16
source indices wrap. Every custom material, including custom triangle pipelines
using `gl_VertexID` or `_ScreenTexture`, keeps the original direct submission
when it fits and atomic rejection otherwise. Splitting default built-ins can
produce multiple draws/capture snapshots. Public meshes are not automatically
split. Nonempty built-in UI requires at least 3 vertices/3 indices and splits its
private triangle lists above that minimum. End-frame UI uses screen projection
while retaining the game's camera.

`gl_set_drawcall_buffer_capacity` validates first, flushes pending geometry using
the current camera/pass/viewport/model/scissor, then resizes. Invalid requests
neither flush nor change capacities. This replaces the earlier queued-work panic;
callers should account for the flush boundary.

## Input behavior

Mouse presses originating on the canvas receive one owned release through the
owning window, including outside the canvas. Release coordinates remain relative
to the canvas (and can lie outside it); pointer-lock relative/raw motion is retained.
Blur/hidden cleanup precedes the deduplicated focus notification. Unrelated and
duplicate releases are ignored.

Physical and simulated Left holds combine; an ending source cannot erase the
other hold. Cancelled contacts terminate bookkeeping without adopting a new
primary finger. Legacy first genuine Ended behavior is retained even when other
contacts survive. Disabling simulation clears its hold while preserving a physical hold; blur
terminates both sources. Touch wake works with simulation on or off.

If a terminal touch event is lost, a later authoritative `TouchEvent.touches`
list cancels only missing IDs whose Started this canvas owned, at their last
known positions. Explicit changes are delivered first and new Started owners
are registered before reconciliation; unrelated listed contacts are never
adopted. No missing-event detection is possible without a later hint. Tests label
synthetic touch sequences explicitly; they are not physical-touch qualification.
A context-menu workaround is deferred: trusted headed Chrome/Firefox right-click/
Escape sequences released Right in the tested matrix; Firefox Shift-right omitted
contextmenu. DOM events do not prove a visible native popup, and no global menu
suppression or speculative Left/Middle release is introduced.

## Evidence and limits

Boxed scoped ownership removes large Context/UI copies from measured optimized
WASM draw/UI paths while preserving scoped borrowing, unwinding and destruction.
An external generated-scene comparison of `e753c60` (inline) with `cb88c6e` (boxed)
found mean CPU scene-API issuance reductions of 17.91–41.47% across the eight
browser/GL/variant aggregates. Four fixed A-before-B rounds used 216 measured
samples per browser/GL/variant/revision; all 32 round aggregates improved, not
every individual sample. Recorded PNG pixels matched and GL checks were clean.
Only executable ownership source differed (plan prose also differed). This used
Linux Chrome SwiftShader/Firefox software GL and a historical runtime, excluding
deferred end-frame submission and GPU work. It establishes neither whole-frame
FPS, physical-GPU benefit nor application performance. See the qualified
[evidence/checklist](../../plans/0.5.1-browser-efficiency.md).

Final acceptance must use `make release-check` and `make test-ci` on the reviewed
clean candidate, plus exact-commit public supported/MSRV/Miri CI before publication.
The complete local gate includes strict actual audio suspension/trusted gesture/
resume and source cleanup, input/UI/lifecycle/codec/resource/pixel checks and
diagnostic scene trials. CI deliberately runs bounded real graphics smoke and
does not require audio hardware or full timing/lifecycle acceptance. Ten repeated
Firefox attribute matrices are a separate local stability gate; zero observed failures
cannot prove zero flakiness. Final receipts remain separate from these prepared
notes until they have actually run.

Cross-OS/browser-device/physical-GPU/hardware-audio qualification, context
restoration and GPU timing remain unavailable or pending. No game files/assets,
adoption, deployment or renewed game acceptance are part of this release work.
The ttf-parser advisory remains; this is not an official RustSec patched release
or a clean-audit claim. See [fork migration](../fork.md),
[runtime provenance](../browser-runtime.md), [security limits](../security.md) and
[patch attribution](../patch-ledger.md).
