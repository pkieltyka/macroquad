"""Fault controls for owned driver startup, process cleanup and WM health."""
import contextlib
from pathlib import Path
import signal
import sys
import tempfile
import time
import unittest
from unittest.mock import Mock, patch

sys.dont_write_bytecode = True
import browser_process as support


class DriverStartup(unittest.TestCase):
    def test_only_bound_nonzero_ports_from_the_owned_driver_log_are_accepted(self):
        self.assertIsNone(support.assigned_port("chrome", "Starting ChromeDriver on port 0"))
        self.assertIsNone(support.assigned_port("chrome", "ChromeDriver was started successfully on port 0."))
        self.assertIsNone(support.assigned_port("firefox", "Listening on 0.0.0.0:1234"))
        self.assertIsNone(support.assigned_port("firefox", "Listening on 127.0.0.1:65536"))
        self.assertEqual(support.assigned_port("chrome", "ChromeDriver was started successfully on port 43123."), 43123)
        self.assertEqual(support.assigned_port("firefox", "123\tgeckodriver\tINFO\tListening on 127.0.0.1:43124\n"), 43124)
        # A read racing the driver's write must not adopt a truncated port.
        self.assertIsNone(support.assigned_port("firefox", "123\tgeckodriver\tINFO\tListening on 127.0.0.1:4312"))

    def test_exited_startups_retry_at_most_three_times_and_retain_every_log(self):
        @contextlib.contextmanager
        def exited(_command, stdout, **_kwargs):
            stdout.write("driver failed to start\n")
            stdout.flush()
            yield Mock(poll=lambda: 1)

        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "chrome-driver.log"
            with patch.object(support, "owned_process", exited), self.assertRaisesRegex(RuntimeError, "exited during startup"):
                with support.webdriver_process("chrome", ["driver", "--port=0"], path, Mock()) as _port:
                    self.fail("Exited driver yielded")
            self.assertEqual(len(list(Path(directory).glob("*-startup-*.log"))), 3)
            self.assertEqual(path.read_text().count("driver failed to start"), 3)

    def test_ready_driver_fixture_failure_is_never_retried(self):
        started = []

        @contextlib.contextmanager
        def ready(command, stdout, **_kwargs):
            started.append(command)
            stdout.write("ChromeDriver was started successfully on port 43123.\n")
            stdout.flush()
            yield Mock(poll=lambda: None)

        with tempfile.TemporaryDirectory() as directory:
            request = Mock(return_value={"ready": True})
            with patch.object(support, "owned_process", ready), self.assertRaisesRegex(RuntimeError, "fixture failure"):
                with support.webdriver_process("chrome", ["driver", "--port=0"], Path(directory) / "driver.log", request) as port:
                    self.assertEqual(port, 43123)
                    raise RuntimeError("fixture failure")
            self.assertEqual(len(started), 1)
            request.assert_called_once_with(43123, "GET", "/status", timeout=1)

    def test_unbound_or_stalled_startup_never_probes_an_arbitrary_port(self):
        @contextlib.contextmanager
        def unbound(_command, stdout, **_kwargs):
            stdout.write("Starting ChromeDriver on port 0\n")
            stdout.flush()
            yield Mock(poll=lambda: None)

        with tempfile.TemporaryDirectory() as directory:
            request = Mock()
            with patch.object(support, "owned_process", unbound), self.assertRaisesRegex(RuntimeError, "timed out"):
                with support.webdriver_process("chrome", ["driver", "--port=0"], Path(directory) / "driver.log", request, startup_timeout=0):
                    self.fail("Unbound driver yielded")
            request.assert_not_called()


class ProcessOwnership(unittest.TestCase):
    def test_descendant_is_killed_when_its_leader_exits_first(self):
        child_code = "import signal,sys,time; from pathlib import Path; signal.signal(signal.SIGTERM,signal.SIG_IGN); Path(sys.argv[1]).touch(); time.sleep(60)"
        parent_code = "import subprocess,sys,time; from pathlib import Path; p=subprocess.Popen([sys.executable,'-c',sys.argv[1],sys.argv[2]]); Path(sys.argv[3]).write_text(str(p.pid)); time.sleep(60)"
        with tempfile.TemporaryDirectory() as directory:
            ready, pid = Path(directory) / "ready", Path(directory) / "pid"
            with support.owned_process([sys.executable, "-c", parent_code, child_code, str(ready), str(pid)]) as parent:
                deadline = time.monotonic() + 3
                while not ready.exists():
                    self.assertLess(time.monotonic(), deadline, "Child failed to start")
                    time.sleep(0.01)
                child_pid = int(pid.read_text())
                parent.terminate()
                parent.wait(timeout=2)
            state = Path(f"/proc/{child_pid}/stat")
            deadline = time.monotonic() + 1
            while state.exists():
                if state.read_text().split()[2] in ("Z", "X"):
                    break
                self.assertLess(time.monotonic(), deadline, "Owned descendant survived group cleanup")
                time.sleep(0.01)

    def test_signalled_job_gets_its_cleanup_grace_before_kill(self):
        # The job's own cleanup (e.g. deleting nested driver sessions) runs on
        # SIGTERM; the owner must wait for it instead of killing at once.
        code = "import signal,sys,time; from pathlib import Path\ndef stop(*_):\n    time.sleep(0.6); Path(sys.argv[1]).touch(); sys.exit(0)\nsignal.signal(signal.SIGTERM,stop); Path(sys.argv[2]).touch(); time.sleep(60)"
        for grace, cleaned in [(0.1, False), (5, True)]:
            with tempfile.TemporaryDirectory() as directory, self.subTest(grace=grace):
                done, ready = Path(directory) / "done", Path(directory) / "ready"
                with support.owned_process([sys.executable, "-c", code, str(done), str(ready)], grace=grace):
                    deadline = time.monotonic() + 3
                    while not ready.exists():
                        self.assertLess(time.monotonic(), deadline, "Job failed to start")
                        time.sleep(0.01)
                self.assertEqual(done.exists(), cleaned)
        self.assertGreaterEqual(support.JOB_CLEANUP_GRACE, 3 + 2 * 3)

    def test_cleanup_escalates_only_its_owned_group_and_waits_with_a_deadline(self):
        process = Mock(pid=12345)
        with patch.object(support.os, "killpg") as kill, patch.object(support.time, "monotonic", side_effect=[0, 4]):
            support.stop_process_group(process)
        self.assertEqual(kill.call_args_list[0].args, (12345, signal.SIGTERM))
        self.assertEqual(kill.call_args_list[-1].args, (12345, signal.SIGKILL))
        process.wait.assert_called_once_with(timeout=3)


class WindowManager(unittest.TestCase):
    def test_acknowledged_manager_must_remain_alive_during_the_job(self):
        with tempfile.TemporaryDirectory() as directory:
            output = Path(directory)

            @contextlib.contextmanager
            def processes(command, **_kwargs):
                if command[0] == "openbox":
                    (output / "openbox.ready").touch()
                    yield Mock(pid=123, poll=Mock(side_effect=[None, 1]))
                else:
                    yield Mock(poll=lambda: None)

            with patch.object(support, "owned_process", processes), self.assertRaisesRegex(RuntimeError, "exited during browser checks"):
                support.with_window_manager(output, ["fixture-job"])

    def test_live_manager_without_startup_acknowledgement_fails(self):
        @contextlib.contextmanager
        def processes(_command, **_kwargs):
            yield Mock(poll=lambda: None)

        with tempfile.TemporaryDirectory() as directory:
            with patch.object(support, "owned_process", processes), patch.object(support.time, "monotonic", side_effect=[0, 6]), self.assertRaisesRegex(RuntimeError, "did not become ready"):
                support.with_window_manager(Path(directory), ["must-not-run"])


if __name__ == "__main__":
    unittest.main()
