"""Test command construction and validation boundaries without running Cargo."""

import importlib.util
import json
from pathlib import Path
import subprocess
import tempfile
import unittest
from unittest.mock import patch

source = Path(__file__).resolve().parents[1] / "check-fs-ecosystem.py"
spec = importlib.util.spec_from_file_location("fs_ecosystem_check", source)
module = importlib.util.module_from_spec(spec)
spec.loader.exec_module(module)


def metadata_result(packages, direct_names=()):
    packages = [{**package, "id": package.get("id", f"{package['name']}-{index}")}
                for index, package in enumerate(packages)]
    root_id = "fixture-root"
    root_package = {"id": root_id, "name": "fixture-root", "source": None,
                    "manifest_path": "/tmp/fixture/Cargo.toml", "version": "0.0.0"}
    direct_ids = [package["id"] for package in packages if package["name"] in direct_names]
    return json.dumps({
        "packages": [root_package, *packages],
        "resolve": {"root": root_id, "nodes": [{"id": root_id,
                    "deps": [{"pkg": package_id} for package_id in direct_ids]}]},
    })


class CommandTests(unittest.TestCase):
    def test_test_matrix_uses_locked_and_explicit_features(self):
        with patch.object(module.subprocess, "run") as execute:
            module.run_tests(Path("/tmp/fs-ecosystem-test"))
        self.assertEqual(execute.call_count, 10)
        for call in execute.call_args_list:
            self.assertIn("--locked", call.args[0])
            self.assertTrue(call.kwargs["check"])
            self.assertNotIn("shell", call.kwargs)
        self.assertIn("async", execute.call_args_list[1].args[0])
        self.assertIn("registry", execute.call_args_list[3].args[0])

    def test_failure_stops_the_matrix(self):
        failure = subprocess.CalledProcessError(101, ["cargo", "test"])
        with patch.object(module.subprocess, "run", side_effect=failure) as execute:
            with self.assertRaises(subprocess.CalledProcessError):
                module.run_tests(Path("/tmp/fs-ecosystem-test"))
        self.assertEqual(execute.call_count, 1)


class InputTests(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory()
        self.addCleanup(self.temporary.cleanup)
        self.root = Path(self.temporary.name)
        for directory, package in module.PACKAGES.items():
            path = self.root / directory / "Cargo.toml"
            path.parent.mkdir()
            dependencies = '\n[dependencies]\nqubit-fs = { version = "^0.5", path = "../rs-fs" }\n' if directory == "rs-fs-testkit" else ""
            path.write_text(f'[package]\nname = "{package}"\nversion = "0.5.0"\n{dependencies}')
        fixture = self.root / "rs-fs-testkit/fixtures/s3-contract/Cargo.toml"
        fixture.parent.mkdir(parents=True)
        fixture.write_text('[package]\nname = "qubit-fs-s3-contract"\nversion = "0.0.0"\n[dependencies]\nqubit-fs = "^0.5"\nqubit-fs-testkit = "^0.9"\n')
        pin = self.root / "rs-fs-testkit/.infra/ci/local-path-dependencies.tsv"
        pin.parent.mkdir(parents=True)
        pin.write_text("../rs-fs\thttps://github.com/qubit-ltd/rs-fs\t" + "b" * 40 + "\n")

    def test_missing_sibling_is_rejected(self):
        (self.root / "rs-mime/Cargo.toml").unlink()
        with self.assertRaises(FileNotFoundError): module.validate_root(self.root)

    def test_wrong_package_is_rejected(self):
        (self.root / "rs-mime/Cargo.toml").write_text('[package]\nname = "wrong"\n')
        with self.assertRaises(ValueError): module.validate_root(self.root)

    def test_metadata_requires_expected_core_source_and_version(self):
        core = {"name": "qubit-fs", "source": module.REGISTRY_SOURCE,
                "manifest_path": str(self.root / "registry/Cargo.toml"), "version": "0.5.0"}
        with patch.object(module.subprocess, "run") as execute:
            execute.return_value.stdout = metadata_result([core], ["qubit-fs"])
            module.validate_core_graph(self.root, "rs-fs-testkit", "registry")
            for packages in [[], [core, core], [{**core, "source": "registry+https://example.invalid"}],
                             [{**core, "version": "0.4.0"}]]:
                execute.return_value.stdout = metadata_result(packages, ["qubit-fs"])
                with self.assertRaises(ValueError):
                    module.validate_core_graph(self.root, "rs-fs-testkit", "registry")

    def test_metadata_failure_preserves_manifest_and_cargo_stderr(self):
        failure = subprocess.CalledProcessError(
            101, ["cargo", "metadata"], stderr="error: dependency resolution failed"
        )
        manifest = self.root / "rs-mime/Cargo.toml"
        with patch.object(module.subprocess, "run", side_effect=failure):
            with self.assertRaises(ValueError) as raised:
                module.validate_core_graph(self.root, "rs-mime")
        message = str(raised.exception)
        self.assertIn(str(manifest), message)
        self.assertIn("error: dependency resolution failed", message)

    def test_default_source_mode_is_local_and_registry_mode_is_explicit(self):
        local_core = {"name": "qubit-fs", "source": None, "manifest_path": str(self.root / "rs-fs/Cargo.toml"), "version": "0.5.0"}
        registry_core = {**local_core, "source": module.REGISTRY_SOURCE}
        with patch.object(module.subprocess, "run") as execute, patch.object(module, "git_sha", return_value="b" * 40):
            execute.return_value.stdout = metadata_result([local_core], ["qubit-fs"])
            module.validate_core_graph(self.root, "rs-fs-testkit")
            with self.assertRaisesRegex(ValueError, "crates.io"):
                module.validate_core_graph(self.root, "rs-fs-testkit", "registry")
            execute.return_value.stdout = metadata_result([registry_core], ["qubit-fs"])
            with self.assertRaisesRegex(ValueError, "Git SHA"):
                module.validate_core_graph(self.root, "rs-fs-testkit")

    def test_candidate_graph_rejects_same_version_core_from_wrong_directory(self):
        core = {"name": "qubit-fs", "source": None,
                "manifest_path": str(self.root / "elsewhere/Cargo.toml"),
                "version": "0.5.0"}
        with patch.object(module.subprocess, "run") as execute, patch.object(module, "git_sha", return_value="b" * 40):
            execute.return_value.stdout = metadata_result([core], ["qubit-fs"])
            with self.assertRaisesRegex(ValueError, "must resolve"):
                module.validate_core_graph(self.root, "rs-fs-testkit")

    def test_candidate_graph_rejects_correct_core_path_with_wrong_git_sha(self):
        core = {"name": "qubit-fs", "source": None,
                "manifest_path": str(self.root / "rs-fs/Cargo.toml"),
                "version": "0.5.0"}
        with patch.object(module.subprocess, "run") as execute, \
                patch.object(module, "git_sha", return_value="a" * 40):
            execute.return_value.stdout = metadata_result([core], ["qubit-fs"])
            with self.assertRaisesRegex(ValueError, "SHA"):
                module.validate_core_graph(self.root, "rs-fs-testkit")

    def test_candidate_graph_requires_testkit_node_for_s3_fixture(self):
        core = {"name": "qubit-fs", "source": None,
                "manifest_path": str(self.root / "rs-fs/Cargo.toml"), "version": "0.5.0"}
        with patch.object(module.subprocess, "run") as execute, patch.object(module, "git_sha", return_value="b" * 40):
            execute.return_value.stdout = metadata_result([core], ["qubit-fs"])
            with self.assertRaisesRegex(ValueError, "qubit-fs-testkit"):
                module.validate_core_graph(self.root, module.S3_FIXTURE)

    def test_only_explicit_release_manifests_are_validated(self):
        nested = self.root / "rs-fs/fuzz/Cargo.toml"
        nested.parent.mkdir()
        nested.write_text('[package]\nname = "fuzz"\nversion = "0.0.0"\n')
        with patch.object(module, "validate_core_graph") as validate, patch.object(module, "run_tests"):
            module.main(["--sibling-root", str(self.root)])
        self.assertEqual(validate.call_count, 6)
        validate.assert_any_call(self.root, module.S3_FIXTURE, "local")
        self.assertFalse(any(call.args[1] == "rs-fs/fuzz" for call in validate.call_args_list))

    def test_ci_only_validates_testkit_even_when_downstream_manifest_is_missing(self):
        (self.root / "rs-fs-local/Cargo.toml").unlink()
        with patch.object(module, "validate_core_graph") as validate, patch.object(module, "run_ci") as run_ci:
            module.main(["--sibling-root", str(self.root), "--phase", "ci"])
        validate.assert_called_once_with(self.root, "rs-fs-testkit", "local")
        run_ci.assert_called_once_with(self.root)

    def test_ci_entry_uses_only_testkit_infra_script(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            script = root / "rs-fs-testkit/.infra/bin/ci-check.sh"
            script.parent.mkdir(parents=True)
            script.touch()
            with patch.object(module, "run") as execute:
                module.run_ci(root)
            execute.assert_called_once_with(["bash", str(script)], root / "rs-fs-testkit")

    def test_ci_rejects_missing_entry_before_running_commands(self):
        with tempfile.TemporaryDirectory() as temporary:
            with patch.object(module, "run") as execute:
                with self.assertRaisesRegex(ValueError, "missing CI entry"):
                    module.run_ci(Path(temporary))
            execute.assert_not_called()

    def test_lock_drift_is_rejected(self):
        lock = self.root / "rs-fs/Cargo.lock"
        lock.write_text("before")
        with patch.object(module, "validate_core_graph"), patch.object(module, "run_tests", side_effect=lambda _: lock.write_text("after")):
            with self.assertRaisesRegex(ValueError, "changed Cargo.lock"):
                module.main(["--sibling-root", str(self.root)])

    def test_new_selected_lockfile_is_rejected(self):
        fixture_lock = self.root / module.S3_FIXTURE / "Cargo.lock"
        with patch.object(module, "validate_core_graph"), patch.object(
            module, "run_tests", side_effect=lambda _: fixture_lock.write_text("after")
        ):
            with self.assertRaisesRegex(ValueError, "changed Cargo.lock"):
                module.main(["--sibling-root", str(self.root)])

    def test_ci_detects_new_testkit_lockfile(self):
        lock = self.root / "rs-fs-testkit/Cargo.lock"
        with patch.object(module, "validate_core_graph"), patch.object(
            module, "run_ci", side_effect=lambda _: lock.write_text("created")
        ):
            with self.assertRaisesRegex(ValueError, "changed Cargo.lock"):
                module.main(["--sibling-root", str(self.root), "--phase", "ci"])
