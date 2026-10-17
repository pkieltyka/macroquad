#!/usr/bin/env python3
"""Run the finite WASM graphics regression in Chrome and Firefox via WebDriver.

Build examples/ui_material_regression.rs for wasm32-unknown-unknown first.
Put chromedriver, geckodriver, Chrome, and Firefox on PATH. Firefox needs a
working graphics display; an isolated Xvfb display is sufficient.
"""
import argparse
import contextlib
import functools
import http.server
import json
import os
from pathlib import Path
import shutil
import socket
import subprocess
import threading
import time
import urllib.request
import urllib.parse

ROOT = Path(__file__).resolve().parents[1]
OUTPUT = ROOT / "target" / "ui-material-browser"
OUTPUT.mkdir(parents=True, exist_ok=True)


def request(port, method, path, data=None):
    req = urllib.request.Request(
        f"http://127.0.0.1:{port}{path}",
        data=None if data is None else json.dumps(data).encode(),
        method=method,
        headers={"Content-Type": "application/json"},
    )
    with urllib.request.urlopen(req, timeout=30) as response:
        result = json.load(response)["value"]
    if isinstance(result, dict) and result.get("error"):
        raise RuntimeError(result)
    return result


class Handler(http.server.SimpleHTTPRequestHandler):
    def log_message(self, *args):
        pass


def check_browser(browser, url, marker="UI material regression passed", check_ui=True, unlock_audio=False, timeout=30):
    with socket.socket() as sock:
        sock.bind(("127.0.0.1", 0))
        port = sock.getsockname()[1]
    if browser == "chrome":
        binary = os.environ.get("CHROME_BIN") or shutil.which("google-chrome") or shutil.which("chromium")
        options = {"browserName": "chrome", "goog:chromeOptions": {
            "binary": binary,
            "args": ["--headless=new", "--no-sandbox", "--disable-dev-shm-usage",
                     "--use-gl=angle", "--use-angle=swiftshader", "--enable-unsafe-swiftshader"],
        }}
        command = [os.environ.get("CHROMEDRIVER_BIN", "chromedriver"), f"--port={port}"]
    else:
        options = {"browserName": "firefox", "moz:firefoxOptions": {
            "binary": os.environ.get("FIREFOX_BIN") or shutil.which("firefox"),
            "env": {"MOZ_ENABLE_WAYLAND": "0", "GDK_BACKEND": "x11"},
            "prefs": {"webgl.force-enabled": True, "webgl.disabled": False},
        }}
        command = [os.environ.get("GECKODRIVER_BIN", "geckodriver"), "--port", str(port)]
    session = None
    with (OUTPUT / f"{browser}-driver.log").open("w") as log:
        driver = subprocess.Popen(command, stdout=log, stderr=subprocess.STDOUT)
        try:
            deadline = time.monotonic() + 15
            while True:
                try:
                    request(port, "GET", "/status")
                    break
                except OSError:
                    if driver.poll() is not None or time.monotonic() >= deadline:
                        raise RuntimeError(f"{browser} driver did not start; see {log.name}")
                    time.sleep(0.1)
            result = request(port, "POST", "/session", {"capabilities": {"alwaysMatch": options}})
            session = result["sessionId"]
            request(port, "POST", f"/session/{session}/url", {"url": url})
            deadline = time.monotonic() + timeout
            unlocked = False
            while True:
                result = request(port, "POST", f"/session/{session}/execute/sync", {
                    "script": "return window.evidence", "args": [],
                })
                if unlock_audio and result and result.get("audioInitialized") and not unlocked:
                    request(port, "POST", f"/session/{session}/actions", {"actions": [{
                        "type": "pointer", "id": "audio-unlock", "parameters": {"pointerType": "mouse"},
                        "actions": [{"type": "pointerMove", "x": 20, "y": 20, "duration": 0},
                                    {"type": "pointerDown", "button": 0}, {"type": "pointerUp", "button": 0}],
                    }]})
                    unlocked = True
                if result and (result["errors"] or any(marker in m for m in result["messages"])):
                    break
                if time.monotonic() >= deadline:
                    (OUTPUT / f"{browser}.json").write_text(json.dumps(result, indent=2) + "\n")
                    raise RuntimeError(f"{browser} regression timed out: {result}")
                time.sleep(0.1)
            (OUTPUT / f"{browser}.json").write_text(json.dumps(result, indent=2) + "\n")
            if result["errors"] or not any(marker in m for m in result["messages"]):
                raise AssertionError(result)
            if check_ui and (result["phases"][0]["uiShaders"] != 0 or result["uiShaders"] != 1):
                raise AssertionError(result)
            if unlock_audio and not unlocked:
                raise AssertionError("Audio fixture completed before the trusted unlock gesture")
            print(f"{browser}: passed; {marker}", flush=True)
        finally:
            if session:
                with contextlib.suppress(Exception):
                    request(port, "DELETE", f"/session/{session}")
            driver.terminate()
            try:
                driver.wait(timeout=10)
            except subprocess.TimeoutExpired:
                driver.kill()
                driver.wait()


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--fixture", default="tests/ui_material_regression.html")
    parser.add_argument("--marker", default="UI material regression passed")
    parser.add_argument("--runtime", type=Path, help="Paired runtime copied into the fixture's output directory")
    parser.add_argument("--output", type=Path, default=OUTPUT)
    parser.add_argument("--unlock-audio", action="store_true")
    parser.add_argument("--timeout", type=float, default=30, help="Per-browser fixture deadline in seconds")
    args = parser.parse_args()
    OUTPUT = args.output.resolve()
    OUTPUT.mkdir(parents=True, exist_ok=True)
    query = ""
    if args.runtime:
        runtime = OUTPUT / "mq_js_bundle.js"
        shutil.copyfile(args.runtime, runtime)
        query = "?" + urllib.parse.urlencode({"runtime": "/" + runtime.relative_to(ROOT).as_posix()})
    server = http.server.ThreadingHTTPServer(("127.0.0.1", 0), functools.partial(Handler, directory=str(ROOT)))
    threading.Thread(target=server.serve_forever, daemon=True).start()
    try:
        url = f"http://127.0.0.1:{server.server_port}/{args.fixture.lstrip('/')}" + query
        for browser in ["chrome", "firefox"]:
            check_browser(browser, url, marker=args.marker,
                          check_ui=args.fixture == "tests/ui_material_regression.html",
                          unlock_audio=args.unlock_audio, timeout=args.timeout)
    finally:
        server.shutdown()
        server.server_close()
