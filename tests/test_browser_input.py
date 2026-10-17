"""Focus sequencing permits real OS flutter while requiring new trusted input."""
import copy
import sys
import unittest

sys.dont_write_bytecode = True
from browser_input.browser import focus_returned, validate_blur_release


def evidence(cycles=1):
    return {
        "focused": True, "domFocused": True, "visibility": "visible", "flags": 0,
        "domFocus": [{"focused": value, "trusted": True} for value in [False, True] * cycles],
        "focus": ["false", "true"] * cycles,
        "events": [{"type": "up", "button": 2}, {"type": "up", "button": 0}],
    }


class FocusEvidence(unittest.TestCase):
    def test_one_or_multiple_new_trusted_focus_cycles_pass_without_extra_releases(self):
        for cycles in [1, 2, 3]:
            with self.subTest(cycles=cycles):
                result = evidence(cycles)
                self.assertTrue(focus_returned(result, 0))
                validate_blur_release(result, 0, 0)

    def test_stale_untrusted_or_incomplete_focus_history_cannot_pass(self):
        original = evidence(2)
        self.assertFalse(focus_returned(original, len(original["domFocus"])))
        for update in [
            {"domFocus": [{"focused": False, "trusted": False}, {"focused": True, "trusted": False}]},
            {"domFocus": original["domFocus"] + [{"focused": False, "trusted": True}]},
            {"domFocus": [{"focused": True, "trusted": True}, {"focused": True, "trusted": True}]},
            {"focused": False}, {"visibility": "hidden"},
        ]:
            with self.subTest(update=update):
                self.assertFalse(focus_returned({**original, **update}, 0))

    def test_repeated_focus_does_not_allow_duplicate_missing_or_stale_rust_releases(self):
        for count in [0, 1, 3, 4]:
            result = copy.deepcopy(evidence(2))
            result["events"] = [{"type": "up"}] * count
            with self.subTest(count=count), self.assertRaises(AssertionError):
                validate_blur_release(result, 0, 0)
        with self.assertRaises(AssertionError):
            validate_blur_release(evidence(2), 4, 0)
        held = evidence(2)
        held["flags"] = 1
        with self.assertRaises(AssertionError):
            validate_blur_release(held, 0, 0)


if __name__ == "__main__":
    unittest.main()
