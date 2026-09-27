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
  parser.add_argument("--current", action="store_true", required=True)
  parser.add_argument("--yes", action="store_true")
  args = parser.parse_args()
  ensure_clean_main()
  tag = f"v{version()}"
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
  run("cargo", "test", "--locked")
  run("cargo", "skyline", "check")
  run("cargo", "skyline", "build", "--release")
  run("python3", "tools/verify_nro.py", NRO)
  run("git", "commit", "--allow-empty", "-m", f"chore: release {tag}")
  run("git", "push", "origin", "main")
  run("git", "tag", "-a", tag, "-m", tag)
  run("git", "push", "origin", tag)
  print(f"Published {tag}. Verify its release workflow and downloadable assets.")


if __name__ == "__main__":
  main()
