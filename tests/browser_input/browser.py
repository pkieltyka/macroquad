#!/usr/bin/env python3
"""Trusted outside-canvas mouse release and labelled synthetic touch cancellation."""

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
import time
import urllib.parse

sys.dont_write_bytecode = True
ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / "tests"))
sys.path.insert(0, str(ROOT / "tests/browser_scene"))
import ui_material_browser as driver
from browser import source_identity, sha256


def execute(port, session, script, args=None):
    return driver.request(
        port,
        "POST",
        f"/session/{session}/execute/sync",
        {"script": script, "args": args or []},
    )


def wait(port, session, predicate, description):
    deadline = time.monotonic() + 5
    while True:
        result = execute(
            port,
            session,
            "return window.evidence.ready ? inputSnapshot() : window.evidence",
        )
        if result.get("errors"):
            raise AssertionError(result)
        if predicate(result):
            return result
        if time.monotonic() > deadline:
            raise AssertionError(f"{description}: {result}")
        time.sleep(0.03)


def actions(port, session, steps):
    driver.request(
        port,
        "POST",
        f"/session/{session}/actions",
        {
            "actions": [
                {
                    "type": "pointer",
                    "id": "owned-mouse",
                    "parameters": {"pointerType": "mouse"},
                    "actions": steps,
                }
            ]
        },
    )


def outside(port, session):
    wait(port, session, lambda r: r.get("ready"), "ready")
    actions(
        port,
        session,
        [
            {"type": "pointerMove", "x": 70, "y": 80, "duration": 0},
            {"type": "pointerDown", "button": 0},
        ],
    )
    wait(port, session, lambda r: r["flags"] & 1, "canvas press")
    actions(
        port,
        session,
        [
            {"type": "pointerMove", "x": 330, "y": 215, "duration": 0},
            {"type": "pointerUp", "button": 0},
        ],
    )
    result = wait(
        port,
        session,
        lambda r: not r["flags"] & 1 and any(e["type"] == "up" for e in r["events"]),
        "owned outside release reaches WASM",
    )
    assert result["trusted"][-1] == {
        "type": "mouseup",
        "trusted": True,
        "onCanvas": False,
    }, result
    assert result["events"][-1] == {
        "type": "up",
        "button": 0,
        "x": 290,
        "y": 175,
    }, result


def cancel(port, session):
    wait(port, session, lambda r: r.get("ready"), "ready")
    before = sum(
        e["type"] == "up"
        for e in execute(port, session, "return inputSnapshot()")["events"]
    )
    execute(port, session, "syntheticTouch('touchstart',[7]);")
    wait(port, session, lambda r: r["flags"] & 1, "synthetic touch press")
    execute(port, session, "syntheticTouch('touchcancel',[7]);")
    result = wait(
        port,
        session,
        lambda r: not r["flags"] & 1
        and sum(e["type"] == "up" for e in r["events"]) > before
        and r["touches"]
        and r["touches"][-1] == {"phase": "Cancelled", "id": 7},
        "synthetic touchcancel reaches terminal WASM mouse state",
    )
    assert result["touches"][-1] == {"phase": "Cancelled", "id": 7}, result


def snapshot(port, session):
    return execute(port, session, "return inputSnapshot()")


def flush(port, session):
    count = len(snapshot(port, session)["frames"])
    execute(port, session, "wasm_exports.input_update();")
    return wait(
        port, session, lambda r: len(r["frames"]) > count, "explicit fixture flush"
    )


def ups(result):
    return sum(e["type"] == "up" for e in result["events"])


def exercise_all(port, session):
    outside(port, session)
    cancel(port, session)

    def check(script, held, delta=0):
        before = ups(snapshot(port, session))
        execute(port, session, script)
        result = flush(port, session)
        assert bool(result["flags"] & 1) == held and ups(result) == before + delta, (
            script,
            result,
        )
        return result

    check(
        "syntheticMouse('mouseup',0,document.getElementById('other')); syntheticMouse('mouseup',0,window);",
        False,
    )
    # Labelled synthetic contacts exercise real DOM → WASM → Macroquad handlers.
    for first, last in [(11, 12), (12, 11)]:
        check("syntheticTouch('touchstart',[11,12]);", True)
        check("syntheticTouch('touchcancel',[99]);", True)
        check(f"syntheticTouch('touchcancel',[{first}]);", True)
        check(f"syntheticTouch('touchcancel',[{last}]);", False, 1)
        check(
            f"syntheticTouch('touchcancel',[{last}]); syntheticTouch('touchend',[{last}]);",
            False,
        )
    check(
        "syntheticTouch('touchstart',[21,22]); syntheticTouch('touchend',[21]);",
        False,
        1,
    )
    check("syntheticTouch('touchcancel',[22]);", False)
    check(
        "syntheticTouch('touchstart',[23]); syntheticTouch('touchend',[23]); syntheticTouch('touchcancel',[23]);",
        False,
        1,
    )
    # Mixed physical/simulated sources defer final up until the held source ends.
    for terminal in ["touchend", "touchcancel"]:
        check("syntheticMouse('mousedown'); syntheticTouch('touchstart',[31]);", True)
        check(f"syntheticTouch('{terminal}',[31]);", True)
        check("syntheticMouse('mouseup',0,window);", False, 1)
        check("syntheticTouch('touchstart',[32]); syntheticMouse('mousedown');", True)
        check("syntheticMouse('mouseup',0,window);", True)
        check(f"syntheticTouch('{terminal}',[32]);", False, 1)
    check(
        "syntheticTouch('touchstart',[40]); wasm_exports.input_simulate(false);",
        False,
        1,
    )
    disabled = check(
        "syntheticTouch('touchcancel',[40]); syntheticTouch('touchstart',[41]); syntheticTouch('touchcancel',[41]);",
        False,
    )
    assert disabled["touches"][-1] == {"phase": "Cancelled", "id": 41}, disabled
    check(
        "wasm_exports.input_simulate(true); syntheticTouch('touchstart',[42]); syntheticMouse('mousedown'); wasm_exports.input_simulate(false);",
        True,
    )
    check(
        "wasm_exports.input_simulate(true); wasm_exports.input_simulate(false); syntheticTouch('touchcancel',[42]);",
        True,
    )
    check("syntheticMouse('mouseup',0,window);", False, 1)
    check(
        "wasm_exports.input_simulate(true); syntheticTouch('touchstart',[43]); syntheticMouse('mousedown'); syntheticMouse('mouseup',0,window); wasm_exports.input_simulate(false);",
        False,
        1,
    )
    check("wasm_exports.input_simulate(true);", False)
    # Canvas ownership survives DOM handlers that stop bubbling.
    check(
        "document.getElementById('other').addEventListener('mouseup',e=>e.stopPropagation(),{once:true}); syntheticMouse('mousedown'); syntheticMouse('mouseup',0,document.getElementById('other'));",
        False,
        1,
    )
    # Real tab focus/visibility transitions, with held mouse and keyboard input.
    execute(port, session, "document.querySelector('canvas').focus();")
    original = driver.request(port, "GET", f"/session/{session}/window")
    before = ups(snapshot(port, session))
    actions(
        port,
        session,
        [
            {"type": "pointerMove", "x": 70, "y": 80, "duration": 0},
            {"type": "pointerDown", "button": 2},
            {"type": "pointerDown", "button": 0},
        ],
    )
    execute(
        port,
        session,
        "document.querySelector('canvas').dispatchEvent(new KeyboardEvent('keydown',{code:'KeyA',bubbles:true})); syntheticTouch('touchstart',[50]);",
    )
    assert snapshot(port, session)["flags"] & 7 == 7
    other = driver.request(
        port, "POST", f"/session/{session}/window/new", {"type": "tab"}
    )["handle"]
    driver.request(port, "POST", f"/session/{session}/window", {"handle": other})
    driver.request(port, "DELETE", f"/session/{session}/window")
    driver.request(port, "POST", f"/session/{session}/window", {"handle": original})
    result = flush(port, session)
    assert (
        result["flags"] & 7 == 0
        and ups(result) == before + 2
        and result["focus"][-2:] == ["false", "true"]
    ), result
    released = [e["button"] for e in result["events"] if e["type"] == "up"][-2:]
    # Right terminates first; Left waits for the simulated hold's blur cleanup.
    assert released == [2, 0], result
    actions(
        port,
        session,
        [{"type": "pointerUp", "button": 0}, {"type": "pointerUp", "button": 2}],
    )
    check(
        "syntheticTouch('touchcancel',[50]); syntheticTouch('touchstart',[51]); syntheticMouse('mouseup',0,window);",
        True,
    )
    check("syntheticTouch('touchcancel',[51]);", False, 1)
    check(
        "document.getElementById('other').dispatchEvent(new KeyboardEvent('keydown',{code:'KeyA',bubbles:true}));",
        False,
    )
    assert not snapshot(port, session)["flags"] & 4
    # Request actual pointer lock from a trusted gesture, then inject a labelled
    # raw-motion vector to check Macroquad's relative-coordinate preservation.
    execute(port, session, "document.querySelector('canvas').focus(); armGrab();")
    actions(
        port,
        session,
        [
            {"type": "pointerMove", "x": 70, "y": 80, "duration": 0},
            {"type": "pointerDown", "button": 0},
        ],
    )
    locked = wait(port, session, lambda r: r["locked"], "actual pointer lock")
    execute(
        port,
        session,
        "const event=new MouseEvent('mousemove',{clientX:600,clientY:600});Object.defineProperties(event,{movementX:{value:7},movementY:{value:-3}});document.querySelector('canvas').dispatchEvent(event);",
    )
    moved = snapshot(port, session)
    assert (moved["x"], moved["y"]) == (locked["x"] + 7, locked["y"] - 3), (
        locked,
        moved,
    )
    before = ups(moved)
    actions(port, session, [{"type": "pointerUp", "button": 0}])
    result = wait(
        port,
        session,
        lambda r: not r["flags"] & 1 and ups(r) == before + 1,
        "pointer-lock release",
    )
    assert (result["x"], result["y"]) == (moved["x"], moved["y"]), result
    execute(port, session, "wasm_exports.input_grab(false);")
    wait(port, session, lambda r: not r["locked"], "pointer lock exit")


def exercise_touch_wake(port, session):
    wait(port, session, lambda r: r.get("ready"), "touch wake ready")
    for enabled, identifier in [(True, 61), (False, 62)]:
        execute(port, session, "wasm_exports.input_simulate(arguments[0]);", [enabled])
        flush(port, session)
        count = len(snapshot(port, session)["frames"])
        execute(
            port, session, "syntheticTouch('touchstart',arguments[0]);", [[identifier]]
        )
        result = wait(
            port,
            session,
            lambda r: len(r["frames"]) > count
            and r["touches"]
            and r["touches"][-1] == {"phase": "Started", "id": identifier},
            "touch-only start wakes real blocking loop",
        )
        assert bool(result["flags"] & 1) == enabled, result
        count = len(result["frames"])
        before = ups(result)
        execute(
            port, session, "syntheticTouch('touchcancel',arguments[0]);", [[identifier]]
        )
        result = wait(
            port,
            session,
            lambda r: len(r["frames"]) > count
            and r["touches"][-1] == {"phase": "Cancelled", "id": identifier},
            "touch-only cancellation wakes real blocking loop",
        )
        assert not result["flags"] & 1 and ups(result) == before + int(enabled), result


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--baseline", action="store_true")
    parser.add_argument("--browser", choices=("chrome", "firefox"))
    parser.add_argument("--output", type=Path, default=ROOT / "target/browser-input")
    args = parser.parse_args()
    output = args.output.resolve()
    output.relative_to(ROOT)
    output.mkdir(parents=True, exist_ok=True)
    source = source_identity()
    env = os.environ.copy()
    env["CARGO_TARGET_DIR"] = str(ROOT / "target/browser-input-build")
    env["CARGO_TARGET_WASM32_UNKNOWN_UNKNOWN_RUSTFLAGS"] = (
        env.get("CARGO_TARGET_WASM32_UNKNOWN_UNKNOWN_RUSTFLAGS", "")
        + " -C link-arg=--allow-undefined"
    )
    artifacts = {}
    for version in (1, 2):
        for touch_wake in ([False] if args.baseline else [False, True]):
            features = (["webgl2"] if version == 2 else []) + (
                ["touch-wake"] if touch_wake else []
            )
            command = [
                "cargo",
                "build",
                "--manifest-path",
                str(ROOT / "tests/browser_input/Cargo.toml"),
                "--target",
                "wasm32-unknown-unknown",
            ] + (["--features", ",".join(features)] if features else [])
            subprocess.run(command, cwd=ROOT, env=env, check=True)
            name = f"webgl{version}" + ("-touch-wake" if touch_wake else "")
            destination = output / f"{name}.wasm"
            shutil.copyfile(
                Path(env["CARGO_TARGET_DIR"])
                / "wasm32-unknown-unknown/debug/browser-input-regression.wasm",
                destination,
            )
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
            artifacts[name] = sha256(destination)
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
    (output / "build.json").write_text(
        json.dumps(
            {
                "source": source,
                "profile": "debug",
                "runtimeSha256": sha256(ROOT / "js/mq_js_bundle.js"),
                "wasmSha256": artifacts,
                "tools": {
                    name: subprocess.check_output(command, stderr=subprocess.STDOUT)
                    .decode()
                    .splitlines()[0]
                    for name, command in tool_commands.items()
                },
                "lockSha256": sha256(ROOT / "tests/browser_input/Cargo.lock"),
                "rustc": subprocess.check_output(["rustc", "--version"])
                .decode()
                .strip(),
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
        cells = (
            [("chrome", 1)]
            if args.baseline
            else [(b, v) for b in ("chrome", "firefox") for v in (1, 2)]
        )
        for browser, version in cells:
            if args.browser and browser != args.browser:
                continue
            for name, exercise in (
                [("outside", outside), ("synthetic-cancel", cancel)]
                if args.baseline
                else [
                    ("owned-input", exercise_all),
                    ("touch-wake", exercise_touch_wake),
                ]
            ):
                driver.OUTPUT = output / f"{browser}-webgl{version}-{name}"
                driver.OUTPUT.mkdir(exist_ok=True)
                artifact = f"webgl{version}" + (
                    "-touch-wake" if name == "touch-wake" else ""
                )
                wasm = "/" + (output / f"{artifact}.wasm").relative_to(ROOT).as_posix()
                url = (
                    f"http://127.0.0.1:{server.server_port}/tests/browser_input/case.html?"
                    + urllib.parse.urlencode({"wasm": wasm})
                )

                def run(port, session):
                    try:
                        exercise(port, session)
                        result = snapshot(port, session)
                        assert result["version"].startswith(
                            f"WebGL {version}.0"
                        ), result
                        execute(port, session, "wasm_exports.input_finish();")
                    except Exception:
                        (driver.OUTPUT / f"{browser}.json").write_text(
                            json.dumps(snapshot(port, session), indent=2) + "\n"
                        )
                        raise

                try:
                    driver.check_browser(
                        browser,
                        url,
                        marker="browser input regression: passed",
                        check_ui=False,
                        force_dpr1=True,
                        exercise=run,
                    )
                except Exception as error:
                    failures.append(f"{browser} WebGL{version} {name}: {error}")
                    print(failures[-1], flush=True)
    finally:
        server.shutdown()
        server.server_close()
    if failures:
        raise AssertionError("\n".join(failures))
    if source_identity() != source:
        raise RuntimeError("Source changed during input validation")


if __name__ == "__main__":
    main()
