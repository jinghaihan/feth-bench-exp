"""Check release version selection without changing the repository or publishing."""

import tempfile
import unittest
from pathlib import Path
from unittest.mock import patch

import release


class ReleaseTests(unittest.TestCase):
  def test_version_increments(self):
    self.assertEqual(release.bump_version("0.1.0", "patch"), "0.1.1")
    self.assertEqual(release.bump_version("0.1.9", "minor"), "0.2.0")
    self.assertEqual(release.bump_version("0.1.9", "major"), "1.0.0")
    with self.assertRaises(ValueError):
      release.bump_version("0.1.0", "invalid")

  def test_updates_package_and_version_without_changing_dependency_versions(self):
    with tempfile.TemporaryDirectory() as directory:
      root = Path(directory)
      cargo = '[package]\nname = "feth-bench-exp"\nversion = "0.1.0"\n\n[dependencies.test]\nversion = "1.2.3"\n'
      (root / "Cargo.toml").write_text(cargo, encoding="utf-8")
      (root / "VERSION").write_text("0.1.0\n", encoding="utf-8")
      with patch.object(release, "ROOT", root):
        self.assertEqual(release.version(), "0.1.0")
        release.write_version("0.1.1")
        self.assertEqual(release.version(), "0.1.1")
      self.assertEqual((root / "VERSION").read_text(encoding="utf-8"), "0.1.1\n")
      self.assertEqual(
        (root / "Cargo.toml").read_text(encoding="utf-8"),
        cargo.replace('version = "0.1.0"', 'version = "0.1.1"', 1),
      )

  def test_rejects_inconsistent_versions(self):
    with tempfile.TemporaryDirectory() as directory:
      root = Path(directory)
      (root / "Cargo.toml").write_text('[package]\nversion = "0.1.0"\n', encoding="utf-8")
      (root / "VERSION").write_text("0.1.1\n", encoding="utf-8")
      with patch.object(release, "ROOT", root), self.assertRaises(ValueError):
        release.version()


if __name__ == "__main__":
  unittest.main()
