# Releasing `ssol_simulator`

This repo now uses tag-driven GitHub Actions to build release archives for:

- Windows x86_64
- Linux x86_64
- macOS x86_64
- macOS arm64

## Current Release Format

Releases are currently archive-based, not single-binary.

That is the intentional first-pass choice for this project because the game depends on runtime assets loaded from disk at startup, including models, textures, shaders, audio, fonts, and the scene JSON. A true single-file build is still possible in theory, but it would require an asset-embedding path that is stable for this Bevy `0.17` codebase and worth the extra complexity.

Each release artifact therefore contains:

- the platform executable
- the required `assets/` content listed in `scripts/release_assets.txt`
- a small `README.txt`

## Release Preconditions

- `Cargo.toml` contains the version you want to release.
- The git tag matches that version exactly, prefixed with `v`.
  - Example: `Cargo.toml` version `0.1.0` -> tag `v0.1.0`
- The default release build is the default Cargo feature set.
  - The optional `ai` feature is not part of the automated release artifacts right now.
- GitHub Actions must be allowed to create releases in this repository.

## Runtime Assets Included

The release packaging script reads `scripts/release_assets.txt`.

Current packaged asset paths:

- `assets/audio`
- `assets/fonts`
- `assets/models`
- `assets/scenes/level-zero.json`
- `assets/shaders`
- `assets/textures`

When new runtime-loaded assets are added, update `scripts/release_assets.txt` so releases stay complete without shipping unrelated dev files.

## Local Dry Run

On Linux, a local packaging dry run looks like this:

```bash
cargo build --release --locked
python3 scripts/package_release.py \
  --target x86_64-unknown-linux-gnu \
  --version 0.1.0-local \
  --binary target/release/ssol_simulator \
  --output-dir dist
```

This produces an archive under `dist/`.

For other platforms, use the matching target triple and binary path on a native machine, or use GitHub Actions.

## CI Workflows

- `CI Build`
  - Runs on pull requests, pushes to `main`, and manual `workflow_dispatch`
  - Builds and packages the same platform matrix as the release workflow
  - Uploads the packaged archives as workflow artifacts

- `Release`
  - Runs when a tag matching `v*` is pushed
  - Builds native binaries on Windows, Linux, and macOS runners
  - Packages the release archives
  - Creates a draft GitHub Release and uploads all artifacts

## Release Steps

1. Update `version` in `Cargo.toml`.
2. Commit the release changes and merge them to `main`.
3. Optionally run the `CI Build` workflow manually to confirm packaging before tagging.
4. Create and push the release tag:

```bash
git tag vX.Y.Z
git push origin vX.Y.Z
```

5. Wait for the `Release` workflow to finish.
6. Open the draft release on GitHub.
7. Download and smoke-test the archives you care about.
8. Edit the release notes if needed.
9. Publish the draft release.

## Artifact Verification

After downloading an artifact:

1. Extract it fully.
2. Confirm the archive contains the executable and the packaged `assets/` paths.
3. Launch the executable from the extracted directory.
4. Confirm the game starts and loads the scene successfully.

Do not move the executable out of the extracted folder on archive-based releases. It must stay next to the bundled `assets/` directory.

## Local Fallback Release Publishing

If GitHub Actions builds succeed but automatic publishing is unavailable, you can create the draft release locally with `gh`.

Example after producing archives in `dist/`:

```bash
gh release create "vX.Y.Z" dist/* --draft --generate-notes
```

If the draft release already exists and you need to add artifacts:

```bash
gh release upload "vX.Y.Z" dist/*
```

## Notes On Asset Resolution

The runtime now resolves non-Bevy scene data through a shared asset-root helper that supports:

- `BEVY_ASSET_ROOT`
- launching from the repo root during development
- launching from an extracted release archive where `assets/` sits next to the executable

That means release archives should work without requiring users to run the game from the source checkout.
