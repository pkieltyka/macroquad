#!/usr/bin/env python3
"""Build/run finite WebGL1/2 texture deletion tests with Chrome and Firefox.

Requires Rust's wasm32 target, Chrome/chromedriver, Firefox/geckodriver, and a
working Firefox graphics display (Xvfb is sufficient). --expect-stale checks
that an unpatched source build actually reproduces the cache failure.
"""
import argparse
import contextlib
import functools
import http.server
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
import threading
import time
import urllib.parse
import urllib.request

ROOT = Path(__file__).resolve().parents[2]
sys.dont_write_bytecode = True
sys.path.insert(0, str(ROOT / "tests"))
from browser_process import webdriver_process


def request(port, method, path, data=None, timeout=30):
    req = urllib.request.Request(
        f"http://127.0.0.1:{port}{path}",
        data=None if data is None else json.dumps(data).encode(), method=method,
        headers={"Content-Type": "application/json"},
    )
    with urllib.request.urlopen(req, timeout=timeout) as response:
        value = json.load(response)["value"]
    if isinstance(value, dict) and value.get("error"):
        raise RuntimeError(value)
    return value


def build(output, selection=False):
    env = os.environ.copy()
    env["CARGO_TARGET_DIR"] = str(ROOT / "target" / "texture-cache-build")
    flags = env.get("CARGO_TARGET_WASM32_UNKNOWN_UNKNOWN_RUSTFLAGS", "")
    env["CARGO_TARGET_WASM32_UNKNOWN_UNKNOWN_RUSTFLAGS"] = flags + " -C link-arg=--allow-undefined"
    command = ["cargo", "build", "--manifest-path", str(ROOT / "tests/texture_cache/Cargo.toml"), "--target", "wasm32-unknown-unknown"]
    variants = [("webgl1", []), ("webgl2", ["--features", "webgl2"])]
    if selection:
        variants.append(("auto", ["--features", "auto"]))
    for name, flags in variants:
        subprocess.run(command + flags, env=env, cwd=ROOT, check=True)
        source = Path(env["CARGO_TARGET_DIR"]) / "wasm32-unknown-unknown/debug/texture-cache-regression.wasm"
        shutil.copyfile(source, output / f"{name}.wasm")


def check(browser, base_url, output, expected_stale, runtime=None, selection=False):
    if browser == "chrome":
        options = {"browserName": "chrome", "goog:chromeOptions": {
            "binary": os.environ.get("CHROME_BIN") or shutil.which("google-chrome") or shutil.which("chromium"),
            "args": ["--headless=new", "--no-sandbox", "--disable-dev-shm-usage", "--use-gl=angle", "--use-angle=swiftshader", "--enable-unsafe-swiftshader"],
        }}
        command = [os.environ.get("CHROMEDRIVER_BIN", "chromedriver"), "--port=0"]
    else:
        options = {"browserName": "firefox", "moz:firefoxOptions": {
            "binary": os.environ.get("FIREFOX_BIN") or shutil.which("firefox"),
            "env": {"MOZ_ENABLE_WAYLAND": "0", "GDK_BACKEND": "x11"},
            "prefs": {"webgl.force-enabled": True},
        }}
        command = [os.environ.get("GECKODRIVER_BIN", "geckodriver"), "--port", "0"]
    session = None
    with webdriver_process(browser, command, output / f"{browser}-driver.log", request) as port:
        try:
            session = request(port, "POST", "/session", {"capabilities": {"alwaysMatch": options}})["sessionId"]
            request(port, "POST", f"/session/{session}/timeouts", {"script": 15000, "pageLoad": 15000, "implicit": 0})
            cases = [("webgl1", "webgl1", 1, None, None), ("webgl2", "webgl2", 2, None, None)]
            if selection:
                cases += [("auto", "auto", 2, None, None), ("fallback", "auto", 1, "webgl2", None),
                          ("unavailable", "auto", None, "all", "Unable to initialize WebGL2 or WebGL1"),
                          ("strict2-unavailable", "webgl2", None, "webgl2", "Unable to initialize required WebGL2"),
                          ("old-runtime", "auto", None, None, "Missing imports: env.webgl_selection_version")]
            for name, artifact, version, blocked, failure in cases:
                wasm = "/" + (output / f"{artifact}.wasm").relative_to(ROOT).as_posix()
                query = {"wasm": wasm}
                if runtime:
                    query["runtime"] = "/" + runtime.resolve().relative_to(ROOT).as_posix()
                if blocked:
                    query["block"] = blocked
                if name == "old-runtime":
                    query["runtime"] = "/tests/fixtures/mq_js_bundle-pre-auto.js"
                url = base_url + "/tests/texture_cache/case.html?" + urllib.parse.urlencode(query)
                request(port, "POST", f"/session/{session}/url", {"url": url})
                deadline = time.monotonic() + 30
                while True:
                    serialized = request(port, "POST", f"/session/{session}/execute/sync", {"script": "return window.evidence ? JSON.stringify(window.evidence) : null", "args": []})
                    evidence = json.loads(serialized) if serialized is not None else None
                    if evidence and (evidence["errors"] or evidence["completed"]):
                        break
                    if time.monotonic() >= deadline:
                        raise RuntimeError(f"{name} timed out: {evidence}")
                    time.sleep(0.1)
                (output / f"{browser}-{name}.json").write_text(json.dumps(evidence, indent=2) + "\n")
                expected_requests = (["webgl"] if name == "webgl1" else ["webgl2", "webgl"]
                                     if name in ("fallback", "unavailable") else [] if name == "old-runtime" else ["webgl2"])
                expected_mode = None if name == "old-runtime" else 0 if artifact == "auto" else 1 if artifact == "webgl1" else 2
                if evidence["requests"] != expected_requests or evidence["requestedMode"] != expected_mode:
                    raise AssertionError(evidence)
                if failure:
                    expected_error = "Error: " + failure
                    if (evidence["startupError"] != expected_error or evidence["completed"] or evidence["version"] is not None
                            or evidence["createdTextures"] or evidence["createdFramebuffers"]
                            or not evidence["errors"] or any(message not in [expected_error, "WASM failed to load, probably incompatible gl.js version"] for message in evidence["errors"])):
                        raise AssertionError(evidence)
                    print(f"{browser} {name}: expected clear failure before GPU resources", flush=True)
                    continue
                if evidence["errors"] or not evidence["completed"] or evidence["version"] != version:
                    raise AssertionError(evidence)
                if bool(evidence["staleBindings"]) != expected_stale:
                    raise AssertionError(evidence)
                if (evidence["liveTextures"] or evidence["liveFramebuffers"] or not evidence["createdTextures"]
                        or not evidence["createdFramebuffers"] or not any(f"WebGL {version}.0" in message
                        and f"MSAA resolves {str(version == 2).lower()}" in message for message in evidence["messages"])):
                    raise AssertionError(evidence)
                print(f"{browser} {name} selected WebGL{version}: correct pixels, GL error 0, stale binds {evidence['staleBindings']}, resources released", flush=True)
        finally:
            if session:
                with contextlib.suppress(Exception):
                    request(port, "DELETE", f"/session/{session}", timeout=3)


class Handler(http.server.SimpleHTTPRequestHandler):
    def log_message(self, *args):
        pass


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--wasm-dir", default="target/texture-cache-web")
    parser.add_argument("--no-build", action="store_true")
    parser.add_argument("--expect-stale", action="store_true")
    parser.add_argument("--selection", action="store_true", help="Exercise Auto, forced fallback, strict failure and old-runtime rejection")
    parser.add_argument("--runtime", type=Path, help="Use the paired aggregate instead of the vendored Miniquad runtime")
    parser.add_argument("--browsers", nargs="+", choices=["chrome", "firefox"], default=["chrome", "firefox"])
    args = parser.parse_args()
    output = (ROOT / args.wasm_dir).resolve()
    output.relative_to(ROOT)
    output.mkdir(parents=True, exist_ok=True)
    if not args.no_build:
        build(output, args.selection)
    server = http.server.ThreadingHTTPServer(("127.0.0.1", 0), functools.partial(Handler, directory=str(ROOT)))
    threading.Thread(target=server.serve_forever, daemon=True).start()
    try:
        for browser in args.browsers:
            check(browser, f"http://127.0.0.1:{server.server_port}", output, args.expect_stale, args.runtime, args.selection)
    finally:
        server.shutdown()
        server.server_close()
