"""Reject incomplete/contradictory scene evidence independently of browser timing."""
import copy
import sys
import unittest

sys.dont_write_bytecode = True
from browser import validate


def complete_result():
    order = [(0, "camera"), (0, "screen"), (1, "screen"), (1, "camera"), (2, "camera"), (2, "screen")]
    return {
        "errors": [], "completed": True,
        "context": {"requested": 1, "actual": 1, "version": "WebGL 1.0 test", "width": 640, "height": 480},
        "viewport": {"width": 640, "height": 480, "devicePixelRatio": 1},
        "gpuTimingMs": None, "timing": {"diagnosticOnly": True},
        "baselineTextures": 5, "finalTextures": 5, "shutdownTextures": 0,
        "baselineOwnedTextures": 0, "finalOwnedTextures": 0,
        "trials": [{"trial": trial, "variant": variant, "cpuSceneApiMs": [0.5] * 18,
                    "rafIntervalsMs": [16.7] * 18, "visibility": ["visible"] * 19} for trial, variant in order],
        # Pixel values are checked by the independent JS oracle, not this protocol validator.
        "pictures": [{"trial": trial, "variant": variant, "equivalent": True, "glError": 0,
                      "witnesses": [{}] * 14, "rgbaSha256": "a" * 64} for trial, variant in order],
    }


class SceneProtocol(unittest.TestCase):
    def test_complete_result_and_submillisecond_samples_are_accepted_without_speed_threshold(self):
        result = complete_result()
        validate(result, 1)
        result["trials"][0]["cpuSceneApiMs"] = [1000] * 18
        validate(result, 1)

    def test_terminal_success_without_all_matched_visual_and_timing_proof_is_rejected(self):
        for field in ("trials", "pictures"):
            with self.subTest(field=field):
                result = complete_result()
                result[field] = []
                with self.assertRaisesRegex(AssertionError, "Missing or reordered"):
                    validate(result, 1)

    def test_wrong_context_and_incomplete_cleanup_are_rejected(self):
        result = complete_result()
        result["context"]["actual"] = 2
        with self.assertRaisesRegex(AssertionError, "context"):
            validate(result, 1)
        for field, value in (("finalTextures", 8), ("finalOwnedTextures", 3), ("shutdownTextures", None)):
            with self.subTest(field=field):
                result = complete_result()
                result[field] = value
                with self.assertRaisesRegex(AssertionError, "cleanup"):
                    validate(result, 1)

    def test_corrupt_timing_backgrounding_and_unequal_pictures_are_rejected(self):
        result = complete_result()
        mutations = [
            lambda r: r["trials"][0].update(cpuSceneApiMs=[]),
            lambda r: r["trials"][0]["cpuSceneApiMs"].__setitem__(0, float("nan")),
            lambda r: r["trials"][0]["visibility"].__setitem__(0, "hidden"),
            lambda r: r["pictures"][0].update(equivalent=False),
            lambda r: r["pictures"][0].update(glError=1282),
            lambda r: r["pictures"][0].update(rgbaSha256="b" * 64),
            lambda r: r.update(gpuTimingMs=0),
            lambda r: r.update(errors=["Unrelated browser failure"]),
        ]
        for index, mutate in enumerate(mutations):
            with self.subTest(mutation=index):
                broken = copy.deepcopy(result)
                mutate(broken)
                with self.assertRaises(AssertionError):
                    validate(broken, 1)


if __name__ == "__main__":
    unittest.main()
