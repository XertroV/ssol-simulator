#!/usr/bin/env python3
from __future__ import annotations

import argparse
import shutil
import tarfile
import textwrap
import zipfile
from pathlib import Path
from tempfile import TemporaryDirectory

REPO_ROOT = Path(__file__).resolve().parent.parent
ASSET_MANIFEST = Path(__file__).with_name("release_assets.txt")

TARGET_METADATA = {
    "x86_64-pc-windows-msvc": ("windows", "x86_64", "zip"),
    "x86_64-unknown-linux-gnu": ("linux", "x86_64", "tar.gz"),
    "aarch64-apple-darwin": ("macos", "arm64", "tar.gz"),
}


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        description="Package ssol_simulator release archives with runtime assets."
    )
    parser.add_argument("--target", required=True, choices=sorted(TARGET_METADATA))
    parser.add_argument("--version", required=True, help="Release version, with or without a leading v.")
    parser.add_argument("--binary", required=True, type=Path, help="Path to the built executable.")
    parser.add_argument("--output-dir", required=True, type=Path, help="Directory to write archives into.")
    return parser.parse_args()


def normalized_version(version: str) -> str:
    return version[1:] if version.startswith("v") else version


def release_name(version: str, target: str) -> str:
    platform_name, arch, _archive_format = TARGET_METADATA[target]
    return f"ssol-simulator-v{normalized_version(version)}-{platform_name}-{arch}"


def manifest_paths() -> list[Path]:
    lines = [line.strip() for line in ASSET_MANIFEST.read_text(encoding="utf-8").splitlines()]
    return [Path(line) for line in lines if line and not line.startswith("#")]


def copy_runtime_assets(staging_root: Path) -> None:
    for relative_path in manifest_paths():
        source = REPO_ROOT / relative_path
        destination = staging_root / relative_path
        if not source.exists():
            raise FileNotFoundError(f"Missing required release asset path: {relative_path}")

        if source.is_dir():
            shutil.copytree(source, destination, dirs_exist_ok=True)
        else:
            destination.parent.mkdir(parents=True, exist_ok=True)
            shutil.copy2(source, destination)

    shutil.copy2(ASSET_MANIFEST, staging_root / "PACKAGED_ASSETS.txt")


def write_readme(staging_root: Path, version: str, target: str) -> None:
    bundle_name = release_name(version, target)
    readme = textwrap.dedent(
        f"""\
        Open SSOL release bundle: {bundle_name}

        Contents:
        - executable: run the bundled game binary
        - assets/: required runtime assets for this build

        Notes:
        - Extract the archive before launching the game.
        - Keep the executable and the bundled assets/ directory together.
        - If you move only the executable, runtime asset loading will fail.
        """
    )
    (staging_root / "README.txt").write_text(readme, encoding="utf-8")


def create_archive(staging_root: Path, output_dir: Path, target: str, version: str) -> Path:
    bundle_name = release_name(version, target)
    archive_format = TARGET_METADATA[target][2]
    output_dir.mkdir(parents=True, exist_ok=True)

    if archive_format == "zip":
        archive_path = output_dir / f"{bundle_name}.zip"
        with zipfile.ZipFile(archive_path, "w", compression=zipfile.ZIP_DEFLATED) as archive:
            for path in staging_root.rglob("*"):
                archive.write(path, path.relative_to(staging_root.parent))
        return archive_path

    archive_path = output_dir / f"{bundle_name}.tar.gz"
    with tarfile.open(archive_path, "w:gz") as archive:
        archive.add(staging_root, arcname=staging_root.name)
    return archive_path


def main() -> int:
    args = parse_args()
    binary_path = args.binary.resolve()
    if not binary_path.is_file():
        raise FileNotFoundError(f"Built binary not found: {binary_path}")

    bundle_name = release_name(args.version, args.target)
    with TemporaryDirectory() as temp_dir:
        temp_root = Path(temp_dir)
        staging_root = temp_root / bundle_name
        staging_root.mkdir(parents=True, exist_ok=True)

        shutil.copy2(binary_path, staging_root / binary_path.name)
        copy_runtime_assets(staging_root)
        write_readme(staging_root, args.version, args.target)

        archive_path = create_archive(staging_root, args.output_dir.resolve(), args.target, args.version)
        print(archive_path)

    return 0


if __name__ == "__main__":
    raise SystemExit(main())
