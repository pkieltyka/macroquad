"""Fault controls for the public policy evidence contract."""
import copy
import importlib.util
import math
from pathlib import Path
import unittest
import sys

sys.dont_write_bytecode = True

spec = importlib.util.spec_from_file_location("capacity_policy", Path(__file__).parent / "browser_capacity/policy_browser.py")
policy = importlib.util.module_from_spec(spec)
spec.loader.exec_module(policy)


def draw(count=6, vertices=4):
    return dict(mode=4, count=count, vertexCount=vertices, maxIndex=vertices-1,
                vertexBytes=vertices*40, indexBytes=count*2)


def evidence(profile="debug"):
    phases = []
    if profile == "release":
        phases.append(dict(name="policy-reject", draws=[draw(), draw(12, 8)]))
    for name, segments, degrees in [("default", 834, 1177), ("wrap", 17001, 24001)]:
        # Valid chunking within the effective WebGL2 vertex domain.
        sizes = [833, 1] if name == "default" else [16383, 618]
        phases.append(dict(name="policy-arc-"+name, segments=segments, arcDegrees=degrees,
                           draws=[draw(n*6, n*4) for n in sizes], sourceIndices=segments*6,
                           maxPositionError=0.001, boundaryPixels=0))
    phases += [dict(name="policy-tiny", draws=[draw(3, 3) for _ in range(6)]),
               dict(name="policy-setter", draws=[draw() for _ in range(3)])]
    for phase in phases:
        phase.update(completePixels=True, glError=0)
    return dict(errors=[], completed=True, context=dict(version="WebGL 2.0", attributes=dict(antialias=False),
                requests=[dict(attributes=dict(antialias=False))], samples=0, sampleBuffers=0),
                resources=dict(createdBuffers=2, liveBuffers=0), phases=phases,
                messages=["submitted 9 vertices/3 indices, effective capacity 8 vertices/12 indices. Entire call rejected; submit smaller meshes and use valid local indices"] if profile == "release" else [])


class PolicyContract(unittest.TestCase):
    def test_fixture_has_unambiguous_pixel_centers(self):
        distances = [math.hypot(x+0.5-32, y+0.5-32) for x in range(64) for y in range(64)]
        self.assertGreater(min(abs(r-boundary) for r in distances for boundary in [12.25, 20.1875]), 0.068)

    def test_valid_debug_and_release(self):
        for profile in ["debug", "release"]:
            policy.validate(evidence(profile), 2, profile)

    def test_fault_controls(self):
        mutations = [
            lambda r: r["errors"].append("console failure"),
            lambda r: r.update(completed=False),
            lambda r: r["resources"].update(liveBuffers=1),
            lambda r: r["phases"][0].update(completePixels=False),
            lambda r: r["phases"][0].update(sourceIndices=6),
            lambda r: r["phases"][0].update(maxPositionError=1),
            lambda r: r["phases"][0].update(boundaryPixels=4096),
            lambda r: r["phases"][0]["draws"][0].update(vertexCount=10001),
            lambda r: r["phases"][0]["draws"][0].update(indexBytes=0),
            lambda r: r["phases"][2]["draws"].pop(),
            lambda r: r["phases"][3]["draws"].pop(),
        ]
        for mutate in mutations:
            result = copy.deepcopy(evidence())
            mutate(result)
            with self.subTest(mutate=mutate), self.assertRaises(AssertionError):
                policy.validate(result, 2, "debug")

    def test_release_diagnostic_once_and_specific(self):
        for messages in [[], ["Entire call rejected"], evidence("release")["messages"] * 2]:
            result = evidence("release")
            result["messages"] = messages
            with self.assertRaises(AssertionError):
                policy.validate(result, 2, "release")


if __name__ == "__main__":
    unittest.main()
