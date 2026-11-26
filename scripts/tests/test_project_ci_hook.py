"""Verify the project CI hook and source identity workflow propagate failures."""

import os
from pathlib import Path
import subprocess
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[2]
HOOK = ROOT / ".infra/project-ci-check.local.sh"
WORKFLOW = ROOT / ".github/workflows/ci.yml"


class ProjectCiHookTests(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory()
        self.addCleanup(self.temporary.cleanup)
        self.bin = Path(self.temporary.name) / "bin"
        self.bin.mkdir()
        self.log = Path(self.temporary.name) / "commands.log"
        python = self.bin / "python3"
        python.write_text(
            "#!/usr/bin/env bash\n"
            "printf '%s\\n' \"$*\" >> \"$COMMAND_LOG\"\n"
            "case \"$*\" in *\"$FAIL_MATCH\"*) exit \"$FAIL_STATUS\" ;; esac\n"
        )
        python.chmod(0o755)
        self.env = {
            **os.environ,
            "PATH": f"{self.bin}:{os.environ['PATH']}",
            "COMMAND_LOG": str(self.log),
            "FAIL_MATCH": "",
            "FAIL_STATUS": "0",
        }

    def test_hook_runs_both_checks_and_propagates_each_failure(self):
        cases = [
            ("check-doc-links.py", 23, ["-B scripts/check-doc-links.py"]),
            ("unittest discover", 29, ["-B scripts/check-doc-links.py", "-B -m unittest discover -s scripts/tests"]),
        ]
        for fail_match, status, expected in cases:
            with self.subTest(fail_match=fail_match):
                self.log.unlink(missing_ok=True)
                environment = {**self.env, "FAIL_MATCH": fail_match, "FAIL_STATUS": str(status)}
                result = subprocess.run(["bash", str(HOOK)], cwd=self.temporary.name,
                                        env=environment, check=False)
                self.assertEqual(result.returncode, status)
                self.assertEqual(self.log.read_text().splitlines(), expected)

    def test_workflow_invocation_propagates_identity_check_failure(self):
        command = "python3 -B scripts/check-fs-ecosystem.py --phase check --sibling-root .."
        self.assertEqual(WORKFLOW.read_text().count(f"run: {command}"), 2)
        environment = {**self.env, "FAIL_MATCH": "check-fs-ecosystem.py", "FAIL_STATUS": "37"}
        result = subprocess.run(["bash", "-e", "-o", "pipefail", "-c", command],
                                cwd=ROOT, env=environment, check=False)
        self.assertEqual(result.returncode, 37)


if __name__ == "__main__":
    unittest.main()
