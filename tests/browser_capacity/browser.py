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


def validate(result, version, antialias=False):
    if (
        not result.get("context", {})
        .get("version", "")
        .startswith(f"WebGL {version}.0")
    ):
        raise AssertionError("Actual WebGL version differs from requested path")
    context = result["context"]
    for request in context["requests"]:
        if request["attributes"] != {"antialias": antialias}:
            raise AssertionError(f"Ignored antialias request: {request}")
    attributes = context["attributes"]
    if not antialias and (
        attributes["antialias"]
        or context["samples"] != 0
        or context["sampleBuffers"] != 0
    ):
        raise AssertionError(f"Antialias=false was not honored: {context}")
    if (
        not attributes["alpha"]
        or not attributes["premultipliedAlpha"]
        or not attributes["depth"]
        or attributes["stencil"]
        or attributes["preserveDrawingBuffer"]
    ):
        raise AssertionError(f"Unrelated context attributes changed: {attributes}")
    resources = result.get("resources", {})
    if resources.get("createdBuffers", 0) <= 0 or resources.get("liveBuffers") != 0:
        raise AssertionError(f"Buffer resources not cleaned up: {resources}")
    expected = {
        "triangles": [(12, 8, 7), (6, 4, 3)],
        "lines": [(4, 4, 3), (2, 2, 1)],
        "u16-domain": (
            [(12, 65_536, 65_535), (6, 4, 3)]
            if version == 1
            else [(6, 65_532, 65_531), (12, 8, 7)]
        ),
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
        if [(d["count"], d["vertexCount"], d["maxIndex"]) for d in draws] != expected[
            phase["name"]
        ]:
            raise AssertionError(phase)
        if not phase.get("completePixels") or phase.get("glError") != 0:
            raise AssertionError(phase)
        for draw in draws:
            if (
                draw["mode"] != (1 if phase["name"] == "lines" else 4)
                or draw["vertexBytes"] != draw["vertexCount"] * 40
                or draw["indexBytes"] != draw["count"] * 2
            ):
                raise AssertionError(draw)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--output", type=Path, default=ROOT / "target/browser-capacity")
    parser.add_argument(
        "--attributes",
        action="store_true",
        help="Also test sample_count=4 across all context modes",
    )
    parser.add_argument(
        "--baseline",
        action="store_true",
        help="Run only Chrome WebGL1 sample_count=1 for fail-before-fix evidence",
    )
    args = parser.parse_args()
    output = args.output.resolve()
    output.relative_to(ROOT)
    output.mkdir(parents=True, exist_ok=True)
    source = source_identity()
    env = os.environ.copy()
    env["CARGO_TARGET_DIR"] = str(ROOT / "target/browser-capacity-build")
    env["CARGO_TARGET_WASM32_UNKNOWN_UNKNOWN_RUSTFLAGS"] = (
        env.get("CARGO_TARGET_WASM32_UNKNOWN_UNKNOWN_RUSTFLAGS", "")
        + " -C link-arg=--allow-undefined"
    )
    artifacts = {}
    for sample_count in ([1, 4] if args.attributes else [1]):
        for version in ([1] if args.baseline else [1, 2, "auto"]):
            features = (
                ["webgl2"] if version == 2 else ["auto"] if version == "auto" else []
            ) + (["antialias4"] if sample_count == 4 else [])
            command = [
                "cargo",
                "build",
                "--manifest-path",
                str(ROOT / "tests/browser_capacity/Cargo.toml"),
                "--bin",
                "browser-capacity-regression",
                "--target",
                "wasm32-unknown-unknown",
            ]
            subprocess.run(
                command + (["--features", ",".join(features)] if features else []),
                env=env,
                cwd=ROOT,
                check=True,
            )
            name = f"webgl{version}" + ("-aa4" if sample_count == 4 else "")
            shutil.copyfile(
                Path(env["CARGO_TARGET_DIR"])
                / "wasm32-unknown-unknown/debug/browser-capacity-regression.wasm",
                output / f"{name}.wasm",
            )
            artifacts[name] = sha256(output / f"{name}.wasm")
            subprocess.run(
                [
                    "node",
                    "tools/check_browser_runtime.mjs",
                    "--wasm",
                    str(output / f"{name}.wasm"),
                    "--no-audio",
                ],
                cwd=ROOT,
                check=True,
            )
    tool_commands = {
        "node": ["node", "--version"],
        "chrome": [
            os.environ.get("CHROME_BIN")
            or shutil.which("google-chrome")
            or shutil.which("chromium"),
            "--version",
        ],
        "chromedriver": [
            os.environ.get("CHROMEDRIVER_BIN", "chromedriver"),
            "--version",
        ],
        "firefox": [
            os.environ.get("FIREFOX_BIN") or shutil.which("firefox"),
            "--version",
        ],
        "geckodriver": [os.environ.get("GECKODRIVER_BIN", "geckodriver"), "--version"],
    }
    metadata = {
        "source": source,
        "profile": "debug",
        "tools": {
            name: subprocess.check_output(command, stderr=subprocess.STDOUT)
            .decode()
            .splitlines()[0]
            for name, command in tool_commands.items()
        },
        "lockSha256": sha256(ROOT / "tests/browser_capacity/Cargo.lock"),
        "runtimeSha256": sha256(ROOT / "js/mq_js_bundle.js"),
        "wasmSha256": artifacts,
        "rustc": subprocess.check_output(["rustc", "--version"]).decode().strip(),
    }
    (output / "build.json").write_text(json.dumps(metadata, indent=2) + "\n")
    server = http.server.ThreadingHTTPServer(
        ("127.0.0.1", 0), functools.partial(driver.Handler, directory=str(ROOT))
    )
    threading.Thread(target=server.serve_forever, daemon=True).start()
    try:
        for browser in (["chrome"] if args.baseline else ["chrome", "firefox"]):
            cases = (
                [("webgl1", "webgl1", 1, False)]
                if args.baseline
                else [
                    ("webgl1", "webgl1", 1, False),
                    ("webgl2", "webgl2", 2, False),
                    ("auto", "webglauto", 2, False),
                    ("auto-fallback", "webglauto", 1, True),
                ]
            )
            for sample_count in ([1, 4] if args.attributes else [1]):
                for name, artifact, version, fallback in cases:
                    suffix = "-aa4" if sample_count == 4 else ""
                    driver.OUTPUT = output / f"{browser}-{name}{suffix}"
                    driver.OUTPUT.mkdir(exist_ok=True)
                    wasm = (
                        "/"
                        + (output / f"{artifact}{suffix}.wasm")
                        .relative_to(ROOT)
                        .as_posix()
                    )
                    url = (
                        f"http://127.0.0.1:{server.server_port}/tests/browser_capacity/case.html?"
                        + urllib.parse.urlencode(
                            {"wasm": wasm, "block2": "1" if fallback else "0"}
                        )
                    )
                    driver.check_browser(
                        browser,
                        url,
                        marker="browser capacity regression: passed",
                        check_ui=False,
                        timeout=30,
                        force_dpr1=True,
                    )
                    result = json.loads((driver.OUTPUT / f"{browser}.json").read_text())
                    validate(result, version, sample_count > 1)
                    context = result["context"]
                    support = (
                        "provided"
                        if context["attributes"]["antialias"]
                        else "unavailable" if sample_count > 1 else "disabled"
                    )
                    print(
                        f"{browser} {name} sample_count={sample_count} (WebGL{version}): complete pixels/bounds; antialias {support}, SAMPLES={context['samples']}",
                        flush=True,
                    )
            if args.attributes:
                driver.OUTPUT = output / f"{browser}-old-v1"
                driver.OUTPUT.mkdir(exist_ok=True)
                wasm = "/" + (output / "webgl1.wasm").relative_to(ROOT).as_posix()
                url = (
                    f"http://127.0.0.1:{server.server_port}/tests/browser_capacity/case.html?"
                    + urllib.parse.urlencode({"wasm": wasm, "selectionVersion": "1"})
                )
                diagnostic = "This fork requires the paired WebGL selection/antialias extension v2"
                try:
                    driver.check_browser(
                        browser, url, marker=diagnostic, check_ui=False, force_dpr1=True
                    )
                except AssertionError:
                    pass
                result = json.loads((driver.OUTPUT / f"{browser}.json").read_text())
                assert (
                    diagnostic in result["messages"]
                    and any(
                        "RuntimeError: unreachable" in message
                        for message in result["messages"]
                    )
                    and not result["completed"]
                ), result
                assert (
                    result["contextRequests"] == []
                    and result["resources"]["createdBuffers"] == 0
                ), result
                print(
                    f"{browser} labelled old-v1: clear failure before context/GPU creation",
                    flush=True,
                )
    finally:
        server.shutdown()
        server.server_close()
    if source_identity() != source:
        raise RuntimeError("Source changed during capacity validation")


if __name__ == "__main__":
    main()
