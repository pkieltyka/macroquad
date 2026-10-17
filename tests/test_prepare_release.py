"""Exercise release identity, immutable source packaging and reproducibility."""
import importlib.util
import json
from pathlib import Path
import shutil
import subprocess
import sys
import tarfile
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[1]
sys.dont_write_bytecode = True
SPEC = importlib.util.spec_from_file_location("prepare_release", ROOT / "tools/prepare_release.py")
release = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(release)


class ReleaseAssets(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory(prefix="macroquad-release-test-")
        self.root = Path(self.temp.name)
        for name in ["Cargo.toml", "LICENSE-MIT", "LICENSE-APACHE", "js", "tools",
                     "vendor/browser-runtime", "vendor/miniquad.patch", "docs/browser-runtime.md",
                     "docs/releases/zubspace-v0.5.0.md"]:
            source, destination = ROOT / name, self.root / name
            destination.parent.mkdir(parents=True, exist_ok=True)
            if source.is_dir():
                shutil.copytree(source, destination, ignore=shutil.ignore_patterns("__pycache__"))
            else:
                shutil.copyfile(source, destination)
        # Only the tree identity is needed here; actual vendoring has its own gate.
        (self.root / "vendor/miniquad").mkdir()
        (self.root / "vendor/miniquad/fixture").write_text("source identity fixture\n")
        (self.root / ".gitignore").write_text("/target\n__pycache__/\n")
        self.git("init", "-q")
        self.git("add", ".")
        self.git("-c", "user.name=Release Test", "-c", "user.email=test@example.invalid", "commit", "-qm", "fixture")

    def tearDown(self):
        self.temp.cleanup()

    def git(self, *args):
        return release.git(self.root, *args).decode().strip()

    def test_exact_commit_source_and_runtime_assets_are_reproducible(self):
        first, second = self.root / "target/first", self.root / "target/second"
        manifest = release.prepare(self.root, first)
        release.prepare(self.root, second)
        self.assertTrue(manifest["publishable"])
        self.assertEqual(manifest["commit"], self.git("rev-parse", "HEAD"))
        for name in ("macroquad", "miniquad"):
            self.assertTrue(manifest["rustPairing"][name]["source"].endswith(f"#{manifest['commit']}"))
        self.assertEqual(manifest["miniquad"]["gitTree"], self.git("rev-parse", "HEAD:vendor/miniquad"))
        self.assertEqual({p.name: p.read_bytes() for p in first.iterdir()},
                         {p.name: p.read_bytes() for p in second.iterdir()})
        for line in (first / "SHA256SUMS").read_text().splitlines():
            expected, name = line.split("  ")
            self.assertEqual(release.digest((first / name).read_bytes()), expected)
        source = first / f"macroquad-{manifest['tag']}-source.tar.gz"
        with tarfile.open(source) as archive:
            cargo = archive.extractfile(f"macroquad-{manifest['tag']}/Cargo.toml").read()
            self.assertEqual(cargo, (self.root / "Cargo.toml").read_bytes())
        runtime = first / f"macroquad-{manifest['tag']}-runtime.tar.gz"
        with tarfile.open(runtime) as archive:
            self.assertEqual(archive.extractfile("js/mq_js_bundle.js").read(), (self.root / "js/mq_js_bundle.js").read_bytes())
            extracted = self.root / "target/extracted-runtime"
            archive.extractall(extracted, filter="data")
        subprocess.run(["node", str(extracted / "tools/regenerate_browser_runtime.mjs")], check=True)
        self.assertEqual((extracted / "js/mq_js_bundle.js").read_bytes(), (self.root / "js/mq_js_bundle.js").read_bytes())
        self.assertNotIn("@COMMIT@", (first / "release-notes.md").read_text())

    def test_dirty_sources_and_conflicting_tags_cannot_be_released(self):
        (self.root / "unreviewed").write_text("drift")
        with self.assertRaisesRegex(ValueError, "clean committed checkout"):
            release.prepare(self.root, self.root / "target/dirty")
        preview = release.prepare(self.root, self.root / "target/preview", preview=True)
        self.assertFalse(preview["publishable"])
        self.assertFalse(list((self.root / "target/preview").glob("*-source.tar.gz")))
        (self.root / "unreviewed").unlink()
        self.git("tag", "zubspace-v0.5.0")
        self.git("-c", "user.name=Release Test", "-c", "user.email=test@example.invalid", "commit", "--allow-empty", "-qm", "new fixture")
        with self.assertRaisesRegex(ValueError, "different commit"):
            release.prepare(self.root, self.root / "target/conflict")

    def test_modified_runtime_is_rejected_even_in_preview(self):
        with (self.root / "js/mq_js_bundle.js").open("a") as handle:
            handle.write("// altered runtime\n")
        with self.assertRaises(subprocess.CalledProcessError):
            release.prepare(self.root, self.root / "target/drift", preview=True)


if __name__ == "__main__":
    unittest.main()
