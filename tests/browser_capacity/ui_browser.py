#!/usr/bin/env python3
"""Complete patterned/scissored UI pixels at default,4/6 and3/3 capacities."""

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
from browser_scene.browser import source_identity, sha256


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--baseline", action="store_true")
    parser.add_argument("--output", type=Path, default=ROOT / "target/browser-ui")
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
    for version in ([1] if args.baseline else [1, 2]):
        for capacity, feature in [
            ("default", []),
            ("4-6", ["ui-small"]),
            ("3-3", ["ui-tiny"]),
        ]:
            name = f"webgl{version}-{capacity}"
            features = (["webgl2"] if version == 2 else []) + feature
            command = [
                "cargo",
                "build",
                "--manifest-path",
                str(ROOT / "tests/browser_capacity/Cargo.toml"),
                "--bin",
                "browser-ui-regression",
                "--target",
                "wasm32-unknown-unknown",
            ] + (["--features", ",".join(features)] if features else [])
            subprocess.run(command, cwd=ROOT, env=env, check=True)
            destination = output / f"{name}.wasm"
            shutil.copyfile(
                Path(env["CARGO_TARGET_DIR"])
                / "wasm32-unknown-unknown/debug/browser-ui-regression.wasm",
                destination,
            )
            artifacts[name] = sha256(destination)
            subprocess.run(
                [
                    "node",
                    "tools/check_browser_runtime.mjs",
                    "--wasm",
                    str(destination),
                    "--no-audio",
                ],
                cwd=ROOT,
                check=True,
            )
    (output / "build.json").write_text(
        json.dumps(
            {
                "source": source,
                "runtimeSha256": sha256(ROOT / "js/mq_js_bundle.js"),
                "lockSha256": sha256(ROOT / "tests/browser_capacity/Cargo.lock"),
                "tools": {
                    name: subprocess.check_output(command, text=True).strip()
                    for name, command in {
                        "rustc": ["rustc", "-Vv"],
                        "cargo": ["cargo", "-V"],
                        "node": ["node", "--version"],
                        "python": [sys.executable, "--version"],
                        "chrome": [
                            os.environ.get("CHROME_BIN", "google-chrome"),
                            "--version",
                        ],
                        "chromedriver": [
                            os.environ.get("CHROMEDRIVER_BIN", "chromedriver"),
                            "--version",
                        ],
                        "firefox": [
                            os.environ.get("FIREFOX_BIN", "firefox"),
                            "--version",
                        ],
                        "geckodriver": [
                            os.environ.get("GECKODRIVER_BIN", "geckodriver"),
                            "--version",
                        ],
                    }.items()
                },
                "wasmSha256": artifacts,
            },
            indent=2,
        )
        + "\n"
    )
    server = http.server.ThreadingHTTPServer(
        ("127.0.0.1", 0), functools.partial(driver.Handler, directory=str(ROOT))
    )
    threading.Thread(target=server.serve_forever, daemon=True).start()
    failures = []
    try:
        for browser in (["chrome"] if args.baseline else ["chrome", "firefox"]):
            for name in artifacts:
                driver.OUTPUT = output / f"{browser}-{name}"
                driver.OUTPUT.mkdir(exist_ok=True)
                wasm = "/" + (output / f"{name}.wasm").relative_to(ROOT).as_posix()
                url = (
                    f"http://127.0.0.1:{server.server_port}/tests/browser_capacity/ui.html?"
                    + urllib.parse.urlencode({"wasm": wasm})
                )
                try:
                    driver.check_browser(
                        browser,
                        url,
                        marker="browser UI regression: passed",
                        check_ui=False,
                        force_dpr1=True,
                    )
                    result = json.loads((driver.OUTPUT / f"{browser}.json").read_text())
                    assert result["completed"] and result["pictures"] == [
                        {"frame": f, "completePixels": True, "glError": 0}
                        for f in [0, 1]
                    ], result
                    print(
                        f"{browser} {name}: complete pixels; {result['context']['version']}",
                        flush=True,
                    )
                except Exception as error:
                    failures.append(f"{browser} {name}: {error}")
                    print(failures[-1], flush=True)
    finally:
        server.shutdown()
        server.server_close()
    if failures:
        raise AssertionError("\n".join(failures))
    if source_identity() != source:
        raise RuntimeError("Source changed during UI validation")


if __name__ == "__main__":
    main()
