#!/usr/bin/env python3
"""Public capacity policy: debug/release, both browsers and explicit WebGL paths."""
import argparse
import functools
import http.server
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
import threading
import urllib.parse

sys.dont_write_bytecode = True
ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / "tests"))
import ui_material_browser as driver
from browser_scene.browser import sha256, source_identity


def validate(result, version, profile):
    if result.get("errors") or not result.get("completed"):
        raise AssertionError(result)
    context = result["context"]
    if not context["version"].startswith(f"WebGL {version}.0"):
        raise AssertionError(context)
    if (context["attributes"]["antialias"] or context["samples"] != 0
            or context["sampleBuffers"] != 0
            or any(r["attributes"] != {"antialias": False} for r in context["requests"])):
        raise AssertionError(context)
    resources = result["resources"]
    if resources["createdBuffers"] <= 0 or resources["liveBuffers"] != 0:
        raise AssertionError(resources)
    names = ["policy-arc-default", "policy-arc-wrap", "policy-tiny", "policy-setter"]
    if profile == "release":
        names.insert(0, "policy-reject")
    if [p["name"] for p in result["phases"]] != names:
        raise AssertionError("Missing/reordered policy phases")
    diagnostics = [m for m in result["messages"] if "Entire call rejected" in m]
    if len(diagnostics) != (1 if profile == "release" else 0):
        raise AssertionError(diagnostics)
    if diagnostics and not all(s in diagnostics[0] for s in (
        "9 vertices/3 indices", "8 vertices/12 indices", "submit smaller meshes", "valid local indices"
    )):
        raise AssertionError(diagnostics)
    for phase in result["phases"]:
        name, draws = phase["name"], phase["draws"]
        if not phase.get("completePixels") or phase.get("glError") != 0 or not draws:
            raise AssertionError(phase)
        max_vertices, max_indices = {
            "policy-arc-default": (10_000, 5_000),
            "policy-arc-wrap": (65_536 if version == 1 else 65_535, 100_000),
            "policy-tiny": (3, 3), "policy-setter": (8, 12), "policy-reject": (8, 12),
        }[name]
        for draw in draws:
            if (draw["mode"] != 4 or not 0 < draw["count"] <= max_indices
                    or not 0 < draw["vertexCount"] <= max_vertices
                    or not 0 <= draw["maxIndex"] < draw["vertexCount"]
                    or draw["vertexBytes"] != draw["vertexCount"] * 40
                    or draw["indexBytes"] != draw["count"] * 2):
                raise AssertionError(draw)
        if name.startswith("policy-arc-"):
            segments = 834 if name.endswith("default") else 17_001
            if (phase["segments"] != segments or phase["arcDegrees"] != (1177 if segments == 834 else 24_001)
                    or sum(d["count"] for d in draws) != segments * 6
                    or phase["sourceIndices"] != segments * 6 or len(draws) < 2
                    or not 0 <= phase["maxPositionError"] <= 0.002
                    or not 0 <= phase["boundaryPixels"] <= 128):
                raise AssertionError(phase)
        elif name == "policy-tiny":
            if [(d["count"], d["vertexCount"], d["maxIndex"]) for d in draws] != [(3, 3, 2)] * 6:
                raise AssertionError(phase)
        elif name == "policy-setter":
            if [(d["count"], d["vertexCount"], d["maxIndex"]) for d in draws] != [(6, 4, 3)] * 3:
                raise AssertionError(phase)
        elif sum(d["count"] for d in draws) != 18:
            raise AssertionError(phase)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--output", type=Path, default=ROOT / "target/browser-capacity-policy")
    args = parser.parse_args()
    output = args.output.resolve()
    output.relative_to(ROOT)
    output.mkdir(parents=True, exist_ok=True)
    source = source_identity()
    env = os.environ.copy()
    env["CARGO_TARGET_DIR"] = str(ROOT / "target/browser-capacity-build")
    env["CARGO_TARGET_WASM32_UNKNOWN_UNKNOWN_RUSTFLAGS"] = env.get("CARGO_TARGET_WASM32_UNKNOWN_UNKNOWN_RUSTFLAGS", "") + " -C link-arg=--allow-undefined"
    artifacts = {}
    for version in [1, 2]:
        for profile in ["debug", "release"]:
            command = ["cargo", "build", "--manifest-path", "tests/browser_capacity/Cargo.toml", "--bin", "browser-capacity-policy", "--target", "wasm32-unknown-unknown"]
            if version == 2:
                command += ["--features", "webgl2"]
            if profile == "release":
                command += ["--release"]
            subprocess.run(command, cwd=ROOT, env=env, check=True)
            artifact = output / f"webgl{version}-{profile}.wasm"
            shutil.copyfile(Path(env["CARGO_TARGET_DIR"]) / f"wasm32-unknown-unknown/{profile}/browser-capacity-policy.wasm", artifact)
            subprocess.run(["node", "tools/check_browser_runtime.mjs", "--wasm", str(artifact), "--no-audio"], cwd=ROOT, check=True)
            artifacts[artifact.name] = sha256(artifact)
    tools = {
        "node": ["node", "--version"], "rustc": ["rustc", "--version"],
        "chrome": [os.environ.get("CHROME_BIN") or shutil.which("google-chrome") or shutil.which("chromium"), "--version"],
        "chromedriver": [os.environ.get("CHROMEDRIVER_BIN", "chromedriver"), "--version"],
        "firefox": [os.environ.get("FIREFOX_BIN") or shutil.which("firefox"), "--version"],
        "geckodriver": [os.environ.get("GECKODRIVER_BIN", "geckodriver"), "--version"],
    }
    metadata = {"source": source, "tools": {name: subprocess.check_output(cmd, stderr=subprocess.STDOUT).decode().splitlines()[0] for name, cmd in tools.items()},
                "lockSha256": sha256(ROOT / "tests/browser_capacity/Cargo.lock"),
                "runtimeSha256": sha256(ROOT / "js/mq_js_bundle.js"), "wasmSha256": artifacts}
    (output / "build.json").write_text(json.dumps(metadata, indent=2) + "\n")
    server = http.server.ThreadingHTTPServer(("127.0.0.1", 0), functools.partial(driver.Handler, directory=str(ROOT)))
    threading.Thread(target=server.serve_forever, daemon=True).start()
    try:
        for browser in ["chrome", "firefox"]:
            for version in [1, 2]:
                for profile in ["debug", "release"]:
                    driver.OUTPUT = output / f"{browser}-webgl{version}-{profile}"
                    driver.OUTPUT.mkdir(exist_ok=True)
                    artifact = output / f"webgl{version}-{profile}.wasm"
                    url = f"http://127.0.0.1:{server.server_port}/tests/browser_capacity/case.html?" + urllib.parse.urlencode({"wasm": "/" + artifact.relative_to(ROOT).as_posix()})
                    driver.check_browser(browser, url, marker="browser capacity regression: passed", check_ui=False, timeout=45, force_dpr1=True)
                    result = json.loads((driver.OUTPUT / f"{browser}.json").read_text())
                    validate(result, version, profile)
                    print(f"{browser} WebGL{version} {profile}: policy pixels/source order/bounds/diagnostics passed", flush=True)
    finally:
        server.shutdown()
        server.server_close()
    if source_identity() != source:
        raise RuntimeError("Source changed during capacity policy validation")


if __name__ == "__main__":
    main()
