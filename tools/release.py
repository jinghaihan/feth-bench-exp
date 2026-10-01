#!/usr/bin/env python3

"""Validate and publish a versioned Skyline plugin release."""

from __future__ import annotations

import argparse
import re
import subprocess
from pathlib import Path


ROOT = Path(__file__).resolve().parent.parent
NRO = "target/aarch64-skyline-switch/release/libfeth_bench_exp.nro"


def run(*args: str, capture: bool = False) -> str:
  result = subprocess.run(args, cwd=ROOT, check=True, text=True, capture_output=capture)
  return result.stdout.strip() if capture else ""


def version() -> str:
  value = (ROOT / "VERSION").read_text(encoding="utf-8").strip()
  if not re.fullmatch(r"\d+\.\d+\.\d+", value):
    raise ValueError(f"invalid VERSION: {value}")
  cargo = (ROOT / "Cargo.toml").read_text(encoding="utf-8")
  match = re.search(r'(?m)^version = "(\d+\.\d+\.\d+)"$', cargo)
  if match is None or match.group(1) != value:
    raise ValueError("Cargo.toml and VERSION do not agree")
  return value


def bump_version(current: str, part: str) -> str:
  major, minor, patch = (int(value) for value in current.split("."))
  if part == "major":
    return f"{major + 1}.0.0"
  if part == "minor":
    return f"{major}.{minor + 1}.0"
  if part == "patch":
    return f"{major}.{minor}.{patch + 1}"
  raise ValueError(f"unknown version increment: {part}")


def write_version(new_version: str) -> None:
  cargo_path = ROOT / "Cargo.toml"
  cargo_text, count = re.subn(
    r'(?m)^version = "\d+\.\d+\.\d+"$',
    f'version = "{new_version}"',
    cargo_path.read_text(encoding="utf-8"),
    count=1,
  )
  if count != 1:
    raise ValueError("could not update Cargo.toml package version")
  (ROOT / "VERSION").write_text(f"{new_version}\n", encoding="utf-8")
  cargo_path.write_text(cargo_text, encoding="utf-8")


def ensure_clean_main() -> None:
  if run("git", "rev-parse", "--show-toplevel", capture=True) != str(ROOT):
    raise ValueError("run from this repository")
  if run("git", "branch", "--show-current", capture=True) != "main":
    raise ValueError("release from main")
  if run("git", "status", "--porcelain", capture=True):
    raise ValueError("commit working-tree changes first")
  run("git", "fetch", "origin", "main", "--tags")
  if run("git", "rev-parse", "HEAD", capture=True) != run(
    "git", "rev-parse", "origin/main", capture=True
  ):
    raise ValueError("local main is not synchronized with origin/main")


def main() -> None:
  parser = argparse.ArgumentParser(description=__doc__)
  selection = parser.add_mutually_exclusive_group(required=True)
  selection.add_argument("--current", action="store_true", help="release VERSION as-is")
  selection.add_argument("--bump", choices=("patch", "minor", "major"))
  parser.add_argument("--yes", action="store_true")
  args = parser.parse_args()
  ensure_clean_main()
  current = version()
  selected = current if args.current else bump_version(current, args.bump)
  tag = f"v{selected}"
  if subprocess.run(
    ("git", "rev-parse", "--verify", "--quiet", f"refs/tags/{tag}"),
    cwd=ROOT,
    check=False,
    capture_output=True,
  ).returncode == 0:
    raise ValueError(f"tag already exists: {tag}")
  if not args.yes and input(f"Build and publish {tag}? [y/N] ").lower() not in (
    "y", "yes"
  ):
    return

  run("cargo", "fmt", "--check")
  run("python3", "-m", "unittest", "discover", "-s", "tools", "-p", "test_*.py")
  run("cargo", "test", "--locked")
  run("cargo", "skyline", "check")
  run("cargo", "skyline", "build", "--release")
  run("python3", "tools/verify_nro.py", NRO)
  if args.current:
    run("git", "commit", "--allow-empty", "-m", f"chore: release {tag}")
  else:
    version_paths = [ROOT / name for name in ("VERSION", "Cargo.toml", "Cargo.lock")]
    original_contents = {path: path.read_bytes() for path in version_paths}
    try:
      write_version(selected)
      run("cargo", "check")
      run("cargo", "skyline", "build", "--release")
      run("python3", "tools/verify_nro.py", NRO)
    except BaseException:
      for path, contents in original_contents.items():
        path.write_bytes(contents)
      raise
    run("git", "add", "VERSION", "Cargo.toml", "Cargo.lock")
    run("git", "commit", "-m", f"chore: release {tag}")
  run("git", "push", "origin", "main")
  run("git", "tag", "-a", tag, "-m", tag)
  run("git", "push", "origin", tag)
  print(f"Published {tag}. Verify its release workflow and downloadable assets.")


if __name__ == "__main__":
  main()
