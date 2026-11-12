"""Focused tests for the documentation link checker."""

import importlib.util
import tempfile
import unittest
from pathlib import Path


SCRIPT = Path(__file__).resolve().parents[1] / "check-doc-links.py"
SPEC = importlib.util.spec_from_file_location("check_doc_links", SCRIPT)
checker = importlib.util.module_from_spec(SPEC)
assert SPEC.loader is not None
SPEC.loader.exec_module(checker)


class CheckDocLinksTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.root = Path(self.temp.name)
        self.doc = self.root / "README.md"
        (self.root / "doc").mkdir()
        (self.root / "doc" / "target file.md").write_text("ok", encoding="utf-8")

    def tearDown(self):
        self.temp.cleanup()

    def check_text(self, content):
        self.doc.write_text(content, encoding="utf-8")
        return checker.check(self.root, [self.doc])

    def test_accepts_inline_reference_image_external_fragment_and_encoded_local_links(self):
        errors = self.check_text(
            "[inline](doc/target%20file.md#part) ![image](doc/target%20file.md)\n"
            "[ref][target]\n[target]: <doc/target%20file.md>\n"
            "[web](https://example.com/a) [mail](mailto:a@example.com) [here](#section)\n"
        )
        self.assertEqual([], errors)

    def test_ignores_links_inside_fenced_code(self):
        self.assertEqual([], self.check_text("```md\n[bad](absent.md)\n```\n"))

    def test_resolves_collapsed_and_shortcut_reference_links(self):
        errors = self.check_text(
            "[collapsed][] [shortcut]\n"
            "[collapsed]: doc/missing.md\n"
            "[shortcut]: doc/target%20file.md\n"
        )
        self.assertEqual(1, len(errors))
        self.assertIn("README.md:1", errors[0])
        self.assertIn("doc/missing.md", errors[0])

    def test_reports_missing_target_with_file_and_line(self):
        errors = self.check_text("heading\n![x](missing.png)\n")
        self.assertEqual(1, len(errors))
        self.assertIn("README.md:2", errors[0])
        self.assertIn("missing.png", errors[0])

    def test_rejects_repository_escape_and_absolute_path(self):
        errors = self.check_text("[escape](../outside.md) [absolute](/etc/passwd)\n")
        self.assertEqual(2, len(errors))
        self.assertTrue(all("escapes repository" in error for error in errors))


if __name__ == "__main__":
    unittest.main()
