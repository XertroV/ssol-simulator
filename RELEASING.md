# Releasing `ssol_simulator`

This repo now uses tag-driven GitHub Actions to build release archives for:

- Windows x86_64
- Linux x86_64
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

Workflows are dual-host: **GitHub.com** (Windows + macOS + Linux) and **Forgejo**
(`git.lan` / `forgejo.lan`, Linux only via self-hosted `act_runner`).

A `define-matrix` job chooses platforms from `GITHUB_SERVER_URL` (Forgejo's schema
rejects job-level `if` expressions that read `matrix.*`):

| Platform | Where it runs |
| --- | --- |
| Linux x86_64 | Always (GitHub `ubuntu-latest` or Forgejo runner with that label) |
| Windows x86_64 | Only when `GITHUB_SERVER_URL` is `https://github.com` |
| macOS arm64 | Only when `GITHUB_SERVER_URL` is `https://github.com` |

- `CI Build`
  - Runs on pushes to `master` and manual `workflow_dispatch`
  - Builds all platforms available on the current host
  - Uploads the packaged archives as workflow artifacts
- `Release`
  - Runs when a tag matching `v*` is pushed
  - Continues only if the actor is the repository owner and the tag points at a commit on `master`
  - Builds available platforms, packages archives
  - On GitHub.com, creates a draft GitHub Release and uploads artifacts
  - On Forgejo, Linux artifacts are still uploaded as workflow artifacts (no auto draft release)

Third-party actions that are not mirrored on `data.forgejo.org` (for example
`dtolnay/rust-toolchain`, `Swatinem/rust-cache`, `softprops/action-gh-release`) use
full `https://github.com/...` URLs so Forgejo clones them from GitHub; the same
URLs work on GitHub-hosted runners.

Artifact upload uses `actions/upload-artifact@v4` on GitHub.com and `@v3` on
Forgejo (v4+ is not supported on GHES-compatible forges).

## Release Steps

1. Update `version` in `Cargo.toml`.
2. Commit the release changes and merge them to `master`.
3. Optionally run the `CI Build` workflow manually to confirm packaging before tagging.
4. Create and push the release tag (to GitHub and/or Forgejo, as you use them):

```bash
git tag vX.Y.Z
git push origin vX.Y.Z
```

5. Wait for the `Release` workflow to finish.
6. On GitHub: open the draft release, smoke-test archives, edit notes, publish.
7. On Forgejo: download Linux artifacts from the workflow run (or package locally).

## Artifact Verification

After downloading an artifact:

1. Extract it fully.
2. Confirm the archive contains the executable and the packaged `assets/` paths.
3. Launch the executable from the extracted directory.
4. Confirm the game starts and loads the scene successfully.

Do not move the executable out of the extracted folder on archive-based releases. It must stay next to the bundled `assets/` directory.

## Local Fallback Release Publishing

If automatic publishing is unavailable, create the draft release after producing
archives in `dist/`.

GitHub (`gh`):

```bash
gh release create "vX.Y.Z" dist/* --draft --generate-notes
# or attach more files later:
gh release upload "vX.Y.Z" dist/*
```

Forgejo API example (replace `TOKEN` and version):

```bash
curl -H "Authorization: token TOKEN" \
  -H "Content-Type: application/json" \
  -d '{"tag_name":"vX.Y.Z","name":"vX.Y.Z","draft":true}' \
  http://git.lan:30142/api/v1/repos/max/ssol-simulator/releases
# Then attach dist/* files through the UI or the releases attachments API.
```

## Notes On Asset Resolution

The runtime now resolves non-Bevy scene data through a shared asset-root helper that supports:

- `BEVY_ASSET_ROOT`
- launching from the repo root during development
- launching from an extracted release archive where `assets/` sits next to the executable

That means release archives should work without requiring users to run the game from the source checkout.

## Self-Hosted Linux Runner Notes (Forgejo)

Forgejo Linux jobs match the `ubuntu-latest` label on the local `act_runner`.

- Jobs use the `catthehacker/ubuntu:act-24.04` container image and install Bevy system
  deps with `apt-get` (`libasound2-dev`, `libudev-dev`, `libwayland-dev`, `libxkbcommon-dev`).
- If the runner labels change, update the Linux `runs-on` entry in both workflow files.
- Release builds still require the repository owner as actor and a tag on `master`.
- Windows/macOS are omitted from the Forgejo matrix entirely so they do not queue.
