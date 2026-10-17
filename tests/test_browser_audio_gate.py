"""Exercise immutable WebDriver snapshots and terminal fixture evidence."""
import importlib.util
import contextlib
import json
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest
from unittest.mock import patch

sys.dont_write_bytecode = True
SPEC = importlib.util.spec_from_file_location("browser_runner", Path(__file__).with_name("ui_material_browser.py"))
runner = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(runner)
from browser_capacity.browser import old_v1_failure

MARKER = "audio lifecycle regression: passed"


class BrowserRunner(unittest.TestCase):
    def run_browser(self, evidence, unlock_audio=False, **kwargs):
        actions = []
        pending = iter(evidence)
        self.polls = 0

        def request(_port, method, path, data=None, timeout=30):
            if path == "/session":
                return {"sessionId": "fixture"}
            if path.endswith("/execute/sync"):
                self.polls += 1
                return json.dumps(next(pending))
            if path.endswith("/actions"):
                actions.append(data)
            return {}

        with tempfile.TemporaryDirectory(prefix="macroquad-audio-runner-") as directory:
            with patch.object(runner, "OUTPUT", Path(directory)), patch.object(runner, "request", request), \
                    patch.object(runner, "webdriver_process", return_value=contextlib.nullcontext(12345)), patch.object(runner.time, "sleep"):
                try:
                    runner.check_browser("chrome", "http://fixture.invalid", marker=MARKER,
                                         check_ui=False, unlock_audio=unlock_audio, **kwargs)
                finally:
                    self.actions = actions


class SnapshotRunner(BrowserRunner):
    def test_snapshot_is_coherent_when_webdriver_serializes_after_page_progress(self):
        program = r"""
const vm = require('node:vm');
const script = JSON.parse(require('node:fs').readFileSync(0, 'utf8'));
function delayedSerialize(script) {
    const evidence = {messages:[], phases:[], errors:[], completed:false};
    const context = {window:{evidence}, document:{visibilityState:'visible'}};
    const returned = vm.runInNewContext(`(function(){${script}})()`, context);
    evidence.context = {version:'WebGL 2.0'};
    evidence.phases.push(...Array.from({length:6}, (_, i) => ({name:`phase${i}`})));
    evidence.messages.push('browser capacity regression: passed');
    evidence.completed = true;
    return typeof returned === 'string' ? JSON.parse(returned) : JSON.parse(JSON.stringify(returned));
}
const old = delayedSerialize('return Object.assign({}, window.evidence, {visibility: document.visibilityState})');
const current = delayedSerialize(script);
process.stdout.write(JSON.stringify({old,current}));
"""
        result = subprocess.run(["node", "-e", program], input=json.dumps(runner.EVIDENCE_SCRIPT),
                                text=True, capture_output=True, check=True)
        snapshots = json.loads(result.stdout)
        old = snapshots["old"]
        self.assertFalse(old["completed"])
        self.assertNotIn("context", old)
        self.assertEqual(len(old["phases"]), 6)
        self.assertIn("browser capacity regression: passed", old["messages"])
        self.assertEqual(snapshots["current"], {"messages": [], "phases": [], "errors": [],
                                                "completed": False, "visibility": "visible"})

    def test_marker_without_completion_keeps_polling(self):
        evidence = {"messages": [MARKER], "errors": [], "completed": False}
        self.run_browser([evidence, {**evidence, "completed": True}])
        self.assertEqual(self.polls, 2)

    def test_marker_without_completion_times_out(self):
        with self.assertRaisesRegex(RuntimeError, "regression timed out"):
            self.run_browser([{"messages": [MARKER], "errors": [], "completed": False}], timeout=-1)

    def test_completion_without_marker_is_rejected(self):
        with self.assertRaises(AssertionError):
            self.run_browser([{"messages": [], "errors": [], "completed": True}])

    def test_page_errors_are_rejected_even_with_completed_pass(self):
        with self.assertRaises(AssertionError):
            self.run_browser([{"messages": [MARKER], "errors": ["stage error"], "completed": True}])

    @staticmethod
    def old_v1():
        return {"messages": ["This fork requires the paired WebGL selection/antialias extension v2",
                             "RuntimeError: unreachable"], "errors": [], "completed": False,
                "contextRequests": [], "resources": {"createdBuffers": 0},
                "expectedStartupErrors": ["RuntimeError: unreachable"]}

    def test_old_v1_requires_diagnostic_and_actual_trap_before_gpu_work(self):
        self.run_browser([self.old_v1()], expected_failure=old_v1_failure)
        for field, value in [("messages", [self.old_v1()["messages"][0]]),
                             ("messages", ["RuntimeError: unreachable"]),
                             ("contextRequests", [{"name": "webgl"}]),
                             ("expectedStartupErrors", []),
                             ("expectedStartupErrors", ["unrelated failure"]),
                             ("resources", {"createdBuffers": 1})]:
            with self.subTest(field=field, value=value):
                evidence = {**self.old_v1(), field: value}
                with self.assertRaisesRegex(RuntimeError, "regression timed out"):
                    self.run_browser([evidence], expected_failure=old_v1_failure, timeout=-1)

    def test_old_v1_rejects_stage_errors_and_successful_completion(self):
        for update in [{"errors": ["stage error"]}, {"completed": True}]:
            with self.subTest(update=update), self.assertRaises(AssertionError):
                self.run_browser([{**self.old_v1(), **update}], expected_failure=old_v1_failure)


class AudioRunner(BrowserRunner):
    def run_browser(self, evidence, **kwargs):
        return super().run_browser(evidence, unlock_audio=True, **kwargs)

    @staticmethod
    def evidence(complete=False, running=True):
        return {"messages": [MARKER] if complete else [], "errors": [], "completed": complete, "audioInitialized": True,
                "audioUnlock": {"ready": True, "trustedGesture": complete,
                                "suspendedBeforeGesture": complete, "runningAfterTrustedGesture": complete and running,
                                "state": "running" if complete and running else "suspended",
                                "gestureSequence": 2, "resumeSequence": 3, "runningSequence": 4 if running else 0}}

    def test_preexisting_completion_is_rejected_before_sending_a_gesture(self):
        with self.assertRaisesRegex(AssertionError, "before the trusted unlock gesture"):
            self.run_browser([self.evidence(complete=True)])
        self.assertEqual(self.actions, [])

    def test_early_marker_is_rejected_even_without_completion(self):
        evidence = self.evidence()
        evidence["messages"] = [MARKER]
        with self.assertRaisesRegex(AssertionError, "before the trusted unlock gesture"):
            self.run_browser([evidence])
        self.assertEqual(self.actions, [])

    def test_completion_without_a_running_context_is_rejected(self):
        with self.assertRaisesRegex(AssertionError, "did not resume to running"):
            self.run_browser([self.evidence(), self.evidence(complete=True, running=False)])
        self.assertEqual(len(self.actions), 1)

    def test_completion_with_observed_trusted_resume_is_accepted(self):
        self.run_browser([self.evidence(), self.evidence(complete=True)])
        self.assertEqual(len(self.actions), 1)


if __name__ == "__main__":
    unittest.main()
