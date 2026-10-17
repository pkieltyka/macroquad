# Generated browser scene

The full local release gate (`make test` or `make release-check`) runs
`tests/browser_scene/browser.py` in addition to the existing audio, ownership,
UI, codec and texture/pass fixtures. It builds
release WASM and checks explicit WebGL1 and WebGL2 in Chrome and Firefox serially.
The intended browser product remains independent of OS; current validated cells use Linux
and software graphics, not cross-OS or physical-GPU qualification.

The scene is fixed at 640×480, DPR1, sample count1, seed `0x5eed0040` and a fixed
pose. Three generated nearest-filtered sheets provide 1,200 atlas tiles,
64 ships, 512 projectiles, 32 eight-glyph bitmap labels and 16 HUD glyphs. No
application asset, network service, render target or application build is used.
Existing fixtures retain churn, UI, depth and pass coverage.
The scene opts into WebDriver's explicit DPR1 setup (Chrome scale factor1,
Firefox `layout.css.devPixelsPerPx=1.0` and `browser.display.os-zoom-behavior=0`)
and rejects any differing observed DPR. Firefox otherwise multiplies the
requested scale by the OS text scale, as described in its
[preference source](https://github.com/mozilla-firefox/firefox/blob/main/modules/libpref/init/StaticPrefList.yaml).
The local release display is explicitly 1280×960 at 96dpi; the scene requests an
800×650 browser window and records the actual content viewport. It must fit the
640×480 canvas completely.

Two workload variants preserve entity counts, label projection and draw order:
tiles, ships, projectiles, labels, then HUD. `camera` switches to the default
camera and back for each label. `screen` switches once for the whole label phase.
Both render the same scene; this is an explicit workload comparison, not a
change to an application or adoption of an upstream optimization.

Each browser/context cell runs three matched trials, alternating variant order
(`camera/screen`, `screen/camera`, `camera/screen`). Each variant warms up for six
frames and measures 18 frames: 108 measured frames per cell, 432 across four
cells. Readback and GL diagnostics run after each measured window. Fourteen
independent pixel witnesses cover atlas cells/borders, distant ships/projectiles,
bitmap glyphs, transparency, label-over-ship ordering and HUD-over-tile ordering.
Every complete RGBA framebuffer must also match the first variant byte for byte,
including all later trials. GL errors, generated texture cleanup and engine
texture shutdown are correctness gates. Negative oracle tests reject blank
output, each corrupted witness and differences outside the witnesses.

Timing is diagnostic and has no acceptance threshold:

- `cpuSceneApiMs` measures Rust scene API issuance. It includes immediate camera
  flushes and any driver work those cause, but excludes deferred end-frame
  submission, GPU completion, sample logging and readback. Because the variants
  defer different amounts of work, this is not total rendering time. A
  fixture-only adapter supplies `performance.now()` to the existing `env.now`
  import instead of the shipped millisecond wall clock; browser precision limits
  still apply. The shipped runtime bytes are unchanged.
- `rafIntervalsMs` measures requestAnimationFrame timestamp intervals across
  the measured window. It includes vsync, scheduling, end-frame work and fixture
  instrumentation. It is cadence, not a GPU-duration measurement.
- `gpuTimingMs` is `null`: GPU timing is unsupported, not measured as zero.

The evidence contains raw samples, per-trial medians, actual GL version and
renderer (unmasked when available), browser user agent, viewport, fixed workload,
release profile/compiler/flags, lockfile/WASM/runtime hashes and a source-file
inventory bound to Git HEAD and dirty state. A build/source guard rejects edits
during the run. Record local release evidence against the reviewed source;
GitHub CI records its separate bounded graphics-smoke evidence. Median
differences from these short software-renderer trials do not establish a game
FPS improvement or isolated library regression. Assess a proposed optimization
using repeated matched runs of the same workload/build/renderer/host conditions;
retain the raw observations and deferred-work limits.

Run under the [release browser setup](fork.md#reproduce-the-fork-checks):

```sh
make test
```

For the scene alone, start Openbox on the isolated display, then run
`python3 tests/browser_scene/browser.py`. Evidence defaults to
`target/browser-scene/`; the release runner uses `target/browser-release/scene/`.
Each cell also saves the first validated framebuffer as `scene.png` for inspection.
No timing result is required to stay below a machine-dependent threshold.
The GitHub graphics-smoke job does not run this scene or require an audio device.
See [the command/coverage policy](../AGENTS.md) for the deliberate CI/local split.

The workload was informed by read-only Zubspace source at
`42bf0b68d358ef41a06b8e7a6b8190b87e418e0e`: per-label camera switches in
`src/game/render/ships.rs`, per-glyph draws/texture metadata in `src/font.rs`,
atlas tiles in `src/game/render.rs` and projectile sprites in
`src/game/render/objects/projectiles.rs`. The generated fixture is self-contained;
those sources and all game assets/services remain outside its dependencies.
Broader browser qualification and controlled GPU/performance baselines remain
on the [pending browser roadmap](browser-roadmap.md).

For optional submission diagnostics, add `--submissions` to `browser.py` and use
an output directory such as `target/browser-efficiency-0.5.1/`. The fixture wraps
WebGL calls only in the post-timing verification frame, from `SCENE_END` through
the next RAF entry, including end-frame submission/commit. It records actual
plain/instanced draw calls, submitted indices, buffer update bytes/calls,
framebuffer/texture binds and buffer/texture allocation or upload churn. This
mode keeps wrappers out of timing trials and leaves the default browser suite
unchanged. `make check-runtime` tests the observer and rejects incomplete count
protocols without launching browsers.

The count oracle independently requires 2,049 quads, 8,196 vertices (40 bytes
each), and 12,294 u16 indices for both variants, with the existing independent
pixel witnesses and whole-frame equality still required. Counts report WebGL
calls, not driver stalls, GPU durations, actual framebuffer transitions or
startup/retained capacity. The scene uses the default framebuffer and warmed
textures: it does not establish offscreen/MSAA or atlas-growth performance.
See the [0.5.1 analysis and scope](../plans/0.5.1-browser-efficiency.md) for the
recorded baseline and separate proposed experiments.
