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
import ui_material_browser as driver
from browser_scene.browser import source_identity, sha256


def execute(port, session, script, args=None):
    serialized = driver.request(
        port,
        "POST",
        f"/session/{session}/execute/sync",
        {"script": "return JSON.stringify((function(){" + script + "}).apply(null, arguments));", "args": args or []},
    )
    return json.loads(serialized) if serialized is not None else None


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


def settled_event(port, session, event=None):
    script = """
const done = arguments[arguments.length - 1];
const event = arguments[0];
(event ? inputWake(event.type, event.ids, event.touches ?? null) : inputIdle()).then(
    result => done(JSON.stringify(result)),
    error => done(JSON.stringify({error: String(error)})));
"""
    serialized = driver.request(port, "POST", f"/session/{session}/execute/async",
                                {"script": script, "args": [event]}, timeout=5)
    result = json.loads(serialized)
    assert not result.get("error"), result
    return result


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
    idle = settled_event(port, session)
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
        lambda r: not r["flags"] & 1 and len(r["frames"]) > idle["frameCount"]
        and any(e["type"] == "up" for e in r["events"]),
        "owned outside release reaches WASM",
    )
    assert result["scheduler"]["requests"] > idle["requests"], result
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


def focus_returned(result, previous_events):
    events = result["domFocus"][previous_events:]
    return (
        result["focused"] and result["domFocused"] and result["visibility"] == "visible"
        and len(events) >= 2 and len(events) % 2 == 0
        and all(event["trusted"] for event in events)
        and [event["focused"] for event in events] == [False, True] * (len(events) // 2)
    )


def validate_blur_release(result, previous_focus, previous_ups):
    assert (
        result["flags"] & 7 == 0
        and ups(result) == previous_ups + 2
        and result["focus"][previous_focus:][-2:] == ["false", "true"]
    ), result


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

    # Labelled synthetic missing-terminal recovery; touches is authoritative.
    check("syntheticTouch('touchstart',[71]);", True)
    recovered = check("syntheticTouch('touchstart',[72],[72]);", True)
    assert recovered["touches"][-2:] == [{"phase":"Started","id":72},{"phase":"Cancelled","id":71}], recovered
    check("syntheticTouch('touchcancel',[72],[]);", False, 1)
    check("syntheticTouch('touchcancel',[71],[]); syntheticTouch('touchend',[72],[]);", False)
    check("syntheticTouch('touchstart',[81,82]);", True)
    survivor = check("syntheticTouch('touchstart',[83],[82,83,99]);", True)
    assert survivor["touches"][-1] == {"phase":"Cancelled","id":81}, survivor
    check("syntheticTouch('touchcancel',[83],[82,99]);", True)
    check("syntheticTouch('touchcancel',[82],[99]);", False, 1)
    # An unrelated contact in touches is not adopted as a simulated owner.
    check("syntheticTouch('touchmove',[99],[]); syntheticTouch('touchstart',[84],[84]);", True)
    check("syntheticTouch('touchcancel',[84],[]);", False, 1)
    check("syntheticTouch('touchstart',[85,86]);", True)
    ended = check("syntheticTouch('touchend',[85],[]);", False, 1)
    assert ended["touches"][-2:] == [{"phase":"Ended","id":85},{"phase":"Cancelled","id":86}], ended
    check("syntheticMouse('mousedown'); syntheticTouch('touchstart',[91]);", True)
    check("syntheticTouch('touchstart',[92],[92]); syntheticTouch('touchcancel',[92],[]);", True)
    check("syntheticMouse('mouseup',0,window);", False, 1)

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
    focused = wait(port, session, lambda r: r["focused"] and r["domFocused"] and r["visibility"] == "visible", "initial document focus")
    focus_count = len(focused["focus"])
    dom_focus_count = len(focused["domFocus"])
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
        "syntheticKey(document.querySelector('canvas')); syntheticTouch('touchstart',[50]);",
    )
    assert snapshot(port, session)["flags"] & 7 == 7
    other = driver.request(
        port, "POST", f"/session/{session}/window/new", {"type": "tab"}
    )["handle"]
    driver.request(port, "POST", f"/session/{session}/window", {"handle": other})
    driver.request(port, "DELETE", f"/session/{session}/window")
    driver.request(port, "POST", f"/session/{session}/window", {"handle": original})
    wait(port, session, lambda r: focus_returned(r, dom_focus_count), "new trusted DOM focus return")
    # Restored callbacks queue Recorder input but do not wake this blocking loop.
    # Flush only after the DOM transition has delivered the runtime callback.
    result = flush(port, session)
    validate_blur_release(result, focus_count, before)
    released = [e["button"] for e in result["events"] if e["type"] == "up"][-2:]
    # Right terminates first; Left waits for the simulated hold's blur cleanup.
    assert released == [2, 0], result
    actions(
        port,
        session,
        [{"type": "pointerUp", "button": 0}, {"type": "pointerUp", "button": 2}],
    )
    duplicate = flush(port, session)
    assert duplicate["flags"] & 7 == 0 and ups(duplicate) == ups(result), duplicate
    check(
        "syntheticTouch('touchcancel',[50]); syntheticTouch('touchstart',[51]); syntheticMouse('mouseup',0,window);",
        True,
    )
    check("syntheticTouch('touchcancel',[51]);", False, 1)
    check(
        "syntheticKey(document.getElementById('other'));",
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
        "syntheticRawMotion();",
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
        trial = settled_event(port, session, {"type": "touchstart", "ids": [identifier]})
        count = trial["before"]["frameCount"]
        result = wait(
            port,
            session,
            lambda r: len(r["frames"]) > count
            and r["touches"]
            and r["touches"][-1] == {"phase": "Started", "id": identifier},
            "touch-only start wakes real blocking loop",
        )
        assert bool(result["flags"] & 1) == enabled, result
        before = ups(result)
        trial = settled_event(port, session, {"type": "touchcancel", "ids": [identifier]})
        count = trial["before"]["frameCount"]
        result = wait(
            port,
            session,
            lambda r: len(r["frames"]) > count
            and r["touches"][-1] == {"phase": "Cancelled", "id": identifier},
            "touch-only cancellation wakes real blocking loop",
        )
        assert not result["flags"] & 1 and ups(result) == before + int(enabled), result

    # Missing-owner recovery must also wake from a freshly idle blocking loop.
    execute(port, session, "wasm_exports.input_simulate(true);")
    flush(port, session)
    settled_event(port, session, {"type":"touchstart","ids":[63]})
    wait(port, session, lambda r: r["flags"] & 1, "missing-touch initial owner")
    trial = settled_event(port, session, {"type":"touchstart","ids":[64],"touches":[64]})
    recovered = wait(port, session, lambda r: len(r["frames"]) > trial["before"]["frameCount"]
                     and r["touches"][-1] == {"phase":"Cancelled","id":63}, "fresh new touch reconciles stale owner")
    assert recovered["flags"] & 1, recovered
    before = ups(recovered)
    trial = settled_event(port, session, {"type":"touchcancel","ids":[64],"touches":[]})
    result = wait(port, session, lambda r: len(r["frames"]) > trial["before"]["frameCount"]
                  and not r["flags"] & 1 and ups(r)==before+1, "fresh cancellation releases last recovered owner")



def exercise_native_menu(port, session):
    wait(port, session, lambda r: r.get("ready"), "native menu ready")
    execute(port, session, "document.querySelector('canvas').focus();")
    trials = []
    for shift in [False, True]:
        before = snapshot(port, session)
        idle = settled_event(port, session)
        if shift:
            driver.request(port, "POST", f"/session/{session}/actions", {"actions":[{"type":"key","id":"menu-key","actions":[{"type":"keyDown","value":"\ue008"}]}]})
        actions(port, session, [
            {"type":"pointerMove","x":70,"y":80,"duration":0},
            {"type":"pointerDown","button":2}, {"type":"pointerUp","button":2},
        ])
        opened = snapshot(port, session)
        driver.request(port, "POST", f"/session/{session}/actions", {"actions":[{"type":"key","id":"menu-key","actions":[
            {"type":"keyDown","value":"\ue00c"}, {"type":"keyUp","value":"\ue00c"},
        ] + ([{"type":"keyUp","value":"\ue008"}] if shift else [])}]})
        after = flush(port, session)
        audit = after["nativeAudit"][len(before["nativeAudit"]):]
        assert any(e["type"]=="mousedown" and e["button"]==2 and e["trusted"] for e in audit), audit
        assert any(e["type"]=="mouseup" and e["button"]==2 and e["trusted"] for e in audit), audit
        assert all(not e["defaultPrevented"] for e in audit if e["type"]=="contextmenu"), audit
        assert not after["flags"] & 2 and ups(after)==ups(before)+1, after
        assert len(after["frames"]) > idle["frameCount"], after
        trials.append({"shift":shift,"beforeFlags":before["flags"],"openedFlags":opened["flags"],
                       "afterEscapeFlags":after["flags"],"freshFrames":len(after["frames"])-idle["frameCount"],"audit":audit})
    execute(port, session, "evidence.nativeMenuTrials=arguments[0];", [trials])
    print("native menu probe: " + json.dumps(trials), flush=True)


def exercise_lost_touch(port, session):
    wait(port, session, lambda r: r.get("ready"), "lost touch ready")
    execute(port, session, "syntheticTouch('touchstart',[71]);")
    first = flush(port, session)
    assert first["flags"] & 1, first
    # No A terminal is delivered; B's next authoritative touches omits A.
    execute(port, session, "syntheticTouch('touchstart',[72],[72]);")
    second = flush(port, session)
    assert second["flags"] & 1, second
    execute(port, session, "syntheticTouch('touchcancel',[72],[]);")
    final = flush(port, session)
    execute(port, session, "evidence.lostTouchTrial={source:'synthetic DOM',beforeFlags:arguments[0],afterNewStartFlags:arguments[1],afterCancelFlags:arguments[2]};", [first["flags"],second["flags"],final["flags"]])
    assert not final["flags"] & 1, final


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--baseline", action="store_true")
    parser.add_argument("--termination-probe", action="store_true")
    parser.add_argument("--wake-only", action="store_true")
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
    for version in ([1] if args.termination_probe else [1, 2]):
        for touch_wake in ([True] if args.wake_only else [False] if args.baseline or args.termination_probe else [False, True]):
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
            [("chrome", 1), ("firefox", 1)] if args.termination_probe else
            [("chrome", 1)]
            if args.baseline
            else [(b, v) for b in ("chrome", "firefox") for v in (1, 2)]
        )
        for browser, version in cells:
            if args.browser and browser != args.browser:
                continue
            for name, exercise in (
                [("touch-wake", exercise_touch_wake)] if args.wake_only else
                [("native-menu", exercise_native_menu), ("lost-touch", exercise_lost_touch)] if args.termination_probe else
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
                    + urllib.parse.urlencode({"wasm": wasm, "nativeMenu": "1" if name == "native-menu" else "0"})
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
                        headed=name == "native-menu",
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
