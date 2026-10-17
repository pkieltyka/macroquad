# Zubspace fork patch ledger

The fork starts at upstream `8d602d3f8ebb8a3a582ee15d92983bbb81ee3ef6`.
Source commits below identify the reviewed community patches; local adaptations
are called out separately. Use `git log --follow -- <path>` to find the local
integration commit. Recheck the ledger when updating upstream and drop patches
that have landed there.

## Owned browser termination for prepared 0.5.1

[Miniquad #602](https://github.com/not-fl3/miniquad/pull/602), by TheRedDeveloper,
head `876e2fb92fa5339071dbaf5d3ef40df4f8901bb7`, was open/unmerged when reviewed
on 2026-10-17. The fork adapts its owning-window mouseup listener idea; it does not
copy the unconditional global-release policy. Only canvas-originated presses are
owned, outside coordinates stay canvas-relative, duplicate/unrelated terminals
are ignored, and blur/hidden releases precede focus notification. Pointer-lock
coordinates and raw motion remain intact.

Local additions preserve physical/simulated Left aggregation, legacy touch Ended
semantics, cancellation and wake with simulation disabled. If a terminal is lost,
a later authoritative touches list reconciles only canvas-owned IDs, using their
last positions and synthetic Cancelled callbacks; unrelated contacts are never
adopted. Recovery needs a later event. Real browser mouse actions and explicitly
synthetic missing-touch/wake regressions are separate evidence; physical-device
and native-menu visibility qualification remain absent. No contextmenu production
handler or menu suppression is added. The source, generated bundle, generator
anchors, provenance and complete reconstructible backend patch move together.
See [migration](releases/zubspace-v0.5.1.md) and [input contract](../plans/0.5.1-browser-efficiency.md#input-batch-complete-owned-browser-input-termination).

## Configured WebGL antialias request for prepared 0.5.1

The previous browser initializer ignored `Conf.sample_count`, implicitly requesting
browser-default antialiasing even for the default1. The paired v2 contract passes
an explicit0/1 antialias argument: values <=1 request false, >1 request true on
explicit1/2, Auto2 and fallback1 alike. Browsers choose actual sample count and
may not provide requested AA; this is no exact-count or performance guarantee.
Alpha and all other context defaults remain unchanged; the Android-only alpha
field is not silently repurposed. Rust checks extension v2 before setup and logs
a clear old-runtime diagnostic; runtime/checker/provenance/vendor patch move
together. This local correction preserves previous input/audio/ownership fixes.

## Local automatic WebGL selection for 0.5.0

The local `WebGLVersion::Auto` variant is now the default. The WASM boundary
introduced mode0 and checked the additive `webgl_selection_version` extension v1
before context setup. The patched vendor initializer tries WebGL2 then WebGL1
only on context-creation failure; explicit1/2 stay strict, and unavailable or
unsupported requests fail before extension/GPU calls. Rust capabilities continue
to use the actual context's owned version string. This is a local fork change,
not a community PR adoption, Miniquad version upgrade or performance claim.

The maintained aggregate deterministically takes its initializer from the
vendored `js/gl.js`; source/provenance/runtime assets bind that input. Legacy
Miniquad ABI2/audio ABI1 and previous audio/cache/ownership fixes are retained.
The historical c4cc runtime is a negative-test input, not the current release
bundle or evidence for new Auto game acceptance. Genuine Chrome/Firefox fixtures
cover auto2, forced fallback1, strict1/2, missing contexts and old-runtime failure,
checking pixels, actual capabilities, GL errors and resource cleanup. The full
vendor delta remains reconstructible from the published 0.4.11 archive.

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

No causal startup/frame-time benefit in Zubspace is established. Upstream's
phone timings are not measurements of this fork. Matched release startup and
artifact-size observations are summarized in [the dependency audit](dependency-audit.md);
the final game acceptance remains separate from those measurements.

## Deleted-texture binding cache

Vendor the published Miniquad 0.4.11 source and adapt
[Miniquad #691](https://github.com/not-fl3/miniquad/pull/691), commit
`fbc59729b24dd6c3c4052a729431fa54ffe42b0d` by ricande
`<ricande@gmail.com>`. The PR was open/unmerged on 2026-10-16. Deleting a GL
texture now invalidates matching current/saved bindings without rebinding
unrelated textures. Local additions are cache-state regressions and a finite
native/WebGL1/WebGL2 deletion/live-update/churn fixture.

The direct path dependency retains version `=0.4.11` for git consumers. The
published artifact/hash, original licenses, complete narrow patch, and
reconstruction check are in [vendor/README.md](../vendor/README.md).
[Upstream issue decisions](upstream-issues.md) connect this defect to #1006 and
Zubspace arena/LVZ texture lifetimes and record separate camera/metadata/memory
measurement work. No camera or atlas optimization is included.

## X11 property and render-pass boundaries

Local commit `f347a865a939fcdb7b4ba8cc3a5e86c13546fb4b` checks Xlib reply
format/count arithmetic and null/short clipboard properties before copying.
It also makes GL render-pass deletion release FBOs without deleting independently
owned attachments, handles null GL version strings, detects WebGL1 version
prefixes correctly and selects supported WebGL2 depth formats. These are local
boundary fixes rather than imports of an unrelated backend revision.
[Vendor provenance](../vendor/README.md) records the reconstructible patch;
direct native/WebGL1/WebGL2 fixtures cover attachment reuse and capabilities.

Accumulated review additionally found an unconditional LP64 initialization
overread in `XdndAware`: format32 passed an inferred i32 item to Xlib's native
long reader. Local commit `4f9022cf796ae16f5987273ee938e45c8f51e789` uses one
`c_long`. The actual production writer passes native-long reads under Miri on
x86_64/i686 and real Xvfb type/format/count/version readback. Restoring the old
allocation makes its regression fail under x86_64 Miri. Full external drag/drop
and multi-client INCR timing are not claimed by these focused tests.

## Independent coroutine and audio lifetimes

Adapt the audio-isolation direction in [#973](https://github.com/not-fl3/macroquad/pull/973),
head `0e97981f8418c08f04f316e2b7e0c7dbe8c7ef42` by kampffrosch94
`<kampffrosch94@github.com>`, rather than importing it as a complete soundness fix.
The reviewed PR leaves broader ownership work unfinished. This fork gives audio
and the scheduler independent thread-local owners. Sounds retain their source
owner; final drop queues deletion without borrowing an already-borrowed backend.
Polling, cancellation, result retrieval and future/result destruction release
the scheduler borrow before running user code. Handles are thread-local;
explicit cancellation also releases unclaimed completed results.

Local additions cover non-reused task IDs, nested manual time and unwind,
self-cancellation, destructor reentry, TLS shutdown, pending sound cancellation
and decode failure. Native graphics tests use exact-selected subprocesses rather
than resetting Miniquad's process-global display. The paired WASM runtime must
provide `audio_source_state` extension v1 (`0` pending, `1` ready, `-1` failed,
`-2` removed), preserve plugin ABI 1, and handle callback and Promise completion
without resurrecting deleted sources. This is an intentional runtime pairing
requirement, recorded by Zubspace's final import/provenance gate.

Regression entry points are `experimental::coroutines::tests`, `audio::tests`,
the grouped integration tests, and `examples/audio_lifecycle_regression.rs` with
`tests/audio_lifecycle_regression.html`. CPU ownership helpers run under Miri
on x86_64 and i686. The finite audio fixture checks genuine Chrome/Firefox
WebAudio imports, failed decode, pending cancellation, clone/drop and unclaimed
result cleanup. Graphics context, UI, telemetry and GPU resource ownership were
completed in the separate scoped-ownership batch below; this audio batch alone
does not establish advisory-wide remediation.

## Image dependency modernization

Upgrade image 0.24.9 to 0.25.10 with default features disabled and PNG/TGA
retained. Declare the fork's Rust minimum as 1.88 and migrate the removed
`ImageOutputFormat` test API. The public `ImageFormat` reexport and image error
payload now use image 0.25 types; consumers using image directly must align
their types. Codec/export behavior is covered by `tests/image_codecs.rs` and
the finite native/WASM `image_codec_regression` example/HTML fixture. Existing
image bounds and graphics transfer regressions remain applicable.

This is a local dependency migration, not an imported community patch.
[Dependency audit](dependency-audit.md) records primary release sources,
compatibility/MSRV consequences, target boundaries and remaining decisions.
Zubspace's combined graph now has one image/PNG family; actual release sizes
increased and the startup samples establish no benefit, as recorded in the
dependency audit. Preserve the vendored Miniquad patch; this change does not
establish closure of Macroquad or font-parser advisories.

## Scoped graphics and supported-surface ownership

Local commit `ea02ddd91439fb1c5024f0dc062c6a04b1f3d271` remediates
[#333](https://github.com/not-fl3/macroquad/issues/333) for the supported surface and
replaces unrestricted mutable globals with scoped TLS context/backend/UI owners.
GPU guards retain their originating owner and enqueue deletion; draw, camera,
material and capture leases retain required resources until flush/reset. The
non-owning backend pass patch above allows attachment clones to outlive targets.
Camera/input/uniform callbacks run outside context access, UI uses
`root_ui(|ui| ...)`, and window entry is atomic/single-use with explicit live
shutdown. The unused unsafe texture slot map/raw waker and telemetry parent
pointers are removed. GPU timing is explicitly unavailable rather than zero.

These are local ownership adaptations, not a wholesale import of #973 or a
standalone advisory closure claim. [Security inventory](security.md) records
remaining foreign contracts, intentional API breaks and actual regression entry
points. Independent and accumulated library review and all four
[fork CI jobs](https://github.com/pkieltyka/macroquad/actions/runs/38994560847)
passed for that revision. Matching Zubspace acceptance remains a separate gate.

## Intermediate application acceptance checkpoints

The Zubspace integration history additionally contains reviewed harness-only
changes: `aeaaec5a98c571af0bf8295d3d3a6573b8ef984c` retains mixed-smoke timeout
state and drains failure logs; `decf5ceb4a558f6aa37cd94869e2e167b3e30bbd`
corrects the mixed fixture's stale HUD-line expectation to the ordinary compact
HUD contract; `e521024672235c5401c5da6ddf0702c15df96433` applies that same
contract to standalone browser smoke. These commits do not change application
WASM, JavaScript or native server bytes.

The retained `smoke-mixed-rtc` Chrome/Firefox, Chaos-arena subgate passed at
that fixture checkpoint, with genuine RTC and independently verified artifact
provenance. Its result is
`reviews/mixed-smoke-e521024/mixed-smoke/result.json` in the retained Zubspace
review evidence. This was one focused subgate at that checkpoint; the final outcome map below
supersedes its then-pending canonical14/release notes without relabeling it.

The independently reviewed respawn repair is now integrated as application
source `64c8f58ca4c58c5b30baf03f55fd4510f2f8c174`. Its actual previously failing
Chrome/Extreme-arena mild-network cell passed strict life-2 admission at energy
2800, followed by held-key suppression, fresh input, focus/network/security and
cleanup checks. All four canonical12 mild keyboard cells passed on those app
bits. The inherited failure and matching current-crates control remain retained;
the earlier pre-repair runs do not become post-repair evidence.

Canonical11's software run remains RED/unclassified (seven peers passed; peer3
had a 12-tick gap against the 10-tick floor). Its prescribed native-GPU renewal
passed all eight peers, maximum gap nine ticks, with actual RX9060XT hardware
WebGL/compositing and independent provenance checks. These are separate retained
verdicts, not a rewritten original aggregate result.

The reviewed fixture/notice head for the app64 checkpoint was
`3875e57e2c36d67964988168c928271906834e67`, including observed-explosion proof
correction `fa3e38b0a81be371d0a4115b51482615818f3608` and the reviewed ASSETS
notice cherry-pick. All nine compiled/application/lock/metadata hashes still
match app source `64c8f58`; host notice packaging copied only ASSETS.md without
rebuilding/restamping the app. Its 20-file HTTP closure and eight local notice
links passed. `notice-package-3875e57/manifest.json` and `artifact-manifest.json`
under retained `reviews/final-acceptance-64c8f58/` distinguish those identities.

The local image pair accepted for app64 is identified below. It is historical
after the later Ship repair and must not stand in for newly built images:

| Image | SHA256 image ID |
| --- | --- |
| Corrected client, reviewed notice context `cb0db65` | `e211ef66646f2bc2050cceef19cda9155704adaa269d07e4e24b31a858407a94` |
| Retained server, reviewed context `712a5d1` | `f45d6f7d38463acd90c8b0e0fddfcf4919571ecf505c2452c53ff799ea71716f` |

The client supersedes the earlier stale-notice image; the server does not
consume ASSETS.md. The exact pair passed actual isolated Compose RTC acceptance
and independent verification using unique project `zubspace-compose-rtc-2772334`,
client/server ports 17000/19001 and UDP 19202–19401. Owned containers and isolated
listeners were absent after cleanup; original services were preserved. This used
an explicit temporary prebuilt-image configuration and documented recipe
composition, not the unchanged default Make invocation. Exact context/image
records are in `final-images-reviewed-712a5d1/manifest.json`; Compose results and
cleanup are in `final-isolated-compose-images-712a5d1/manifest.json` and
`retained-compose-rtc/result.json` beneath that retained evidence root.

Historical Firefox/Chaos bomb-child failure remains RED: it had no observed
explosion-commit fact. Source-only failure recaptures and offline lineage checks
are diagnostic records, not newly observed browser child-proof acceptance.
All four mild bot cells now pass across Chrome/Firefox and Chaos/Extreme with
mild network profile seed20260827, using the documented direct-case execution
maps rather than a claim that the unchanged aggregate passed. Chrome used fixture
`712a5d1`; Firefox used `3875e57`; all used app source `64c8f58`. Every passing
app64 retry was primary-only: the reviewed `fa3e38b0` observed-child branch was
then unexercised. Historical missing-commit Firefox failure is not reconstructed
or upgraded by those runs. The later c975 Chrome observation below supplies new
actual child proof without changing those historical results.

Snapshot-loss and Chrome/Firefox input recovery passed their independent full
provenance audits. Immediate impairment passed its channel contract and both
12%-server-loss natural-prize arms with independent verification; the retained
mixed smoke and actual 200-peer capacity gate also passed those audits. The
capacity recipe rebuilt its optimized server at fixture `3875e57` from unchanged
production Rust, so its actual release-server identity is retained separately
from the nine frozen app/debug artifacts. Standalone ordinary-HUD smoke passed
with stdout-only evidence, without an artifactFiles/full-recapture claim.

The records and mild Firefox summaries are in
`continuation-3875e57/execution-map.json`; Chrome mild cells remain in
`continuation-712a5d1/execution-map.json`. That application completed 17 of its
32 continuation invocations, including both aged-server arms and focused
semantic/baseline/warp/wormhole/Portal/prize/negative-energy lanes. Those passes
retain their original source identities and evidence limits; stdout-only recipes
are not full artifact/QA captures.

Focused objective-authority then found another genuine inherited product gap:
Extreme ship2/Javelin submitted authored energy1200 while strict maxed Ship
admission required2000. The server rejected the baseline and clients rejoined.
This was not a fixture precondition, and no authored-policy workaround was
accepted. The original failure remains under the app64 continuation evidence.

The reviewed production repair is now clean committed delivery
`fb3fbd3928a20416fdde1234a8a327df9e483381`. It independently negotiates Ship
handoff, discovers capability through observer metadata and freezes only an
explicit Ship transaction, avoiding an InitialJoin deadlock. It covers ordinary
Ship and spectator return while retaining exact scope/life/neutral admission and
application-receipt barriers. Three fresh reviews resolved the discovery/join
boundaries; the final review found no issues. Matching frozen source validation
passed 64 Node,1321 client,1413 server tests,24 actual cross-binary cells and
native-all-target/WASM checks. The CPU exchange does not round-trip an actual
server receipt into Game; browser receipt renewals remain required.

Current authoritative evidence is
`reviews/final-acceptance-c975ae2/manifest.json`. Compiled client source is
`c975ae2634573605725107ebfcffc0785d632f3a`; fixture-only delivery is now
`6167ac530c2d5dfa963ad12cb5ecb1e9190ad049`. Retained native server source is
`ec9090fdc50734b6edaef930b12d70a684fcaab1`. The c975 client repair followed
fixture diagnostics626 and the reviewed readiness pilot8ecd1b5. Its actual
native/debug-WASM/release-package/runtime-identity checks passed. Server debug/
release binaries, lockfile and paired runtime/metadata are byte-unchanged, with
separate identities in `artifact-manifest.json`; the server is not restamped.

The preceding fb3 native/debug, release/package and common checks passed.
Native release integration recorded1408 passing/11 ignored tests; the ec909
server renewal recorded1412 passing/11 ignored. Actual objective retries passed
both arenas with original player IDs, life2 maxed energy2000, no RTC rejoin and
zero security violations, with full artifact/QA audits. Baseline receipt, ship
energy, all eight ships' abilities, Chrome/Firefox Extreme spectator return,
clean URL and clock-stall renewals passed. The initial Firefox spectator stdout
PASS lost temporary artifacts; an approved identical persistent renewal passed
a full24-file/two-QA audit, without reconstructing the lost capture.

Active-projectile recovery first failed with53 nonlethal target hits and42 human
bullet deaths. Reviewed child-only fixture `20ef2e7` isolates the authored moving
bot's life2 while preserving motion, other hostile lives and strict assertions.
The actual retry passed its full13-file/one-QA audit:458 copies were dropped,
458 events delayed/released, none pending/expired, and the moving primary
projectile was lethal. This retry was primary-only. Ordinary special-weapon arms
passed Chrome/Firefox in both Chaos and Extreme arenas with full audits.

The former ec909-server/fb3-client Compose checkpoint remains archived under
`reviews/final-acceptance-ec9090f/final-server-image-ec9090f/manifest.json`.
Its client973a646 is superseded. Current image evidence is
`reviews/final-acceptance-c975ae2/final-client-image-c975ae2/manifest.json`:
new client image
`f2f39d841c027cb8de8cff9e4274a826d5dcc5b23465a88ca95981f6f1f4d2f4`
and unchanged server image
`551c68be95742fee7470ce61e212e3598751e3bb374db43333097d6755550848`
passed actual isolated prebuilt-image Compose, independent verification and fresh
review. Served21-file HTTP comparisons were byte-exact, ACK20 progressed and
owned sessions/clients/UDP counts went0→1→0 with complete cleanup. Original
service ports were preserved. This is bounded RTC/package evidence, not a
full artifactFiles/QA capture, combat/capacity, Firefox or performance claim.
No image was deployed or externally published; old serverd678 remains historical.

The c975 FIRST forced-reorder renewal passed `reorder-outcome.json` and its
independent21-file/three-QA audit: all12 strict predicates were true on first
poll, exact observerThor25/event31/mutation33 survived, held event IDs1/29/30/31
were retained and security/queues were zero. The old20ef/ec909/626 REDs remain
archived. Earlier ThorSensor parity and imported-Thor identity hypotheses were
not established: both sides preserve ThorSensor, and the decoder rejects the
proposed canonical-Thor/sensor combination. Source-only diagnostic recaptures
remain distinct from this actual successful browser observation.

Chrome's fully hostile two-bot run also passed, with actual observed child proof
in `chrome-2bot-outcome.json`: parent91/type4/weapon8868/group76, owner0/life7,
committed explosion at5881 with eight fragments; child97/type15 dealt400 damage
(energy293→0) to exact victim768/life2. Authenticated QA attachment upload and
GET200 returned377,617 bytes with SHA256
`f44e524cef40c146201e5b169cc6ee6cf66ae37afdbc849c1a05caa982258a7c`.
Source/server/WASM scope bindings and the observed commit were checked before
PASS. The general independent verifier covered14 files/one QA attachment;
`qaTraceChildAudit` is null, not a separate commit-labeled audit. The earlier
unexercised-child limitation is therefore resolved by this new observation,
without upgrading historical primary-only runs or missing-commit failures.
Hostility remained unchanged; audio recorded559 running starts/zero resume
failures and the primary moving anchor traveled353.23px.

Firefox's FIRST ordinary two-bot execution remains RED: six60-second pursuits
produced148 nonlethal hits (30 primary/118 children) against victim768/life2,
without the required death. `firefox-2bot-failure-analysis.json` retains13 raw
file hashes and source/served-identity audit only, not a full QA-manifest audit.
Reviewed fixture-only held-key correction b901→delivery9d57 advanced to an
actual death/observed commit, but that renewal remained RED at exact fresh-upload
versus GET equality: six last-bit floating-point differences after WebDriver
object transfer. Raw GET independently retained parent208/group163/owner0/life12,
child215/commit12985/fan8 and victim768/life1 damage6/energy1→0/death104; the
failed live identity gate prevents accepted-proof/PASS claims for that run.
Reviewed JSON-primitive capture7dac→delivery6167 preserves exact equality,
without epsilon or rounding. All ten compiled artifacts remain byte-unchanged;
the subsequent strict actual Firefox renewal passed as recorded below.

The old high-refresh common prerequisite and three keyboard cells passed with
full audits, including Firefox Chaos. Its DEBUG Firefox Extreme cell remains
RED:12 inclusive draw samples over151ms yielded72.8477Hz, below the90Hz/16-sample floor,
before gameplay. Markers were present; actual renderer was not captured, so no
GPU/software or host cause is inferred. The approved single optimized-release
Firefox Extreme renewal passed both the driver and independent full verifier
(five files/two QA traces), with12 strict input proofs, minimum117.65Hz/19 draws,
eight zero-canonical-step proofs and zero security/cleanup counts. The release
common prerequisite also passed. Exact evidence is
`final-continuation-audits/firefox-extreme-high-refresh-release-renewal-summary.json`.
An abbreviated-stdout external classifier was corrected before direct retained
verification; no browser rerun or product change supplied that correction.
This separate supported release renewal does not rewrite the DEBUG aggregate.

Latest-user snapshot09:49:46Z at
`44e01bd636345546cab7f2f8d2cd89ac991e85e3` was integrated privately with c975
as reviewed commit `2008e5cc6905e387662a45b59f8d0eb4ca42033b`, tree
`963d0cf096d8419f8fe26b8fd5f1ef7a6823fecf`. The105-path snapshot required16
manual resolutions/seven adaptations. Native1368/1470,24 cross-side cells,
143 Node checks, native-all-targets, debug/release WASM/package/import/notices
and formatting passed, with fresh review finding no blockers. Native overlay
release recorded1465 passing/11 ignored tests.
All four bounded overlay browser cells now pass: Chrome/Firefox ordinary release
startup, Chrome weapon cutover and mixed-browser release combat. The first mixed
run passed both strict gameplay/wire cycles but remained RED at cleanup because
an external Firefox callback contained a semicolon SyntaxError. Fresh-reviewed
R2 external adapter (18 focused tests) supplied one affected renewal, which passed
two exact cycles, RTC drain and full23-file/two-fresh-QA audit. The prior RED
remains immutable. Exact accepted manifest is
`reviews/final-user-acceptance-2008e5c-renewal-r2/manifest.json`; source and earlier
checks remain under `reviews/latest-user-overlay-20261017T094946Z/`.

That accepted overlay is the fixed09:49 snapshot, not an assurance about later
user commits: the original externally advanced to clean8b9741fe and was not
chased or modified. The private fixture combination was subsequently reviewed clear and committed
clean as `f1afd7aa906901489f66d93544ebc8be255693f9`, tree
`cd57ba2b5413b1b6107fc490e4adde043198cdb2`, parent2008. It reconciles only
seven fixture files from reviewed b901/7dac, with247 selected Node tests passing
and no production delta. Application artifacts and the four bounded acceptance
cells remain compiled2008; no rebuild/restamp or extra browser PASS is inferred.

The remaining delivery outcomes are complete on fixture6167/clientc975/serverec909:

- Firefox strict two-bot renewal passed a full13-file/one-QA audit. Primary-family4
  projectile163 dealt649 damage (energy480→0) to victim768/life2, followed by
  life3 damage1700→1300. All51 fresh projectile groups drew,566 audio starts
  ran with zero resume failures, security/queues were zero and cleanup completed.
  This successful run had `childCapture.exercised:false` and no captures: it
  neither proves primitive-JSON transport nor upgrades the earlier child GET RED.
- One separate genuine-input Firefox QA transport probe passed14-file/one-QA
  verification. Its25 accepted rotation/neutral samples and fresh upload/GET
  retained exactly equal whole JSON and raw bytes:148,943 bytes, SHA256
  `68ff260d9273b0f4cc01028a92eebf3c08c36ee7058c118bd6e8968c99196710`.
  This establishes the primitive-JSON transport path, with no death/child claim.
- Focused20 first failed before gameplay on headless Gecko SWGL/WebGL
  initialization; actor capture contains no actual Firefox channel states. The
  approved same-assertion headed-Xvfb renewal passed16 files/two QA captures.
  Manual21 passed14 files with two intentionally disabled QA captures; aimed22
  and bomb23 each passed16 files/two QA; transport24 passed19 files/two QA.
  All have independent full verification and empty owned-process cleanup. The
  default headless20 RED is retained separately from the supported renewal.

The exact retained outcomes are `firefox-2bot-json-identity-outcome.json`,
`primitive-json-transport-probe/outcome.json` and `final-focused-five-outcome.json`
under `reviews/final-acceptance-c975ae2/`. Canonical14's channel/mixed/capacity200/
immutable-image Compose decomposition and the earlier retained canonical/focused
passes remain source-qualified in the preceding evidence roots. Final acceptance
uses that documented decomposition and affected-source renewals, not a claim
that every unchanged default aggregate passed. Final evidence lives here and in
external manifests; packaged browser-runtime provenance keeps its migration-time
record, whose historical pending notes are superseded here without changing
accepted package/image inputs.

## Final release measurement checkpoint

`reviews/final-acceptance-c975ae2/matched-release-measurements/execution-map.json`
records eight cadence cells (four pairs) and two final startup-only cells,
each independently verified. Baseline client1dfb909 and final clientc975 used
fixture6167, the same ec909 debug server and optimized-release WASM. Each pair
matched actual renderer and viewport; all observed-load-overlap arrays were
empty. Chrome155 used ANGLE/SwiftShader WebGL1 at1280×720; headed Firefox158
used Mesa llvmpipe WebGL1 at1262×795, with drawing buffers recorded separately.
These are software-renderer measurements, not native-GPU performance claims.
Chaos/Extreme identify arenas; these ordinary URLs requested no impairment.

| Browser / arena | Baseline Hz | Final Hz | Baseline / final maximum gap |
| --- | ---: | ---: | ---: |
| Chrome / Chaos | 57.903 | 60.016 | 31.4 /18.9ms |
| Firefox / Chaos | 58.994 | 58.825 | 35 /47ms |
| Chrome / Extreme | 60.021 | 51.317 | 18.4 /34.5ms |
| Firefox / Extreme | 58.974 | 58.991 | 51 /37ms |

Each cadence cell sampled roughly60 seconds once. Chrome Extreme is14.50%
lower and remains a material performance concern. The bounded classification in
`chrome-extreme-bounded-analysis.json` found actual workload equivalence was not
established: candidate damage began6.924s into capture, with a lethal hit9.509s
in and an accepted life2 respawn; baseline damage began44.995s in, with a lethal
hit48.928s in and a rejected respawn baseline. The screenshots showing2800
versus1650 energy were saved after disconnect/cleanup, not during cadence, and
cannot establish per-frame rendering work. Ordinary QA was disabled; there are
no entity/draw-count histories, full interval series or CPU profiles. Finite
passive samples found no overlapping heavy processes but cannot exclude all
unsampled/shared-system effects. The actual ordinary gates (at least20 callbacks/
second and no gaps≥80ms, plus genuine input/delivery/security/cleanup) passed
unchanged. Measuring/reporting is complete; this combined n=1 comparison isolates
neither a library regression nor a benefit and does not dismiss the lower result. The two final Chaos startup-only probes passed: navigation→gameplay was
3714.5ms in Chrome and4765ms in Firefox (one sample each). Navigation→loading
complete/harness-ready was3215.4/5358.4ms in Chrome and3354/6312ms in Firefox;
these final-only probes supply no new paired startup speed inference. Host WASM grew6,440,789→6,573,379 bytes
(+132,590/+2.06%) under the same Rust1.98.1 producer; paired JS stayed38,256.
This includes application/API changes and is not attributable solely to image.

`window-closure.json` records all1102 source files, ten actual artifacts and14
baseline files unchanged, with no hash mismatches or owned processes left.
All ten cells passed independent full audits, and the measure-and-report outcome
is achieved with the concern and attribution limits above retained. No further
case/probe remains in the approved acceptance queue. Documentation review/commit
is the remaining handoff step; this report does not imply an unknown future
commit or alter tested artifacts. No app/image deployment, change to the original
user branch/services, or upstream issue/PR/advisory submission has occurred.
The private reviewed user integration and published Macroquad revision retain
their distinct source identities and limitations.

## Reviewed deferrals and rejections

All candidates below were open and unmerged on 2026-10-16. Their diffs,
discussions, and head revisions were refreshed with `gh`; do not assume those
heads remain current later.

| PR and reviewed head | Decision and concrete reason |
| --- | --- |
| [#1075](https://github.com/not-fl3/macroquad/pull/1075), `2d29112684ce7984e47b0c6096ed6d762d89ef96` | Defer. Browser loading progress is HTML (`src/assets/load/progress.rs`, WASM branch), and gameplay uses bitmap sprites. The native loader alone calls `draw_text`; no demonstrated browser atlas-upload benefit. |
| [#1076](https://github.com/not-fl3/macroquad/pull/1076), `856a67bdc86468c39e689416114400ab9d89c756` | Defer. Browser loading progress is HTML and gameplay text is bitmap sprites, so neither establishes a need for font-atlas copying/growth optimization. Revisit only after a measured use case; image invariants are handled separately. |
| [#1078](https://github.com/not-fl3/macroquad/pull/1078), `c09450ff3f94162d8ca6954b40f42d85aad7f0eb` | Defer with the atlas group. Edge padding changes glyph packing/growth/filtering. Browser loading progress is HTML and gameplay text is bitmap sprites; no applicable artifact reproduction justifies this change today. |
| [#1067](https://github.com/not-fl3/macroquad/pull/1067), `ac0742908ad68b4a26e49b7ea215f178591e4ae6` | Defer. No Zubspace wheel-input usage. The paired [Miniquad #661](https://github.com/not-fl3/miniquad/pull/661), head `5df44b12602401bcb0f166e6c8f44a6ec0fd85e6`, also remains open and changes scrolling units/directions; take a tested pair when needed. |
| [#1072](https://github.com/not-fl3/macroquad/pull/1072), `816aece995a9b3f44943c32fd1c0fc0236eff92b` | Reject for this fork's current scope. Raising the pipeline cap from 32 to 128 serves games with many custom materials; Zubspace has none. Lazy UI already avoids one unused slot. |
| [#1077](https://github.com/not-fl3/macroquad/pull/1077), `175ec99b196c565385f9ffa3c6da41a042ac123f` | Defer. Zubspace does not load custom TTF fonts; removing their preload does not address its gameplay bitmap renderer. Default UI fonts already begin without that preload. |
| [#1061](https://github.com/not-fl3/macroquad/pull/1061), `3d1a775d4c2458b9547ed7f52414a66621228b2b` | Reject for current scope. Changes the default 3D camera orientation; Zubspace uses 2D rendering. Other unrelated 3D additions remain outside this fork's purpose. |
| [#1018](https://github.com/not-fl3/macroquad/pull/1018), `c746edd931a0d8cd9cfa094b0581977fc325636d` | Defer. Addresses Linux/X11 implicit pointer grabs, not browser focus/input. No current native Zubspace reproduction; retain native correctness checks without importing an unrelated input policy change. |
| [#878](https://github.com/not-fl3/macroquad/pull/878), `1019ade9e572e3f1c748736252f9954c845452e9` | Reject as remediation. Adds accessors/deprecations but leaves public image fields mutable; current code also differs from its base. Adapt useful ideas only with complete invariant/overflow/transfer enforcement. |
| [#867](https://github.com/not-fl3/macroquad/pull/867), `919f3ad1e4ac99a2c886bdcc2706a42a4f32daec` | Reject as a soundness fix. Only adds deprecation warnings to safe tween functions that still execute undefined behavior. The reviewer explicitly suggests fixing their API. |
| [#849](https://github.com/not-fl3/macroquad/pull/849), `2faab3bfa9de22459a9e04ca1ca43cd594897a15` | Reject. The author reports that the alpha-blend change makes textures disappear in 3D scenes; no corrective commit or validating discussion was present. |

The initial library batch rolls back to the upstream base above. Zubspace's
paired pre-fork integration rollback is
`1dfb9096a19d614f08a4aedd9109011dc7d3796e`; restore Rust dependencies/lockfile,
runtime sources/generated bundle/provenance and Image/task migrations together.
Its integration `725dd5d` pins this published fork revision and requires the
audio-state extension. The application checkpoint and final acceptance artifacts
must be identified separately from the immutable library revision.
