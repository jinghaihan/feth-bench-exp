#!/usr/bin/env python3

"""Check the basic NRO structure before distributing a Skyline plugin."""

import argparse
import struct
from pathlib import Path


def read_u32(data: bytes, offset: int) -> int:
  return struct.unpack_from("<I", data, offset)[0]


def verify_nro(path: Path) -> None:
  data = path.read_bytes()
  if len(data) < 0x80 or data[0x10:0x14] != b"NRO0":
    raise ValueError("NRO0 header is missing")
  if read_u32(data, 0x18) != len(data):
    raise ValueError("declared NRO size does not match the file")

  previous_end = 0
  for name, header_offset in (("text", 0x20), ("rodata", 0x28), ("data", 0x30)):
    offset, size = struct.unpack_from("<II", data, header_offset)
    if size == 0 or offset < previous_end or offset + size > len(data):
      raise ValueError(f"invalid {name} segment")
    previous_end = offset + size

  mod_offset = read_u32(data, 0x04)
  if mod_offset + 4 > len(data) or data[mod_offset:mod_offset + 4] != b"MOD0":
    raise ValueError("MOD0 header is missing")
  if not any(data[0x40:0x60]):
    raise ValueError("NRO Build ID is empty")
  print(f"verified {path} ({len(data)} bytes)")


def main() -> None:
  parser = argparse.ArgumentParser(description=__doc__)
  parser.add_argument("nro", type=Path)
  args = parser.parse_args()
  verify_nro(args.nro)


if __name__ == "__main__":
  main()
