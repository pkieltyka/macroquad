#!/usr/bin/env python3
"""Prepare deterministic GitHub release assets; never push, tag or publish.

Run after committing reviewed sources. --preview emits runtime assets/notes with
an explicitly unpublishable identity for review of an uncommitted checkout.
"""
import argparse
import gzip
import hashlib
import io
import json
from pathlib import Path
import subprocess
import tarfile
import tomllib

ROOT = Path(__file__).resolve().parents[1]


def git(root, *args):
    return subprocess.check_output(["git", "-C", str(root), *args])


def digest(data):
    return hashlib.sha256(data).hexdigest()


def json_bytes(value):
    return (json.dumps(value, indent=2, sort_keys=True) + "\n").encode()


def compress(data):
    stream = io.BytesIO()
    with gzip.GzipFile(filename="", mode="wb", fileobj=stream, mtime=0, compresslevel=9) as archive:
        archive.write(data)
    return stream.getvalue()


def runtime_archive(root, paths):
    stream = io.BytesIO()
    with tarfile.open(fileobj=stream, mode="w", format=tarfile.USTAR_FORMAT) as archive:
        for name in sorted(paths):
            data = (root / name).read_bytes()
            info = tarfile.TarInfo(name)
            info.size = len(data)
            info.mode = 0o644
            archive.addfile(info, io.BytesIO(data))
    return compress(stream.getvalue())


def prepare(root, output, preview=False):
    package = tomllib.loads((root / "Cargo.toml").read_text())["package"]
    version = package["version"]
    tag = f"zubspace-v{version}"
    if not preview:
        if git(root, "status", "--porcelain", "--untracked-files=all").strip():
            raise ValueError("release requires a clean committed checkout (use --preview for review)")
        commit = git(root, "rev-parse", "HEAD").decode().strip()
        tags = git(root, "tag", "--list", tag).decode().strip()
        if tags and git(root, "rev-parse", f"{tag}^{{commit}}").decode().strip() != commit:
            raise ValueError("release tag already identifies a different commit")
    else:
        commit = "UNCOMMITTED-PREVIEW-DO-NOT-PUBLISH"
    if package.get("publish") is not False:
        raise ValueError("GitHub-only package must disable registry publication")
    subprocess.run(["node", str(root / "tools/regenerate_browser_runtime.mjs"), "--check"], check=True)
    provenance = json.loads((root / "js/mq_js_bundle.provenance.json").read_text())
    if provenance["rustPackages"]["macroquad"]["version"] != version or provenance["pairing"]["tag"] != tag:
        raise ValueError("runtime provenance and release identity differ")
    rust_pairing = {name: dict(info) for name, info in provenance["rustPackages"].items()}
    for name in ("macroquad", "miniquad"):
        rust_pairing[name]["source"] = f"git+{package['repository']}?rev={commit}#{commit}"
    paths = [
        "js/mq_js_bundle.js", "js/mq_js_bundle.provenance.json", "js/README.md",
        "vendor/browser-runtime/macroquad-0.4.16.js", "vendor/browser-runtime/LICENSE-MIT",
        "vendor/miniquad/js/gl.js", "vendor/miniquad/LICENSE-MIT", "vendor/miniquad/LICENSE-APACHE",
        "tools/browser_runtime.mjs", "tools/regenerate_browser_runtime.mjs",
        "tools/check_browser_runtime.mjs", "docs/browser-runtime.md", "LICENSE-MIT", "LICENSE-APACHE",
    ]
    if not preview:
        # A clean status alone would overlook a release input ignored by Git.
        git(root, "ls-files", "--error-unmatch", "--", *paths, "vendor/miniquad.patch", f"docs/releases/{tag}.md")
    assets = {
        "mq_js_bundle.js": (root / "js/mq_js_bundle.js").read_bytes(),
        "mq_js_bundle.provenance.json": (root / "js/mq_js_bundle.provenance.json").read_bytes(),
        f"macroquad-{tag}-runtime.tar.gz": runtime_archive(root, paths),
    }
    if not preview:
        assets[f"macroquad-{tag}-source.tar.gz"] = compress(git(root, "archive", "--format=tar", f"--prefix=macroquad-{tag}/", commit))
    manifest = {
        "schemaVersion": 1, "publishable": not preview,
        "repository": package["repository"], "package": package["name"], "version": version,
        "tag": tag, "commit": commit,
        "upstreamMacroquad": {"version": "0.4.16", "revision": "8d602d3f8ebb8a3a582ee15d92983bbb81ee3ef6"},
        "miniquad": {
            "version": "0.4.11", "upstreamRevision": "4f13d4a70caf6a8cbf0187a95461658e7812b566",
            "crateSha256": "dcbaea978d741f3b60ced666b06a2a1e4a2bc81cb8b9dd29f4a40965e1f59e65",
            "patchSha256": digest((root / "vendor/miniquad.patch").read_bytes()),
            "gitTree": None if preview else git(root, "rev-parse", f"{commit}:vendor/miniquad").decode().strip(),
        },
        "runtime": provenance,
        "rustPairing": rust_pairing,
        "runtimeInputs": {name: digest((root / name).read_bytes()) for name in paths},
        "supportedTargets": ["x86_64-unknown-linux-gnu: X11/OpenGL, no default features or audio", "wasm32-unknown-unknown: audio, WebGL1/2"],
        "limitations": ["No official RustSec patched-release or clean-audit claim; ttf-parser advisory remains", "No graphics context restoration or GPU timing support", "No physical GPU/native audio-device coverage claim", "No new game acceptance, whole-frame/FPS or GPU improvement claim; generated CPU API diagnostics and historical game measurements have separate limits"],
        "assets": {name: {"sha256": digest(data), "bytes": len(data)} for name, data in sorted(assets.items())},
    }
    assets["release-manifest.json"] = json_bytes(manifest)
    notes = (root / f"docs/releases/{tag}.md").read_text().replace("@COMMIT@", commit)
    notes += "\nRelease asset SHA-256 (download and verify with `sha256sum -c SHA256SUMS`):\n\n```text\n"
    for name, data in sorted(assets.items()):
        notes += f"{digest(data)}  {name}\n"
    notes += "```\n"
    # Notes are checksummed too; checksum file intentionally excludes itself.
    assets["release-notes.md"] = notes.encode()
    assets["SHA256SUMS"] = "".join(f"{digest(data)}  {name}\n" for name, data in sorted(assets.items())).encode()
    output.mkdir(parents=True, exist_ok=False)
    for name, data in assets.items():
        (output / name).write_bytes(data)
    return manifest


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--output", type=Path, required=True, help="New output directory, normally under target/")
    parser.add_argument("--preview", action="store_true")
    args = parser.parse_args()
    try:
        result = prepare(ROOT, args.output.resolve(), args.preview)
        print(f"Prepared {result['tag']} at {result['commit']}: {args.output}")
    except (ValueError, subprocess.CalledProcessError) as error:
        parser.exit(1, f"release preparation failed: {error}\n")
