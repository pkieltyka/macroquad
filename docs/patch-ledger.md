# Zubspace fork patch ledger

The fork starts at upstream `8d602d3f8ebb8a3a582ee15d92983bbb81ee3ef6`.
Source commits below identify the reviewed community patches; local adaptations
are called out separately. Use `git log --follow -- <path>` to find the local
integration commit. Recheck the ledger when updating upstream and drop patches
that have landed there.

## Lazy UI material and uniform safety

Reviewed with `gh pr view` and `gh pr diff` on 2026-10-16. Both PRs remain open,
unmerged, with no submitted reviews. Their combined changes touch independent
paths. This entry does not establish remediation of RUSTSEC-2025-0035.

| Source | Attribution | Decision and local changes |
| --- | --- | --- |
| [#1074](https://github.com/not-fl3/macroquad/pull/1074), `eb2bf9cc445a288fad68c22d1693002263a28b16` | Benoît Rouleau `<benoit.rouleau@icloud.com>` | Apply the patch: render the UI command list before creating its material or updating the atlas texture. Preserve texture/default-material reset even on empty frames. Add regression coverage. Zubspace has no `root_ui` calls. |
| [#956](https://github.com/not-fl3/macroquad/pull/956), `98d861825e65a6cc23922f00dc28031786304c40` | Maja Kądziołka `<maya@compilercrim.es>` | Adapt the unsafe representation contract and implement it on arrays/slices themselves. Use `size_of_val` for their lengths and document initialized storage, lifetime, and interior-mutability constraints. |

Local hardening beyond #956 removes the unconstrained `transmute_copy` in
scalar uniform uploads. `Material::set_uniform` and `QuadGl::set_uniform` now
require `ToBytes`. The trait is available as `macroquad::material::ToBytes` so
custom representations need an explicit unsafe implementation. Arrays, `i32`,
`f32`, and scalar-math glam vectors/matrices are supported. Tuples and arbitrary
application structs no longer satisfy scalar uploads; migrate tuples to arrays
or glam vectors. The affected examples use arrays. Zubspace has no custom
uniform calls.

Uniform metadata is validated before Miniquad receives it: counts must fit a
positive `i32`; multiplication, offset addition, and Rust slice lengths are
checked, including the backend's signed byte-size limit. Uniform backing
storage uses initialized `u32` words to guarantee four-byte alignment for the
backend's float/int pointer uploads under custom allocators. Scalar and array
writes verify byte lengths and preserve adjacent uniforms. This guards the metadata assumptions in Miniquad 0.4.11; it does not
claim that Miniquad's unrestricted public APIs are themselves sound.

Regression entry points are `tobytes::tests`, `quad_gl::uniform_tests`, the two
`Material::set_uniform` compile-fail doctests, the explicitly run graphics test
`ui_material_is_lazy_and_reused_after_empty_frames`,
`examples/ui_material_regression.rs`, and `tests/ui_material_browser.py`. The
graphics cases cover empty/first/reused UI material, raw UI texture followed by the atlas, valid scalar/array uploads,
and default-material reset for empty and populated UI frames.

Browser startup/frame-time benefit in Zubspace remains unmeasured. Upstream's
phone timings are not measurements of this fork. Native and browser regression
results are recorded in the working punchlist; release Zubspace acceptance
belongs to the pinned-fork integration checkpoint.

## Reviewed deferrals and rejections

All candidates below were open and unmerged on 2026-10-16. Their diffs,
discussions, and head revisions were refreshed with `gh`; do not assume those
heads remain current later.

| PR and reviewed head | Decision and concrete reason |
| --- | --- |
| [#973](https://github.com/not-fl3/macroquad/pull/973), `0e97981f8418c08f04f316e2b7e0c7dbe8c7ef42` | Defer import to the ownership batch. Isolating audio is useful, but its closures can borrow-panic on reentry; initialization, final-handle drop, cross-thread ownership, and asynchronous loads must fit the whole ownership design. The author confirms the wider context work was unfinished in the May 2026 discussion. |
| [#1075](https://github.com/not-fl3/macroquad/pull/1075), `2d29112684ce7984e47b0c6096ed6d762d89ef96` | Defer. Partial changed-row uploads and pre-caching a string may help loading-screen text, but gameplay uses `src/font.rs` bitmap rendering. No measured Zubspace glyph-upload bottleneck yet. Revisit alongside atlas safety after a release-browser profile. |
| [#1076](https://github.com/not-fl3/macroquad/pull/1076), `856a67bdc86468c39e689416114400ab9d89c756` | Defer. Row copying/growth optimization changes atlas and `Image::sub_image` bounds; first finish checked image invariants. Gameplay bitmap text does not establish a need for font-atlas growth optimization. |
| [#1078](https://github.com/not-fl3/macroquad/pull/1078), `c09450ff3f94162d8ca6954b40f42d85aad7f0eb` | Defer with the atlas group. Edge padding changes glyph packing, growth, and filtering. No Zubspace artifact reproduction at supported DPI scales; validate loading-screen text and the combined atlas patches before import. |
| [#1067](https://github.com/not-fl3/macroquad/pull/1067), `ac0742908ad68b4a26e49b7ea215f178591e4ae6` | Defer. No Zubspace wheel-input usage. The paired [Miniquad #661](https://github.com/not-fl3/miniquad/pull/661), head `5df44b12602401bcb0f166e6c8f44a6ec0fd85e6`, also remains open and changes scrolling units/directions; take a tested pair when needed. |
| [#1072](https://github.com/not-fl3/macroquad/pull/1072), `816aece995a9b3f44943c32fd1c0fc0236eff92b` | Reject for this fork's current scope. Raising the pipeline cap from 32 to 128 serves games with many custom materials; Zubspace has none. Lazy UI already avoids one unused slot. |
| [#1077](https://github.com/not-fl3/macroquad/pull/1077), `175ec99b196c565385f9ffa3c6da41a042ac123f` | Defer. Zubspace does not load custom TTF fonts; removing their preload does not address its gameplay bitmap renderer. Default UI fonts already begin without that preload. |
| [#1061](https://github.com/not-fl3/macroquad/pull/1061), `3d1a775d4c2458b9547ed7f52414a66621228b2b` | Reject for current scope. Changes the default 3D camera orientation; Zubspace uses 2D rendering. Other unrelated 3D additions remain outside this fork's purpose. |
| [#1018](https://github.com/not-fl3/macroquad/pull/1018), `c746edd931a0d8cd9cfa094b0581977fc325636d` | Defer. Addresses Linux/X11 implicit pointer grabs, not browser focus/input. No current native Zubspace reproduction; retain native correctness checks without importing an unrelated input policy change. |
| [#878](https://github.com/not-fl3/macroquad/pull/878), `1019ade9e572e3f1c748736252f9954c845452e9` | Reject as remediation. Adds accessors/deprecations but leaves public image fields mutable; current code also differs from its base. Adapt useful ideas only with complete invariant/overflow/transfer enforcement. |
| [#867](https://github.com/not-fl3/macroquad/pull/867), `919f3ad1e4ac99a2c886bdcc2706a42a4f32daec` | Reject as a soundness fix. Only adds deprecation warnings to safe tween functions that still execute undefined behavior. The reviewer explicitly suggests fixing their API. |
| [#849](https://github.com/not-fl3/macroquad/pull/849), `2faab3bfa9de22459a9e04ca1ca43cd594897a15` | Reject. The author reports that the alpha-blend change makes textures disappear in 3D scenes; no corrective commit or validating discussion was present. |

Rollback for this initial batch is the upstream base above. Once Zubspace pins
the fork, record its prior dependency revision and matching browser artifact in
the integration entry before changing that pair.
