#!/usr/bin/env python3
"""Validate the local filesystem crate ecosystem without rewriting inputs."""

import argparse
import hashlib
import json
import os
from pathlib import Path
import subprocess
import sys
import tomllib

PACKAGES = {
    "rs-fs": "qubit-fs",
    "rs-fs-testkit": "qubit-fs-testkit",
    "rs-fs-local": "qubit-fs-local",
    "rs-fs-registry": "qubit-fs-registry",
    "rs-mime": "qubit-mime",
}
MATRIX = [
    ("rs-fs", []), ("rs-fs", ["--features", "async"]),
    ("rs-fs-local", []), ("rs-fs-local", ["--features", "registry"]),
    ("rs-fs-registry", []), ("rs-fs-registry", ["--features", "async"]),
    ("rs-fs-testkit", []), ("rs-fs-testkit", ["--features", "async"]),
    ("rs-mime", []),
]


def validate_root(root: Path) -> None:
    for directory, expected in PACKAGES.items():
        manifest = root / directory / "Cargo.toml"
        data = tomllib.loads(manifest.read_text(encoding="utf-8"))
        if data["package"]["name"] != expected:
            raise ValueError(f"wrong package at {manifest}")
    fixture = root / "rs-fs-testkit/fixtures/s3-contract/Cargo.toml"
    if not fixture.is_file():
        raise ValueError(f"missing fixture manifest: {fixture}")


def run(command: list[str], cwd: Path) -> None:
    print(json.dumps({"cwd": str(cwd), "argv": command}), flush=True)
    subprocess.run(command, cwd=cwd, check=True)


def validate_core_graph(root: Path, directory: str) -> None:
    manifest = root / directory / "Cargo.toml"
    result = subprocess.run(
        ["cargo", "metadata", "--manifest-path", str(manifest),
         "--locked", "--all-features", "--format-version", "1"],
        cwd=root / directory, check=True, capture_output=True, text=True,
    )
    packages = json.loads(result.stdout)["packages"]
    cores = [package for package in packages if package["name"] == "qubit-fs"]
    expected = (root / "rs-fs/Cargo.toml").resolve()
    version = tomllib.loads(expected.read_text(encoding="utf-8"))["package"]["version"]
    if len(cores) != 1:
        raise ValueError(f"{directory}: expected exactly one qubit-fs, got {len(cores)}")
    core = cores[0]
    if (core["source"] is not None
            or Path(core["manifest_path"]).resolve() != expected
            or core["version"] != version):
        raise ValueError(f"{directory}: qubit-fs must resolve to the selected sibling source")


def owned_files(root: Path, filename: str):
    """Walk repository-owned files while pruning build and dependency trees."""
    for directory in PACKAGES:
        for current, directories, files in os.walk(root / directory):
            directories[:] = [name for name in directories if name not in
                              {"target", ".git", ".rs-ci", ".worktrees", ".cargo-home"}]
            if filename in files:
                yield Path(current) / filename


def lock_snapshot(root: Path) -> dict[str, str]:
    return {str(path): hashlib.sha256(path.read_bytes()).hexdigest()
            for path in owned_files(root, "Cargo.lock")}


def run_tests(root: Path) -> None:
    for directory, features in MATRIX:
        run(["cargo", "test", "--manifest-path", str(root / directory / "Cargo.toml"),
             "--locked", "--no-default-features", *features], root / directory)
    run(["cargo", "test", "--locked", "--manifest-path",
         str(root / "rs-fs-testkit/fixtures/s3-contract/Cargo.toml")],
        root / "rs-fs-testkit")


def run_ci(root: Path) -> None:
    # Each current CI script already runs default and all-feature tests.
    for directory in PACKAGES:
        script = root / directory / "ci-check.sh"
        if not script.is_file():
            raise ValueError(f"missing CI entry: {script}")
        run(["bash", str(script)], root / directory)


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--sibling-root", type=Path, required=True)
    parser.add_argument("--phase", choices=["tests", "ci"], default="tests")
    args = parser.parse_args(argv)
    root = args.sibling_root.resolve(strict=True)
    validate_root(root)
    before = lock_snapshot(root)
    try:
        for manifest in owned_files(root, "Cargo.toml"):
            validate_core_graph(root, str(manifest.parent.relative_to(root)))
        if args.phase == "tests":
            run_tests(root)
        else:
            run_ci(root)
    finally:
        if lock_snapshot(root) != before:
            raise ValueError("validation changed Cargo.lock files; inspect the changes")
    print(json.dumps({"result": "PASS", "phase": args.phase, "root": str(root)}))
    return 0


if __name__ == "__main__":
    try:
        sys.exit(main())
    except (OSError, ValueError, KeyError, subprocess.CalledProcessError) as error:
        if isinstance(error, subprocess.CalledProcessError) and error.stderr:
            print(error.stderr, file=sys.stderr)
        print(f"ecosystem validation failed: {error}", file=sys.stderr)
        sys.exit(1)