"""Reject incomplete/contradictory scene evidence independently of browser timing."""
import copy
import sys
import unittest

sys.dont_write_bytecode = True
from browser import validate, validate_submissions


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


class SubmissionProtocol(unittest.TestCase):
    def fixture(self):
        order = [(0, "camera"), (0, "screen"), (1, "screen"), (1, "camera"), (2, "camera"), (2, "screen")]
        return {"submissions": {"frames": [{"trial": trial, "variant": variant, "counts": {
            "vertexUploadBytes": 327840, "indexUploadBytes": 24588, "bufferDataBytes": 0,
            "submittedIndices": 12294, "submittedVertices": 0,
            # Camera frames flush once per label; screen frames share uploads.
            "calls": {"drawElementsInstanced": 37 if variant == "camera" else 6,
                      "bufferSubData": 74 if variant == "camera" else 4,
                      "bindFramebuffer": 76 if variant == "camera" else 6},
        }} for trial, variant in order]}}

    def test_complete_matched_work_is_accepted(self):
        validate_submissions(self.fixture())
        plain = self.fixture()
        for frame in plain["submissions"]["frames"]:
            calls = frame["counts"]["calls"]
            calls["drawElements"] = calls.pop("drawElementsInstanced")
        validate_submissions(plain)

    def test_incomplete_work_calls_and_resource_churn_are_rejected(self):
        mutations = [
            lambda r: r["submissions"]["frames"].pop(),
            lambda r: r["submissions"]["frames"].reverse(),
            lambda r: r["submissions"]["frames"][0]["counts"].update(vertexUploadBytes=327680),
            lambda r: r["submissions"]["frames"][0]["counts"].update(indexUploadBytes=24576),
            lambda r: r["submissions"]["frames"][0]["counts"].update(submittedIndices=12288),
            lambda r: r["submissions"]["frames"][0]["counts"].update(bufferDataBytes=1024),
            lambda r: r["submissions"]["frames"][0]["counts"].pop("bufferDataBytes"),
            lambda r: r["submissions"]["frames"][0]["counts"].update(submittedVertices=False),
            lambda r: r["submissions"]["frames"][0]["counts"]["calls"].update(drawElementsInstanced=0),
            lambda r: r["submissions"]["frames"][0]["counts"]["calls"].update(bufferSubData=3),
            lambda r: r["submissions"]["frames"][0]["counts"]["calls"].update(bufferSubData=0),
            lambda r: r["submissions"]["frames"][0]["counts"]["calls"].update(bufferSubData=76),
            lambda r: r["submissions"]["frames"][0]["counts"]["calls"].update(bindFramebuffer=0),
            lambda r: r["submissions"]["frames"][1]["counts"]["calls"].update(bindFramebuffer=1),
            lambda r: r["submissions"]["frames"][0]["counts"]["calls"].update(createTexture=1),
            lambda r: r["submissions"]["frames"][0]["counts"]["calls"].update(bufferData=True),
        ]
        for index, mutate in enumerate(mutations):
            with self.subTest(mutation=index):
                result = self.fixture()
                mutate(result)
                with self.assertRaises(AssertionError):
                    validate_submissions(result)


if __name__ == "__main__":
    unittest.main()
