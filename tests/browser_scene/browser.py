#!/usr/bin/env python3
"""Finite generated-scene correctness gate and diagnostic matched timing trials.

Run under Xvfb/Openbox with the same browser/driver setup as browser_release.sh.
Both explicit WebGL paths use release WASM and the included paired runtime.
"""
import argparse
import base64
import functools
import hashlib
import http.server
import json
import math
import os
from pathlib import Path
import shutil
import statistics
import subprocess
import sys
import threading
import urllib.parse

sys.dont_write_bytecode = True
ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / "tests"))
import ui_material_browser as driver


def sha256(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def source_identity():
    names = sorted(set(subprocess.check_output(
        ["git", "ls-files", "-co", "--exclude-standard", "-z"], cwd=ROOT,
    ).decode().strip("\0").split("\0")))
    files = {name: sha256(ROOT / name) for name in names}
    return {"gitHead": subprocess.check_output(["git", "rev-parse", "HEAD"], cwd=ROOT).decode().strip(),
            "dirty": bool(subprocess.check_output(["git", "status", "--porcelain"], cwd=ROOT)),
            "files": files,
            "inventorySha256": hashlib.sha256(json.dumps(files, sort_keys=True).encode()).hexdigest()}


def build(output):
    before = source_identity()
    env = os.environ.copy()
    env["CARGO_TARGET_DIR"] = str(ROOT / "target/browser-scene-build")
    env["CARGO_TARGET_WASM32_UNKNOWN_UNKNOWN_RUSTFLAGS"] = env.get("CARGO_TARGET_WASM32_UNKNOWN_UNKNOWN_RUSTFLAGS", "") + " -C link-arg=--allow-undefined"
    command = ["cargo", "build", "--manifest-path", str(ROOT / "tests/browser_scene/Cargo.toml"),
               "--release", "--target", "wasm32-unknown-unknown"]
    commands = []
    for version in (1, 2):
        invocation = command + (["--features", "webgl2"] if version == 2 else [])
        subprocess.run(invocation, cwd=ROOT, env=env, check=True)
        destination = output / f"webgl{version}.wasm"
        shutil.copyfile(Path(env["CARGO_TARGET_DIR"]) / "wasm32-unknown-unknown/release/browser-scene-regression.wasm", destination)
        subprocess.run(["node", "tools/check_browser_runtime.mjs", "--wasm", str(destination), "--no-audio"], cwd=ROOT, check=True)
        commands.append(invocation)
    if source_identity() != before:
        raise RuntimeError("Source identity changed during fixture build")
    tool_commands = {
        "node": ["node", "--version"],
        "chrome": [os.environ.get("CHROME_BIN") or shutil.which("google-chrome") or shutil.which("chromium"), "--version"],
        "chromedriver": [os.environ.get("CHROMEDRIVER_BIN", "chromedriver"), "--version"],
        "firefox": [os.environ.get("FIREFOX_BIN") or shutil.which("firefox"), "--version"],
        "geckodriver": [os.environ.get("GECKODRIVER_BIN", "geckodriver"), "--version"],
    }
    return {"source": before, "commands": commands, "profile": "release",
            "tools": {name: subprocess.check_output(command, stderr=subprocess.STDOUT).decode().splitlines()[0] for name, command in tool_commands.items()},
            "rustc": subprocess.check_output(["rustc", "--version"], cwd=ROOT).decode().strip(),
            "rustflags": env["CARGO_TARGET_WASM32_UNKNOWN_UNKNOWN_RUSTFLAGS"],
            "lockSha256": sha256(ROOT / "tests/browser_scene/Cargo.lock"),
            "runtimeSha256": sha256(ROOT / "js/mq_js_bundle.js"),
            "wasmSha256": {str(version): sha256(output / f"webgl{version}.wasm") for version in (1, 2)}}


def validate_submissions(result):
    frames = result.get("submissions", {}).get("frames", [])
    expected = [(0, "camera"), (0, "screen"), (1, "screen"), (1, "camera"), (2, "camera"), (2, "screen")]
    if [(frame.get("trial"), frame.get("variant")) for frame in frames] != expected:
        raise AssertionError("Missing or reordered submission frames")
    # Independent workload arithmetic: 1,200 tiles + 64 ships + 512 projectiles
    # + 32*8 label glyphs + HUD rectangle + 16 HUD glyphs = 2,049 quads.
    for frame in frames:
        counts = frame.get("counts", {})
        calls = counts.get("calls", {})
        for name in ("vertexUploadBytes", "indexUploadBytes", "bufferDataBytes", "submittedIndices", "submittedVertices"):
            value = counts.get(name)
            if not isinstance(value, int) or isinstance(value, bool) or value < 0:
                raise AssertionError("Invalid submission byte/work count")
        if (counts.get("vertexUploadBytes") != 2049 * 4 * 40
                or counts.get("indexUploadBytes") != 2049 * 6 * 2
                or counts.get("submittedIndices") != 2049 * 6
                or counts.get("submittedVertices") != 0
                or counts.get("bufferDataBytes") != 0):
            raise AssertionError("Submission workload differs from the fixed scene")
        for name, value in calls.items():
            if not isinstance(value, int) or isinstance(value, bool) or value < 0:
                raise AssertionError("Invalid submission call count")
        draws = calls.get("drawElements", 0) + calls.get("drawElementsInstanced", 0)
        # Each flush uploads one vertex/index pair per shared segment (never more
        # than one pair per call) and draws inside at least one begun/ended pass.
        uploads = calls.get("bufferSubData", 0)
        if (draws <= 0 or uploads % 2 or not 2 <= uploads <= draws * 2
                or calls.get("bindFramebuffer", 0) < 2):
            raise AssertionError("Missing actual submission calls")
        if any(calls.get(name, 0) for name in ("bufferData", "createBuffer", "deleteBuffer", "createTexture", "deleteTexture", "texImage2D", "texSubImage2D", "drawArrays", "drawArraysInstanced")):
            raise AssertionError("Unexpected warm scene resource churn")


def validate(result, version):
    if result.get("errors") or not result.get("completed"):
        raise AssertionError("Scene did not complete cleanly")
    context = result.get("context", {})
    if (context.get("requested") != version or context.get("actual") != version
            or not context.get("version", "").startswith(f"WebGL {version}.0")
            or context.get("width") != 640 or context.get("height") != 480):
        raise AssertionError("Actual WebGL context does not match the requested path")
    if result.get("viewport") != {"width": 640, "height": 480, "devicePixelRatio": 1}:
        raise AssertionError("Unexpected viewport or DPR")
    if result.get("gpuTimingMs") is not None or not result.get("timing", {}).get("diagnosticOnly"):
        raise AssertionError("Unsupported GPU timing or timing acceptance claim")
    if (result.get("baselineTextures") is None or result.get("finalTextures") != result["baselineTextures"]
            or result.get("baselineOwnedTextures") is None or result.get("finalOwnedTextures") != result["baselineOwnedTextures"]
            or result.get("shutdownTextures") != 0):
        raise AssertionError("Texture cleanup was not observed")
    expected = [(0, "camera"), (0, "screen"), (1, "screen"), (1, "camera"), (2, "camera"), (2, "screen")]
    trials = result.get("trials", [])
    pictures = result.get("pictures", [])
    if ([(trial["trial"], trial["variant"]) for trial in trials] != expected
            or [(picture["trial"], picture["variant"]) for picture in pictures] != expected):
        raise AssertionError("Missing or reordered matched trials/visual checks")
    for trial in trials:
        for name in ("cpuSceneApiMs", "rafIntervalsMs"):
            values = trial.get(name, [])
            if len(values) != 18 or any(not isinstance(v, (int, float)) or not math.isfinite(v) or v < 0 for v in values):
                raise AssertionError("Missing or invalid timing samples")
        if len(trial.get("visibility", [])) != 19 or any(v != "visible" for v in trial["visibility"]):
            raise AssertionError("Trial was backgrounded or lacks scheduling evidence")
    for picture in pictures:
        if not picture.get("equivalent") or picture.get("glError") != 0 or len(picture.get("witnesses", [])) != 14:
            raise AssertionError("Visual/GL validation missing")
        digest = picture.get("rgbaSha256", "")
        if len(digest) != 64 or any(c not in "0123456789abcdef" for c in digest):
            raise AssertionError("Framebuffer hash missing")
    if len({picture["rgbaSha256"] for picture in pictures}) != 1:
        raise AssertionError("Matched trials produced different framebuffers")


def summarize(result):
    trials = [{"trial": t["trial"], "variant": t["variant"],
               "cpuSceneApiMedianMs": statistics.median(t["cpuSceneApiMs"]),
               "rafIntervalMedianMs": statistics.median(t["rafIntervalsMs"])} for t in result["trials"]]
    return {"diagnosticOnly": True, "gpuTimingMs": None, "trials": trials,
            "interpretation": "Matched fixed-scene diagnostics, not end-to-end GPU time or a game/library speedup; no timing threshold"}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--submissions", action="store_true", help="Observe post-timing frames only; no performance claim")
    parser.add_argument("--output", type=Path, default=ROOT / "target/browser-scene")
    args = parser.parse_args()
    output = args.output.resolve()
    output.relative_to(ROOT)
    output.mkdir(parents=True, exist_ok=True)
    build_metadata = build(output)
    (output / "build.json").write_text(json.dumps(build_metadata, indent=2) + "\n")
    server = http.server.ThreadingHTTPServer(("127.0.0.1", 0), functools.partial(driver.Handler, directory=str(ROOT)))
    threading.Thread(target=server.serve_forever, daemon=True).start()
    summaries = {}
    try:
        for browser in ("chrome", "firefox"):
            for version in (1, 2):
                driver.OUTPUT = output / f"{browser}-webgl{version}"
                driver.OUTPUT.mkdir(exist_ok=True)
                wasm = "/" + (output / f"webgl{version}.wasm").relative_to(ROOT).as_posix()
                url = f"http://127.0.0.1:{server.server_port}/tests/browser_scene/case.html?" + urllib.parse.urlencode({"wasm": wasm, **({"submissions": "1"} if args.submissions else {})})
                driver.check_browser(browser, url, marker="browser scene regression: passed", check_ui=False, timeout=60, force_dpr1=True)
                evidence_path = driver.OUTPUT / f"{browser}.json"
                result = json.loads(evidence_path.read_text())
                validate(result, version)
                if args.submissions:
                    validate_submissions(result)
                preview = result.pop("previewDataUrl")
                if not preview.startswith("data:image/png;base64,"):
                    raise AssertionError("Scene preview is not PNG")
                (driver.OUTPUT / "scene.png").write_bytes(base64.b64decode(preview.split(",", 1)[1], validate=True))
                result["build"] = {key: value for key, value in build_metadata.items() if key != "source"}
                result["source"] = {key: value for key, value in build_metadata["source"].items() if key != "files"}
                result["diagnostics"] = summarize(result)
                evidence_path.write_text(json.dumps(result, indent=2) + "\n")
                summaries[f"{browser}-webgl{version}"] = {"context": result["context"], **result["diagnostics"]}
                if args.submissions:
                    summaries[f"{browser}-webgl{version}"]["submissions"] = result["submissions"]
                print(f"{browser} WebGL{version}: 6 matched visual checks, 108 measured frames, cleanup passed; timing diagnostic only", flush=True)
    finally:
        server.shutdown()
        server.server_close()
    if source_identity() != build_metadata["source"]:
        raise RuntimeError("Source identity changed during browser validation")
    (output / "summary.json").write_text(json.dumps(summaries, indent=2) + "\n")


if __name__ == "__main__":
    main()
