# Browser priorities after 0.5.0

The primary product is HTML5 games using WASM/WebGL1/2, with Chrome and Firefox
across modern operating systems. Native Linux X11/OpenGL supports development
and testing. These pending follow-ups reflect the clarified browser priority;
they do not reopen completed historical implementation/acceptance outcomes or
add release blockers without evidence of a current defect.

## Intended target and verified coverage

| Browser/host | Intent and evidence |
| --- | --- |
| Chrome/Firefox on Linux | Primary browser target; existing finite browser fixtures and historical application acceptance ran on Linux hosts. Their recorded software/hardware and workload limits remain in force. |
| Chrome/Firefox on Windows and macOS | Primary browser target; cross-OS qualification remains pending. Linux-hosted browser results do not establish these combinations as tested. |
| Chrome/Firefox on other modern OS environments | Intended where the required WebAssembly, WebGL and browser audio capabilities are available; qualification remains pending. |
| Safari/WebKit, mobile browsers and device/GPU combinations | Any additional qualification must be explicit. No Safari/mobile or universal device/driver acceptance is claimed. |

- [ ] Define and record browser versions, OS/device/GPU combinations and required
  capabilities for a practical qualification matrix. Keep intended targets
  distinct from actual verified cells and retained failures.
- [ ] Extend browser lifecycle/input regressions for focus recovery and background
  timing, resize/DPI changes, mouse release outside the canvas and audio unlock.
  Preserve genuine user gestures and browser scheduling behavior in the checks.
- [ ] Define and test graceful behavior after graphics context loss. The current
  recovery limit is a page reload; context restoration is not implemented.
- [ ] Establish a controlled browser/GPU performance baseline before considering
  Miniquad #682 or #672. Match rendering work, renderer, viewport and build
  identity; record frame/startup series and limits. Preserve the historical
  Chrome Extreme lower-cadence concern without claiming isolated attribution or
  an improvement from this release.
- [ ] Evaluate Miniquad #602 for relevant browser correctness using refreshed
  source/discussion and a concrete regression before adopting a change.
- [ ] Keep native platform work focused on maintenance and development/test needs.
  Expansion to native Windows/macOS or other backends is not a current product
  priority; existing native source is retained.

No candidate patch is adopted by this roadmap. Any future runtime/backend change
needs its own source review, Rust/JavaScript pairing, focused regressions and
appropriate browser acceptance. See [current support and migration](fork.md),
[security limits](security.md) and [historical patch/evidence ledger](patch-ledger.md).
