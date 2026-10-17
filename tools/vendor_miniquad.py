#!/usr/bin/env python3
"""Reconstruct/check the narrow Miniquad patch from its published crate.

Default: compare the tracked vendored source with a fresh reconstruction.
Use --output /new/path to reconstruct it elsewhere for inspection. --crate can
select an existing download instead of fetching the pinned crates.io artifact.
"""
import argparse
import hashlib
from pathlib import Path
import shutil
import subprocess
import tarfile
import tempfile
import urllib.request

ROOT = Path(__file__).resolve().parents[1]
VERSION = "0.4.11"
SHA256 = "dcbaea978d741f3b60ced666b06a2a1e4a2bc81cb8b9dd29f4a40965e1f59e65"
URL = f"https://static.crates.io/crates/miniquad/miniquad-{VERSION}.crate"


def files(directory):
    return {path.relative_to(directory): path.read_bytes() for path in directory.rglob("*") if path.is_file()}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--crate", type=Path)
    parser.add_argument("--output", type=Path)
    args = parser.parse_args()
    with tempfile.TemporaryDirectory(prefix="miniquad-source-") as temp:
        temp = Path(temp)
        archive = args.crate
        if archive is None:
            archive = temp / "source.crate"
            with urllib.request.urlopen(URL, timeout=30) as response:
                archive.write_bytes(response.read())
        if hashlib.sha256(archive.read_bytes()).hexdigest() != SHA256:
            raise ValueError("Miniquad archive checksum does not match the pinned source")
        with tarfile.open(archive) as source:
            source.extractall(temp, filter="data")
        rebuilt = temp / f"miniquad-{VERSION}"
        subprocess.run(["git", "apply", "--check", str(ROOT / "vendor/miniquad.patch")], cwd=rebuilt, check=True)
        subprocess.run(["git", "apply", str(ROOT / "vendor/miniquad.patch")], cwd=rebuilt, check=True)
        if args.output:
            shutil.copytree(rebuilt, args.output)
            print(f"Reconstructed patched Miniquad {VERSION}: {args.output}")
        else:
            expected, actual = files(rebuilt), files(ROOT / "vendor/miniquad")
            changed = sorted(path for path in expected.keys() | actual.keys() if expected.get(path) != actual.get(path))
            if changed:
                raise ValueError("Vendored source differs from reconstruction: " + ", ".join(map(str, changed)))
            print(f"Miniquad {VERSION}: {len(actual)} source files match the verified archive plus local patch")


if __name__ == "__main__":
    main()
