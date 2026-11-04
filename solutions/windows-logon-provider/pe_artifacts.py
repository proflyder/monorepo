#!/usr/bin/env python3
"""PE checks for the Windows logon provider artifacts.

The build no longer ships a Windows linker toolchain on the host: Bazel builds
the DLL with rules_rust and the hermetic zig cc linker. This dependency-free
script replaces the old `llvm-objdump` verification and runs anywhere with
Python 3 (macOS and the Ubuntu CI runner):

    python3 pe_artifacts.py normalize out/RicardoKompLogon.dll out/cp-selftest.exe
    python3 pe_artifacts.py verify out/RicardoKompLogon.dll

`normalize` zeroes the PE timestamps (COFF header, export and debug directory
entries) that the linker stamps with the current time. Identical sources must
produce identical bytes: the OCI publisher refuses a second build of the same
VERSION with different content.

`verify` fails unless the DLL imports only the allowed system DLLs (static
CRT/libc++ expected) and exports exactly `DllGetClassObject` and
`DllCanUnloadNow`.
"""
from __future__ import annotations

import argparse
import struct
import sys
from pathlib import Path

# System DLLs the provider may import. Everything else (in particular a
# dynamically linked CRT or libc++) fails the check.
ALLOWED_DLLS = {
    "advapi32.dll",
    "bcryptprimitives.dll",
    "combase.dll",
    "crypt32.dll",
    "kernel32.dll",
    "ntdll.dll",
    "ole32.dll",
    "oleaut32.dll",
    "rpcrt4.dll",
    "secur32.dll",
    "shlwapi.dll",
    "userenv.dll",
    "winhttp.dll",
    "ws2_32.dll",
}
ALLOWED_PREFIXES = ("api-ms-win-",)
REQUIRED_EXPORTS = {"DllGetClassObject", "DllCanUnloadNow"}


class PEError(RuntimeError):
    pass


class PE:
    def __init__(self, path: Path) -> None:
        self.path = path
        self.data = bytearray(path.read_bytes())
        if self.data[:2] != b"MZ":
            raise PEError(f"{path}: not a DOS/PE image")
        self.elfanew = struct.unpack_from("<I", self.data, 0x3C)[0]
        if self.data[self.elfanew : self.elfanew + 4] != b"PE\0\0":
            raise PEError(f"{path}: missing PE signature")
        coff = self.elfanew + 4
        (_, self.section_count, _, _, _, self.optional_size, _) = struct.unpack_from(
            "<HHIIIHH", self.data, coff
        )
        optional = coff + 20
        magic = struct.unpack_from("<H", self.data, optional)[0]
        if magic == 0x20B:
            self.image_base = struct.unpack_from("<Q", self.data, optional + 24)[0]
            self.data_dir_offset = optional + 112
        elif magic == 0x10B:
            self.image_base = struct.unpack_from("<I", self.data, optional + 28)[0]
            self.data_dir_offset = optional + 96
        else:
            raise PEError(f"{path}: unsupported optional header magic {magic:#x}")
        (self.data_dir_count,) = struct.unpack_from("<I", self.data, self.data_dir_offset - 4)
        self.sections = []
        section_table = optional + self.optional_size
        for index in range(self.section_count):
            entry = section_table + index * 40
            name = self.data[entry : entry + 8].rstrip(b"\0").decode("ascii", "replace")
            (vsize, va, raw_size, raw_ptr) = struct.unpack_from("<IIII", self.data, entry + 8)
            self.sections.append((va, vsize, raw_ptr, raw_size))

    def rva_to_offset(self, rva: int) -> int:
        for va, vsize, raw_ptr, raw_size in self.sections:
            size = max(vsize, raw_size)
            if va <= rva < va + size:
                return raw_ptr + (rva - va)
        raise PEError(f"{self.path}: RVA {rva:#x} is not backed by a section")

    def read_c_string(self, rva: int) -> str:
        offset = self.rva_to_offset(rva)
        end = self.data.index(0, offset)
        return self.data[offset:end].decode("ascii", "replace")

    def data_directory(self, index: int) -> tuple[int, int]:
        if index >= self.data_dir_count:
            return (0, 0)
        offset = self.data_dir_offset + index * 8
        return struct.unpack_from("<II", self.data, offset)

    def imports(self) -> list[str]:
        rva, _ = self.data_directory(1)
        if rva == 0:
            return []
        offset = self.rva_to_offset(rva)
        names = []
        while True:
            descriptor = struct.unpack_from("<IIIII", self.data, offset)
            if not any(descriptor):
                break
            names.append(self.read_c_string(descriptor[3]))
            offset += 20
        return names

    def exports(self) -> list[str]:
        rva, size = self.data_directory(0)
        if rva == 0 or size == 0:
            return []
        offset = self.rva_to_offset(rva)
        number_of_names = struct.unpack_from("<I", self.data, offset + 24)[0]
        address_of_names = struct.unpack_from("<I", self.data, offset + 32)[0]
        names = []
        for index in range(number_of_names):
            name_rva = struct.unpack_from(
                "<I", self.data, self.rva_to_offset(address_of_names) + index * 4
            )[0]
            names.append(self.read_c_string(name_rva))
        return names

    def normalize(self) -> None:
        """Zero every timestamp the linker may derive from the wall clock."""
        struct.pack_into("<I", self.data, self.elfanew + 8, 0)
        for index in (0, 6):  # export directory, debug directory
            rva, size = self.data_directory(index)
            if rva == 0 or size == 0:
                continue
            offset = self.rva_to_offset(rva)
            if index == 0:
                struct.pack_into("<I", self.data, offset + 4, 0)
                continue
            for entry in range(offset, offset + size, 28):
                struct.pack_into("<I", self.data, entry + 4, 0)
        self.path.chmod(self.path.stat().st_mode | 0o200)
        self.path.write_bytes(self.data)


def normalize(paths: list[Path]) -> None:
    for path in paths:
        PE(path).normalize()
        print(f"normalized {path}")


def verify(path: Path) -> None:
    pe = PE(path)
    imports = sorted({name.lower() for name in pe.imports()})
    exports = set(pe.exports())

    unexpected = [
        name
        for name in imports
        if name not in ALLOWED_DLLS and not name.startswith(ALLOWED_PREFIXES)
    ]
    if unexpected:
        raise PEError(
            "unexpected DLL imports: "
            + ", ".join(unexpected)
            + "\nstatic CRT/libc++ expected; check the zig toolchain"
        )
    if exports != REQUIRED_EXPORTS:
        raise PEError(
            f"exports are {sorted(exports)}, expected {sorted(REQUIRED_EXPORTS)}"
        )

    print(f"{path}: imports")
    for name in imports:
        print(f"  {name}")
    print(f"{path}: exports")
    for name in sorted(exports):
        print(f"  {name}")


def main(argv: list[str]) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    subparsers = parser.add_subparsers(dest="command", required=True)
    normalize_parser = subparsers.add_parser("normalize")
    normalize_parser.add_argument("files", nargs="+", type=Path)
    verify_parser = subparsers.add_parser("verify")
    verify_parser.add_argument("file", type=Path)
    args = parser.parse_args(argv)

    try:
        if args.command == "normalize":
            normalize(args.files)
        else:
            verify(args.file)
    except (OSError, PEError, ValueError) as error:
        print(f"pe_artifacts: {error}", file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
