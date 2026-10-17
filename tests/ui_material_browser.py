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
import sys
import threading
import time
import urllib.request
import urllib.parse
sys.dont_write_bytecode = True
from browser_process import webdriver_process

ROOT = Path(__file__).resolve().parents[1]
OUTPUT = ROOT / "target" / "ui-material-browser"
OUTPUT.mkdir(parents=True, exist_ok=True)
EVIDENCE_SCRIPT = "return window.evidence ? JSON.stringify({...window.evidence, visibility: document.visibilityState}) : null"


def request(port, method, path, data=None, timeout=30):
    req = urllib.request.Request(
        f"http://127.0.0.1:{port}{path}",
        data=None if data is None else json.dumps(data).encode(),
        method=method,
        headers={"Content-Type": "application/json"},
    )
    with urllib.request.urlopen(req, timeout=timeout) as response:
        result = json.load(response)["value"]
    if isinstance(result, dict) and result.get("error"):
        raise RuntimeError(result)
    return result


class Handler(http.server.SimpleHTTPRequestHandler):
    def log_message(self, *args):
        pass


def validate_audio_unlock(result, gesture_sent):
    audio = result.get("audioUnlock") or {}
    if not gesture_sent or not audio.get("trustedGesture"):
        raise AssertionError("Audio fixture completed before the trusted unlock gesture")
    if not (audio.get("suspendedBeforeGesture") and audio.get("runningAfterTrustedGesture")
            and audio.get("state") == "running"
            and 0 < audio.get("gestureSequence", 0) < audio.get("resumeSequence", 0) < audio.get("runningSequence", 0)):
        raise AssertionError("Audio context did not resume to running after the trusted gesture")


def check_browser(browser, url, marker="UI material regression passed", check_ui=True, unlock_audio=False, timeout=30, force_dpr1=False, exercise=None, expected_failure=None, headed=False):
    driver_env = os.environ.copy()
    if browser == "chrome":
        binary = os.environ.get("CHROME_BIN") or shutil.which("google-chrome") or shutil.which("chromium")
        options = {"browserName": "chrome", "goog:chromeOptions": {
            "binary": binary,
            "args": ["--headless=new", "--no-sandbox", "--disable-dev-shm-usage",
                     "--use-gl=angle", "--use-angle=swiftshader", "--enable-unsafe-swiftshader"],
        }}
        if headed:
            options["goog:chromeOptions"]["args"].remove("--headless=new")
        if force_dpr1:
            options["goog:chromeOptions"]["args"].append("--force-device-scale-factor=1")
        command = [os.environ.get("CHROMEDRIVER_BIN", "chromedriver"), "--port=0"]
    else:
        # Set X11 on the driver process so Firefox inherits it even when the
        # invoking desktop session uses Wayland.
        driver_env.update({"MOZ_ENABLE_WAYLAND": "0", "GDK_BACKEND": "x11"})
        options = {"browserName": "firefox", "moz:firefoxOptions": {
            "binary": os.environ.get("FIREFOX_BIN") or shutil.which("firefox"),
            "prefs": {"webgl.force-enabled": True, "webgl.disabled": False},
        }}
        if force_dpr1:
            options["moz:firefoxOptions"]["prefs"]["layout.css.devPixelsPerPx"] = "1.0"
            # OS text scaling otherwise multiplies devPixelsPerPx in Firefox.
            options["moz:firefoxOptions"]["prefs"]["browser.display.os-zoom-behavior"] = 0
        command = [os.environ.get("GECKODRIVER_BIN", "geckodriver"), "--port", "0"]
    session = None
    with webdriver_process(browser, command, OUTPUT / f"{browser}-driver.log", request, env=driver_env) as port:
        try:
            result = request(port, "POST", "/session", {"capabilities": {"alwaysMatch": options}})
            session = result["sessionId"]
            request(port, "POST", f"/session/{session}/timeouts", {"script": 15000, "pageLoad": 15000, "implicit": 0})
            if force_dpr1:
                request(port, "POST", f"/session/{session}/window/rect", {"width": 800, "height": 650})
            request(port, "POST", f"/session/{session}/url", {"url": url})
            if exercise:
                exercise(port, session)
            deadline = time.monotonic() + timeout
            unlocked = False
            while True:
                serialized = request(port, "POST", f"/session/{session}/execute/sync", {
                    "script": EVIDENCE_SCRIPT, "args": [],
                })
                # WebDriver can serialize returned object properties at different
                # times. Serialize inside the page's single event-loop task.
                result = json.loads(serialized) if serialized is not None else None
                marked = result and any(marker in m for m in result["messages"])
                completed = result and result.get("completed") is True
                if unlock_audio and (marked or completed):
                    (OUTPUT / f"{browser}.json").write_text(json.dumps(result, indent=2) + "\n")
                    validate_audio_unlock(result, unlocked)
                if unlock_audio and not completed and result and result.get("audioUnlock", {}).get("ready") and not unlocked:
                    request(port, "POST", f"/session/{session}/actions", {"actions": [{
                        "type": "pointer", "id": "audio-unlock", "parameters": {"pointerType": "mouse"},
                        "actions": [{"type": "pointerMove", "x": 20, "y": 20, "duration": 0},
                                    {"type": "pointerDown", "button": 0}, {"type": "pointerUp", "button": 0}],
                    }]})
                    unlocked = True
                if result and (result["errors"] or completed or (expected_failure and expected_failure(result))):
                    break
                if time.monotonic() >= deadline:
                    (OUTPUT / f"{browser}.json").write_text(json.dumps(result, indent=2) + "\n")
                    raise RuntimeError(f"{browser} regression timed out: {result}")
                time.sleep(0.1)
            (OUTPUT / f"{browser}.json").write_text(json.dumps(result, indent=2) + "\n")
            if result["errors"] or (not expected_failure and not (completed and marked)):
                raise AssertionError(result)
            if expected_failure and not expected_failure(result):
                raise AssertionError(result)
            if check_ui and (result["phases"][0]["uiShaders"] != 0 or result["uiShaders"] != 1):
                raise AssertionError(result)
            if unlock_audio:
                validate_audio_unlock(result, unlocked)
            outcome = "expected startup failure observed" if expected_failure else "passed"
            print(f"{browser}: {outcome}; {marker}", flush=True)
        finally:
            if session:
                with contextlib.suppress(Exception):
                    request(port, "DELETE", f"/session/{session}", timeout=3)


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
