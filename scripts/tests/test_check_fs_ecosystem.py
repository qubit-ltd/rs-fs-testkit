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
            path.write_text(f'[package]\nname = "{package}"\nversion = "0.5.0"\n')
        fixture = self.root / "rs-fs-testkit/fixtures/s3-contract/Cargo.toml"
        fixture.parent.mkdir(parents=True)
        fixture.write_text('[package]\nname = "qubit-fs-s3-contract"\nversion = "0.0.0"\n')

    def test_missing_sibling_is_rejected(self):
        (self.root / "rs-mime/Cargo.toml").unlink()
        with self.assertRaises(FileNotFoundError): module.validate_root(self.root)

    def test_wrong_package_is_rejected(self):
        (self.root / "rs-mime/Cargo.toml").write_text('[package]\nname = "wrong"\n')
        with self.assertRaises(ValueError): module.validate_root(self.root)

    def test_metadata_requires_one_local_core(self):
        core = {"name": "qubit-fs", "source": None, "manifest_path": str(self.root / "rs-fs/Cargo.toml"), "version": "0.5.0"}
        with patch.object(module.subprocess, "run") as execute:
            execute.return_value.stdout = json.dumps({"packages": [core]})
            module.validate_core_graph(self.root, "rs-mime")
            for packages in [[], [core, core], [{**core, "source": "registry+https://example.invalid"}], [{**core, "version": "0.4.0"}], [{**core, "manifest_path": str(self.root / "other/Cargo.toml")}]]:
                execute.return_value.stdout = json.dumps({"packages": packages})
                with self.assertRaises(ValueError): module.validate_core_graph(self.root, "rs-mime")

    def test_all_nested_manifests_are_validated(self):
        nested = self.root / "rs-fs/fuzz/Cargo.toml"
        nested.parent.mkdir()
        nested.write_text('[package]\nname = "fuzz"\nversion = "0.0.0"\n')
        with patch.object(module, "validate_core_graph") as validate, patch.object(module, "run_tests"):
            module.main(["--sibling-root", str(self.root)])
        self.assertEqual(validate.call_count, 7)
        validate.assert_any_call(self.root, "rs-fs/fuzz")

    def test_lock_drift_is_rejected(self):
        lock = self.root / "rs-fs/Cargo.lock"
        lock.write_text("before")
        with patch.object(module, "validate_core_graph"), patch.object(module, "run_tests", side_effect=lambda _: lock.write_text("after")):
            with self.assertRaisesRegex(ValueError, "changed Cargo.lock"):
                module.main(["--sibling-root", str(self.root)])
