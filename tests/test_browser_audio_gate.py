"""Exercise the actual WebDriver runner's terminal audio evidence decisions."""
import importlib.util
from pathlib import Path
import sys
import tempfile
import unittest
from unittest.mock import patch

sys.dont_write_bytecode = True
SPEC = importlib.util.spec_from_file_location("browser_runner", Path(__file__).with_name("ui_material_browser.py"))
runner = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(runner)
MARKER = "audio lifecycle regression: passed"


class AudioRunner(unittest.TestCase):
    def run_browser(self, evidence):
        actions = []
        pending = iter(evidence)

        def request(_port, method, path, data=None):
            if path == "/session":
                return {"sessionId": "fixture"}
            if path.endswith("/execute/sync"):
                return next(pending)
            if path.endswith("/actions"):
                actions.append(data)
            return {}

        with tempfile.TemporaryDirectory(prefix="macroquad-audio-runner-") as directory:
            with patch.object(runner, "OUTPUT", Path(directory)), patch.object(runner, "request", request), \
                    patch.object(runner.subprocess, "Popen"), patch.object(runner.time, "sleep"):
                try:
                    runner.check_browser("chrome", "http://fixture.invalid", marker=MARKER,
                                         check_ui=False, unlock_audio=True)
                finally:
                    self.actions = actions

    @staticmethod
    def evidence(complete=False, running=True):
        return {"messages": [MARKER] if complete else [], "errors": [], "audioInitialized": True,
                "audioUnlock": {"ready": True, "trustedGesture": complete,
                                "suspendedBeforeGesture": complete, "runningAfterTrustedGesture": complete and running,
                                "state": "running" if complete and running else "suspended",
                                "gestureSequence": 2, "resumeSequence": 3, "runningSequence": 4 if running else 0}}

    def test_preexisting_completion_is_rejected_before_sending_a_gesture(self):
        with self.assertRaisesRegex(AssertionError, "before the trusted unlock gesture"):
            self.run_browser([self.evidence(complete=True)])
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
