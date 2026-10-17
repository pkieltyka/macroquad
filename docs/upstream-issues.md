# Upstream issue decisions for Zubspace

Reviewed 2026-10-16 using `gh issue view`/`gh pr view` and current source in
this fork (Macroquad 0.4.16, Miniquad 0.4.11). All issues below remain open
upstream; an open issue is not proof that the exact code here still has its
original defect. This is a finite Zubspace shortlist, not a claim to resolve
all upstream reports.

## Confirmed texture-binding cache defect

[Macroquad #1006](https://github.com/not-fl3/macroquad/issues/1006) reports black
rectangles in place of font glyphs. The
[reproduction and analysis](https://github.com/not-fl3/macroquad/issues/1006#issuecomment-6199729461)
and [Miniquad #691](https://github.com/not-fl3/miniquad/pull/691) identify a
backend lifetime error: `GlContext::delete_texture` deletes the GL texture but
leaves its name in the current and saved binding cache. A later live-texture
update can restore the deleted name; GL-name reuse can also skip a necessary
bind when allocating a replacement.

The exact published Miniquad 0.4.11 source was reproduced with
`tests/texture_cache`: bind/draw A, delete A, update only live B, draw/read B,
delete B, then create/draw/delete a replacement, repeated 32 times. The
application never reuses deleted handles. Both Chrome and Firefox, in both
WebGL1 and WebGL2, reported **126 stale deleted-ID bindings per run**, while
all 128 pixel checks and GL-error checks still passed. Correct pixels alone
would therefore miss this defect. This fixture confirms the stale-cache path;
it does not claim to reproduce every black-glyph symptom or a fatal mobile
crash. Native GL also reproduces the error directly: the original source restores
texture name `1` after deletion, failing the expected zero-binding assertion.
The patched native GL run and all four browser/WebGL combinations pass
128 pixel checks per case with GL error zero and no stale binding attempts;
live uploads restore binding zero after the previously bound texture is deleted.
Browser checks use Chrome SwiftShader and Firefox under Xvfb, rather than a
claim of coverage across physical GPU vendors.

The narrow fix in `vendor/miniquad` adapts source commit
`fbc59729b24dd6c3c4052a729431fa54ffe42b0d` by ricande
`<ricande@gmail.com>` from Miniquad #691 (still open/unmerged at review).
After deleting a GL texture, it forgets every matching current/saved binding
without GL calls or changes to unrelated bindings. CPU tests cover both cache
layers and preservation of unrelated textures/buffers. The published crate,
licenses, exact source hash, reproducible delta, and regeneration check are
documented in [vendor/README.md](../vendor/README.md).

This backend fix matters beyond font rendering: Zubspace replaces LVZ textures
in `src/lvz/render.rs`, rebuilds the LVZ render cache in `src/game/events.rs`,
and clears arena caches during transitions. Those lifetimes make deleted/live
texture interleavings relevant even though gameplay text uses bitmap sprites.
Actual arena-transition acceptance remains part of the pinned-fork integration
checkpoint. Texture-cache invalidation does not reclaim Miniquad's append-only
texture metadata or establish RustSec remediation.

## Measure before changing behavior

| Issue | Source evidence and decision |
| --- | --- |
| [#950: expensive camera reset](https://github.com/not-fl3/macroquad/issues/950) | Worth profiling later. `set_default_camera` flushes queued render work, so elapsed time includes earlier draws. Zubspace's `src/game/render/ships.rs::draw_ship_label_text`/`draw_ship_label` switch to the screen camera and back for each label. Measure release CPU time, flush/draw counts, and representative ship counts before considering a screen-space label pass. No camera change in this batch. |
| [#934: texture-size lookup cost](https://github.com/not-fl3/macroquad/issues/934) | Measure first. `draw_texture_ex` uses one `texture.size()` for the original texture, plus a lookup if atlas batching substitutes another texture. Miniquad reads stored texture metadata, not a GPU size query. Zubspace's bitmap glyph loop in `src/font.rs` makes CPU cost plausible, but no measured bottleneck justifies duplicating mutable texture-size state. |
| [#959: GPU texture memory](https://github.com/not-fl3/macroquad/issues/959), [#635: texture/atlas lifetime](https://github.com/not-fl3/macroquad/issues/635) | Keep as separate arena-lifecycle measurements. #959's discussion reports GPU retention even when Macroquad's live-texture count reaches zero; driver retention and ownership leaks require different evidence. Miniquad's texture metadata vector also appends without reclaiming slots after deletion. Track live resources, metadata growth, GPU residency, and repeated arena/LVZ replacement separately; the binding-cache patch does not claim to fix these. |

## Existing coverage, fixes, and explicit deferrals

| Issue or PR | Decision for the exact current code |
| --- | --- |
| [#1055: strict-mode registrar startup](https://github.com/not-fl3/macroquad/issues/1055) | Confirmed separately and assigned to the browser-runtime batch. Preserve its scoped registrar declaration fix, runtime source/hash, and strict-startup regression. The vendor's original JavaScript is retained unchanged. |
| [#1041: WASM linking](https://github.com/not-fl3/macroquad/issues/1041) | Keep Zubspace's `--allow-undefined` setting for intentional JavaScript imports and validate every required import against real implementations. Do not add the suggested `--import-memory` workaround: the runtime expects the module's exported memory. The finite fixture rejects fallback import stubs. |
| [#1020: unnecessary non-MSAA resolves](https://github.com/not-fl3/macroquad/issues/1020) | No new patch. `render_target_ex` only creates resolve attachments when `sample_count > 1`; that distinction is present in base `8d602d3`. Preserve it in runtime/WebGL acceptance. |
| [#876: render-target slowness](https://github.com/not-fl3/macroquad/issues/876), [#275: Linux/Nvidia CPU use](https://github.com/not-fl3/macroquad/issues/275) | The reported unconditional `glFinish`/`glFlush` path is already gated by `telemetry::is_enabled()` here. No blanket claim about every driver/vsync report; profile an actual supported-target reproduction before further synchronization changes. |
| [#655: screenshot texture leak](https://github.com/not-fl3/macroquad/issues/655) | The original unmanaged allocation is absent: `get_screen_data` stores its temporary texture in managed resource storage and drops it after readback. This corresponds to the issue's reference to [#666](https://github.com/not-fl3/macroquad/pull/666); ownership/destruction acceptance remains separate. |
| [#627: missing font-atlas destructor](https://github.com/not-fl3/macroquad/issues/627) | `Atlas` already implements `Drop` and deletes its texture. No duplicate destructor patch; destruction borrowing is part of the ownership work. |
| [#385: reversed characters](https://github.com/not-fl3/macroquad/issues/385) | Current input queues use `push_back`/`pop_front`, preserving FIFO order. No patch. |
| [#277: mouse release outside canvas](https://github.com/not-fl3/macroquad/issues/277) | Not solved by this batch. The canvas-only event pattern merits attention if mouse gameplay is added; no current Zubspace gameplay mouse usage establishes a need to import a new event policy. Keep existing keyboard/focus recovery acceptance. |
| [#787: canvas-only resize](https://github.com/not-fl3/macroquad/issues/787) | Defer a `ResizeObserver` change: Zubspace uses a viewport-filling canvas. Validate window resize/DPI/focus in browser acceptance; embedded-layout behavior needs its own requirement/reproduction. |
| [#533: old Safari OGG decoding](https://github.com/not-fl3/macroquad/issues/533) | The report concerns Safari 16.1 and an OGG asset; it does not justify a codec workaround for Zubspace's `.wa2` sound path. Retain real loading/playback and decode-failure checks; do not treat old browser support comments as present compatibility evidence. |
| [#749: refresh-dependent simulation speed](https://github.com/not-fl3/macroquad/issues/749) | Keep Zubspace's existing fixed-step/high-refresh acceptance. No engine sleep/FPS-limiter patch, especially on the browser event loop. |
| [#1075](https://github.com/not-fl3/macroquad/pull/1075), [#1076](https://github.com/not-fl3/macroquad/pull/1076), [#1078](https://github.com/not-fl3/macroquad/pull/1078): font-atlas changes | Defer. The WASM branch of `src/assets/load/progress.rs` sends progress to the HTML loader and yields; only its native branch calls `draw_text`. Browser gameplay text uses bitmap sprites. Thus neither browser loading text nor gameplay demonstrates an atlas optimization/filtering benefit. Revisit only after a measured need or an applicable rendering reproduction. |

Focus recovery, audio-unlock/loading/playback failures, arena transitions,
WASM identity, and packaged-runtime checks stay in the existing acceptance
matrix rather than becoming independent speculative patches here.
