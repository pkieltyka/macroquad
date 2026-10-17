"""Bounded ownership of browser-test processes on the supported Linux runner."""
import argparse
import contextlib
import os
from pathlib import Path
import re
import shlex
import signal
import subprocess
import sys
import time


def stop_process_group(process, grace=3):
    """Signal the owned group and reap its leader; descendant delivery is async."""
    deadline = time.monotonic() + grace
    with contextlib.suppress(ProcessLookupError):
        os.killpg(process.pid, signal.SIGTERM)
    while True:
        process.poll()  # Reap the leader even if its descendants outlive it.
        try:
            os.killpg(process.pid, 0)
        except ProcessLookupError:
            break
        if time.monotonic() >= deadline:
            with contextlib.suppress(ProcessLookupError):
                os.killpg(process.pid, signal.SIGKILL)
            break
        time.sleep(0.05)
    process.wait(timeout=3)


# A signalled suite deletes its WebDriver session (<=3s) and stops its own
# driver groups (<=6s) before exiting. Wait that long before SIGKILL so nested
# sessions never outlive the job that owns them.
JOB_CLEANUP_GRACE = 20


@contextlib.contextmanager
def owned_process(command, grace=3, **kwargs):
    process = subprocess.Popen(command, start_new_session=True, **kwargs)
    previous = signal.getsignal(signal.SIGTERM)

    def terminate(_signum, _frame):
        raise SystemExit(128 + signal.SIGTERM)

    signal.signal(signal.SIGTERM, terminate)
    try:
        yield process
    finally:
        try:
            stop_process_group(process, grace)
        finally:
            signal.signal(signal.SIGTERM, previous)


def assigned_port(browser, text):
    pattern = (
        r"ChromeDriver was started successfully on port ([0-9]+)\."
        if browser == "chrome"
        # Require the complete line: a read racing the write could see a prefix.
        else r"Listening on 127\.0\.0\.1:([0-9]+)\r?\n"
    )
    match = re.search(pattern, text)
    if match and 0 < int(match[1]) <= 65535:
        return int(match[1])
    return None


@contextlib.contextmanager
def webdriver_process(browser, command, log_path, request, env=None, startup_timeout=15):
    # Let the OS allocate the listening port atomically. Never probe a released
    # reservation that another user's driver could acquire before ours binds.
    deadline = time.monotonic() + startup_timeout
    with log_path.open("w") as combined:
        for attempt in range(1, 4):
            ready = False
            attempt_path = log_path.with_name(f"{log_path.stem}-startup-{attempt}.log")
            try:
                with attempt_path.open("w") as log:
                    with owned_process(command, stdout=log, stderr=subprocess.STDOUT, env=env) as process:
                        while True:
                            if process.poll() is not None:
                                raise RuntimeError(f"{browser} driver exited during startup; see {attempt_path}")
                            port = assigned_port(browser, attempt_path.read_text())
                            if port:
                                try:
                                    status = request(port, "GET", "/status", timeout=1)
                                    if status.get("ready") and process.poll() is None:
                                        ready = True
                                        break
                                except OSError:
                                    pass
                            if time.monotonic() >= deadline:
                                raise RuntimeError(f"{browser} driver startup timed out; see {attempt_path}")
                            time.sleep(0.05)
                        # Only startup failures are retried. Once yielded, every
                        # fixture assertion/error propagates without another run.
                        yield port
                        return
            except RuntimeError:
                if ready:
                    # A ready driver has already yielded: do not retry its job.
                    raise
                if attempt == 3 or time.monotonic() >= deadline:
                    raise
            finally:
                combined.write(attempt_path.read_text())
                combined.flush()


def with_window_manager(output, command):
    output.mkdir(parents=True, exist_ok=True)
    ready = output.resolve() / "openbox.ready"
    ready.unlink(missing_ok=True)
    acknowledge = shlex.join([sys.executable, "-c", "from pathlib import Path; import sys; Path(sys.argv[1]).touch()", str(ready)])
    with (output / "openbox.log").open("w") as log:
        with owned_process(["openbox", "--sm-disable", "--startup", acknowledge], stdout=log, stderr=subprocess.STDOUT) as manager:
            deadline = time.monotonic() + 5
            while not ready.exists():
                if manager.poll() is not None or time.monotonic() >= deadline:
                    raise RuntimeError(f"Owned Openbox did not become ready; see {log.name}")
                time.sleep(0.05)
            if manager.poll() is not None:
                raise RuntimeError(f"Owned Openbox exited at startup; see {log.name}")
            env = {**os.environ, "MACROQUAD_BROWSER_WM_PID": str(manager.pid)}
            with owned_process(command, grace=JOB_CLEANUP_GRACE, env=env) as job:
                while job.poll() is None:
                    if manager.poll() is not None:
                        raise RuntimeError(f"Owned Openbox exited during browser checks; see {log.name}")
                    time.sleep(0.1)
                if manager.poll() is not None:
                    raise RuntimeError(f"Owned Openbox exited during browser checks; see {log.name}")
                return job.returncode


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--window-manager", type=Path, required=True)
    parser.add_argument("command", nargs=argparse.REMAINDER)
    args = parser.parse_args()
    command = args.command[1:] if args.command[:1] == ["--"] else args.command
    if not command:
        parser.error("provide a command after --")
    sys.exit(with_window_manager(args.window_manager, command))
