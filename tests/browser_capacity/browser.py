#!/usr/bin/env python3
"""Actual WebGL upload/index bounds and complete boundary pixels in four cells."""
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
sys.path.insert(0, str(ROOT / "tests/browser_scene"))
import ui_material_browser as driver
from browser import sha256, source_identity


def validate(result, version):
    if not result.get("context", {}).get("version", "").startswith(f"WebGL {version}.0"):
        raise AssertionError("Actual WebGL version differs from requested path")
    expected = {
        "triangles": [(12, 8, 7), (6, 4, 3)],
        "lines": [(4, 4, 3), (2, 2, 1)],
        "u16-domain": ([(12, 65_536, 65_535), (6, 4, 3)] if version == 1
                       else [(6, 65_532, 65_531), (12, 8, 7)]),
        "last-portable-vertex": [(6, 65_535, 65_534), (12, 8, 7)],
        "index-count": [(66_000, 4, 3)],
        "material-order": [(6, 4, 3)] * 3,
    }
    if result.get("errors") or not result.get("completed"):
        raise AssertionError(result)
    if [phase["name"] for phase in result["phases"]] != list(expected):
        raise AssertionError("Missing/reordered capacity phases")
    for phase in result["phases"]:
        draws = phase["draws"]
        if [(d["count"], d["vertexCount"], d["maxIndex"]) for d in draws] != expected[phase["name"]]:
            raise AssertionError(phase)
        if not phase.get("completePixels") or phase.get("glError") != 0:
            raise AssertionError(phase)
        for draw in draws:
            if (draw["mode"] != (1 if phase["name"] == "lines" else 4)
                    or draw["vertexBytes"] != draw["vertexCount"] * 40
                    or draw["indexBytes"] != draw["count"] * 2):
                raise AssertionError(draw)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--output", type=Path, default=ROOT / "target/browser-capacity")
    args = parser.parse_args()
    output = args.output.resolve()
    output.relative_to(ROOT)
    output.mkdir(parents=True, exist_ok=True)
    source = source_identity()
    env = os.environ.copy()
    env["CARGO_TARGET_DIR"] = str(ROOT / "target/browser-capacity-build")
    env["CARGO_TARGET_WASM32_UNKNOWN_UNKNOWN_RUSTFLAGS"] = env.get("CARGO_TARGET_WASM32_UNKNOWN_UNKNOWN_RUSTFLAGS", "") + " -C link-arg=--allow-undefined"
    for version in (1, 2, "auto"):
        command = ["cargo", "build", "--manifest-path", str(ROOT / "tests/browser_capacity/Cargo.toml"), "--target", "wasm32-unknown-unknown"]
        subprocess.run(command + (["--features", "webgl2"] if version == 2 else ["--features", "auto"] if version == "auto" else []), env=env, cwd=ROOT, check=True)
        shutil.copyfile(Path(env["CARGO_TARGET_DIR"]) / "wasm32-unknown-unknown/debug/browser-capacity-regression.wasm", output / f"webgl{version}.wasm")
        subprocess.run(["node", "tools/check_browser_runtime.mjs", "--wasm", str(output / f"webgl{version}.wasm"), "--no-audio"], cwd=ROOT, check=True)
    tool_commands = {
        "node": ["node", "--version"],
        "chrome": [os.environ.get("CHROME_BIN") or shutil.which("google-chrome") or shutil.which("chromium"), "--version"],
        "chromedriver": [os.environ.get("CHROMEDRIVER_BIN", "chromedriver"), "--version"],
        "firefox": [os.environ.get("FIREFOX_BIN") or shutil.which("firefox"), "--version"],
        "geckodriver": [os.environ.get("GECKODRIVER_BIN", "geckodriver"), "--version"],
    }
    metadata = {"source": source, "profile": "debug",
                "tools": {name: subprocess.check_output(command, stderr=subprocess.STDOUT).decode().splitlines()[0] for name, command in tool_commands.items()},
                "lockSha256": sha256(ROOT / "tests/browser_capacity/Cargo.lock"), "runtimeSha256": sha256(ROOT / "js/mq_js_bundle.js"),
                "wasmSha256": {str(v): sha256(output / f"webgl{v}.wasm") for v in (1, 2, "auto")},
                "rustc": subprocess.check_output(["rustc", "--version"]).decode().strip()}
    (output / "build.json").write_text(json.dumps(metadata, indent=2) + "\n")
    server = http.server.ThreadingHTTPServer(("127.0.0.1", 0), functools.partial(driver.Handler, directory=str(ROOT)))
    threading.Thread(target=server.serve_forever, daemon=True).start()
    try:
        for browser in ("chrome", "firefox"):
            for name, artifact, version, fallback in [("webgl1", "webgl1", 1, False), ("webgl2", "webgl2", 2, False),
                                                       ("auto", "webglauto", 2, False), ("auto-fallback", "webglauto", 1, True)]:
                driver.OUTPUT = output / f"{browser}-{name}"
                driver.OUTPUT.mkdir(exist_ok=True)
                wasm = "/" + (output / f"{artifact}.wasm").relative_to(ROOT).as_posix()
                url = f"http://127.0.0.1:{server.server_port}/tests/browser_capacity/case.html?" + urllib.parse.urlencode({"wasm": wasm, "block2": "1" if fallback else "0"})
                driver.check_browser(browser, url, marker="browser capacity regression: passed", check_ui=False, timeout=30, force_dpr1=True)
                result = json.loads((driver.OUTPUT / f"{browser}.json").read_text())
                validate(result, version)
                print(f"{browser} {name} (WebGL{version}): upload bounds, exact fits, complete triangle/line/material pixels passed", flush=True)
    finally:
        server.shutdown()
        server.server_close()
    if source_identity() != source:
        raise RuntimeError("Source changed during capacity validation")


if __name__ == "__main__":
    main()
