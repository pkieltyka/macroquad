# Prepare the GitHub release

The fork is GitHub-only (`publish = false`); retain package name and licenses.
Review source/runtime pairing, migrations, source/asset generation and security
claims before committing. Preview assets for review with:

```sh
python3 tools/prepare_release.py --preview --output target/release-preview
```

Preview manifests have `publishable: false`, no source archive and a deliberately
invalid commit placeholder. They must never be published. After review and a
clean commit, generate the final assets externally (no source edits needed):

```sh
python3 tools/prepare_release.py --output target/release-zubspace-v0.5.0
cd target/release-zubspace-v0.5.0
sha256sum -c SHA256SUMS
```

The generator refuses dirty/untracked source changes and an existing tag at a
different commit. It records the exact commit, vendored tree/patch and every
runtime input, creates deterministic source/runtime tarballs, and fills the
reviewed release notes template with concrete commit and asset checksums.
`SHA256SUMS` hashes notes and the manifest too; it intentionally excludes itself.
Run the public CI on the final pushed commit and require every job to pass before
publication, including the real Chrome/Firefox WASM browser job with explicit
WebGL1/2 graphics and runtime audio/UI/ownership/codec fixtures. The shared Rust,
MSRV and Miri jobs remain supporting correctness gates. Push without force;
create an annotated tag only if it
does not already exist, and verify the remote commit before release creation:

```sh
git tag -a zubspace-v0.5.0 -m 'Zubspace Macroquad 0.5.0' RELEASE_COMMIT
git push origin refs/tags/zubspace-v0.5.0
gh release create zubspace-v0.5.0 --repo pkieltyka/macroquad --verify-tag --title 'Zubspace Macroquad 0.5.0' --notes-file target/release-zubspace-v0.5.0/release-notes.md target/release-zubspace-v0.5.0/*
```

The only attached files are runtime, runtime provenance, runtime/source tarballs,
release manifest, notes and checksums. No application artifacts are included.
Download all assets into a fresh directory with `gh release download`, verify
`sha256sum -c SHA256SUMS`, and verify the remote tag peels to the release commit.
Record the actual GitHub release URL and public CI result in the handoff.
Do not overwrite existing release/tag refs or repair source after review without
another review/commit/check cycle.
