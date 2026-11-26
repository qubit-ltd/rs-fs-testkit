#!/usr/bin/env python3
"""Validate the released filesystem crate dependency graph without rewriting inputs."""

import argparse
import hashlib
import json
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
REGISTRY_SOURCE = "registry+https://github.com/rust-lang/crates.io-index"
S3_FIXTURE = "rs-fs-testkit/fixtures/s3-contract"
CORE_PIN_FILE = ".infra/ci/local-path-dependencies.tsv"
MATRIX = [
    ("rs-fs", []), ("rs-fs", ["--features", "async"]),
    ("rs-fs-local", []), ("rs-fs-local", ["--features", "registry"]),
    ("rs-fs-registry", []), ("rs-fs-registry", ["--features", "async"]),
    ("rs-fs-testkit", []), ("rs-fs-testkit", ["--features", "async"]),
    ("rs-mime", []),
]


def selected_directories() -> tuple[str, ...]:
    return (*PACKAGES, S3_FIXTURE)


def validate_root(root: Path, directories=PACKAGES) -> None:
    for directory in directories:
        manifest = root / directory / "Cargo.toml"
        if directory == S3_FIXTURE:
            if not manifest.is_file():
                raise ValueError(f"missing fixture manifest: {manifest}")
            continue
        expected = PACKAGES[directory]
        data = tomllib.loads(manifest.read_text(encoding="utf-8"))
        if data["package"]["name"] != expected:
            raise ValueError(f"wrong package at {manifest}")
    fixture = root / S3_FIXTURE / "Cargo.toml"
    if not fixture.is_file():
        raise ValueError(f"missing fixture manifest: {fixture}")


def run(command: list[str], cwd: Path) -> None:
    print(json.dumps({"cwd": str(cwd), "argv": command}), flush=True)
    subprocess.run(command, cwd=cwd, check=True)


def git_sha(path: Path) -> str:
    return subprocess.run(
        ["git", "-C", str(path), "rev-parse", "HEAD"],
        check=True, capture_output=True, text=True,
    ).stdout.strip()


def pinned_core_sha(root: Path) -> str:
    for line in (root / "rs-fs-testkit" / CORE_PIN_FILE).read_text(encoding="utf-8").splitlines():
        if not line or line.startswith("#"):
            continue
        fields = line.split("\t")
        if len(fields) != 3 or fields[0] != "../rs-fs":
            raise ValueError(f"invalid core pin in {CORE_PIN_FILE}: {line!r}")
        if len(fields[2]) != 40 or any(char not in "0123456789abcdef" for char in fields[2]):
            raise ValueError(f"core pin must be a full Git SHA in {CORE_PIN_FILE}")
        return fields[2]
    raise ValueError(f"missing rs-fs pin in {CORE_PIN_FILE}")


def validate_core_graph(root: Path, directory: str, source_mode: str = "local") -> None:
    manifest = root / directory / "Cargo.toml"
    try:
        result = subprocess.run(
            ["cargo", "metadata", "--manifest-path", str(manifest),
             "--locked", "--all-features", "--format-version", "1"],
            cwd=root / directory, check=True, capture_output=True, text=True,
        )
    except subprocess.CalledProcessError as error:
        cargo_stderr = (error.stderr or "").strip()
        detail = f"; cargo stderr: {cargo_stderr}" if cargo_stderr else "; cargo stderr: <empty>"
        raise ValueError(
            f"{directory}: cargo metadata failed for {manifest}{detail}"
        ) from error
    metadata = json.loads(result.stdout)
    packages = metadata["packages"]
    manifest_data = tomllib.loads(manifest.read_text(encoding="utf-8"))
    declared = set()
    for section in ("dependencies", "dev-dependencies", "build-dependencies"):
        for dependency, specification in manifest_data.get(section, {}).items():
            package_name = specification.get("package", dependency) if isinstance(specification, dict) else dependency
            declared.add(package_name)
    expected_direct = {"qubit-fs", "qubit-fs-testkit"} & declared
    resolve = metadata.get("resolve") or {}
    root_id = resolve.get("root")
    root_node = next((node for node in resolve.get("nodes", []) if node["id"] == root_id), None)
    direct_ids = {dependency["pkg"] for dependency in root_node["deps"]} if root_node else set()
    resolved = {
        name: [package for package in packages
               if package["name"] == name and package.get("id") in direct_ids]
        for name in expected_direct
    }
    expected_paths = {
        "qubit-fs": (root / "rs-fs/Cargo.toml").resolve(),
        "qubit-fs-testkit": (root / "rs-fs-testkit/Cargo.toml").resolve(),
    }
    if source_mode == "registry" and "qubit-fs" in expected_direct:
        cores = resolved["qubit-fs"]
        expected = expected_paths["qubit-fs"]
        version = tomllib.loads(expected.read_text(encoding="utf-8"))["package"]["version"]
        if len(cores) != 1 or cores[0]["source"] != REGISTRY_SOURCE or cores[0]["version"] != version:
            raise ValueError(f"{directory}: qubit-fs must resolve to crates.io {version} ({REGISTRY_SOURCE})")
        return
    pinned_sha = pinned_core_sha(root) if source_mode == "local" and expected_direct else None
    for name in expected_direct:
        candidates = resolved[name]
        if len(candidates) != 1:
            if not candidates:
                raise ValueError(f"{directory}: expected direct dependency {name} is missing from cargo metadata")
            raise ValueError(f"{directory}: expected exactly one direct {name}, got {len(candidates)}")
        package = candidates[0]
        expected = expected_paths[name]
        expected_version = tomllib.loads(expected.read_text(encoding="utf-8"))["package"]["version"]
        valid = (package["source"] is None
                 and Path(package["manifest_path"]).resolve() == expected
                 and package["version"] == expected_version)
        expected_sha = pinned_sha if name == "qubit-fs" else git_sha(expected.parent)
        actual_sha = git_sha(Path(package["manifest_path"]).resolve().parent) if package["source"] is None else None
        valid = valid and actual_sha == expected_sha
        if not valid:
            raise ValueError(
                f"{directory}: {name} must resolve to {expected} at Git SHA {expected_sha}; "
                f"got source={package['source']!r}, version={package['version']!r}, "
                f"manifest={package['manifest_path']}, SHA={actual_sha}"
            )


def selected_files(root: Path, filename: str, directories=None):
    """Return files belonging to the explicit release validation matrix."""
    for directory in selected_directories() if directories is None else directories:
        path = root / directory / filename
        if path.is_file() or filename == "Cargo.lock":
            yield path


def lock_snapshot(root: Path, directories=None) -> dict[str, str]:
    return {str(path): (hashlib.sha256(path.read_bytes()).hexdigest()
                        if path.is_file() else "<missing>")
            for path in selected_files(root, "Cargo.lock", directories)}


def run_tests(root: Path) -> None:
    for directory, features in MATRIX:
        run(["cargo", "test", "--manifest-path", str(root / directory / "Cargo.toml"),
             "--locked", "--no-default-features", *features], root / directory)
    run(["cargo", "test", "--locked", "--manifest-path",
         str(root / "rs-fs-testkit/fixtures/s3-contract/Cargo.toml")],
        root / "rs-fs-testkit")


def run_ci(root: Path) -> None:
    script = root / "rs-fs-testkit/.infra/bin/ci-check.sh"
    if not script.is_file():
        raise ValueError(f"missing CI entry: {script}")
    run(["bash", str(script)], root / "rs-fs-testkit")


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--sibling-root", type=Path, required=True)
    parser.add_argument("--phase", choices=["check", "tests", "ci"], default="tests")
    parser.add_argument("--source-mode", choices=["local", "registry"], default="local")
    args = parser.parse_args(argv)
    root = args.sibling_root.resolve(strict=True)
    directories = selected_directories() if args.phase == "tests" else (
        ("rs-fs-testkit", S3_FIXTURE) if args.phase == "check" else ("rs-fs-testkit",)
    )
    validate_root(root, directories)
    before = lock_snapshot(root, directories)
    try:
        for directory in directories:
            validate_core_graph(root, directory, args.source_mode)
        if args.phase == "tests":
            run_tests(root)
        elif args.phase == "ci":
            run_ci(root)
    finally:
        if lock_snapshot(root, directories) != before:
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
