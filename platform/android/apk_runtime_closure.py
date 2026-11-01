#!/usr/bin/env python3
"""APK-level runtime closure gate for the Android client.

This test inspects build artifacts rather than the JVM test classpath, because
the Robolectric classpath contains Guava (as a test dependency) and therefore
cannot detect a missing runtime ``ListenableFuture`` in the APK.

Two independent checks run:

1. DEX definition check: the packaged ``classes*.dex`` must *define* the classes
   that ``androidx.profileinstaller`` / ``androidx.concurrent.futures`` need at
   runtime. Parsing DEX ``class_defs`` proves a definition; searching the string
   table does not, because a missing class is still referenced by name there.
2. Closure check: every class referenced from the deploy jar (the exact set of
   classes D8 receives) must be defined in that jar or be a platform/optional
   class. This catches *any* missing transitive runtime dependency, not only
   the one that caused the ProfileInstaller crash.

The ``--self-test`` mode mutates a copy of the real DEX so that a required
descriptor remains referenced by the string table but loses its ``class_def``.
The definition check must reject it. This runs as part of the Bazel test so the
gate cannot silently degrade into a reference-only check.
"""

from __future__ import annotations

import re
import sys
import tempfile
import zipfile
from pathlib import Path

# Classes that must be DEFINED in the packaged DEX. Keep this list focused on
# the startup/profileinstaller path; the closure check covers everything else.
REQUIRED_DEX_CLASSES = (
    "Lcom/google/common/util/concurrent/ListenableFuture;",
    "Landroidx/concurrent/futures/AbstractResolvableFuture;",
    "Landroidx/concurrent/futures/CallbackToFutureAdapter;",
    "Landroidx/profileinstaller/ProfileInstaller;",
)

# Platform classes are supplied by the Android runtime / JDK, not by the APK.
PLATFORM_PREFIXES = (
    "java.",
    "javax.",
    "jdk.",
    "sun.",
    "com.sun.",
    "android.",
    "dalvik.",
    "org.xml.",
    "org.xmlpull.",
    "org.w3c.",
    "org.json.",
    "org.apache.",
    "libcore.",
)

# Optional/compile-only classes: annotation processors, and TLS providers that
# OkHttp probes reflectively and uses only when present.
OPTIONAL_PREFIXES = (
    "com.android.tools.r8.annotations.",
    "com.google.auto.value.",
    "com.google.errorprone.annotations.",
    "com.google.j2objc.annotations.",
    "org.checkerframework.",
    "org.bouncycastle.",
    "org.conscrypt.",
    "org.openjsse.",
)

# Stale constant-pool entries in `firebase-components`: the published
# `Component$Builder.class` lists these two nested classes, but neither the
# artifact nor any caller defines or resolves them (the build uses primitive
# flags). Firebase is the only source of these names; ignoring them keeps the
# closure check focused on classes that can actually fail at runtime.
OPTIONAL_CLASSES = frozenset((
    "com.google.firebase.components.Component$ComponentType",
    "com.google.firebase.components.Component$Instantiation",
))

_DEX_MAGIC = b"dex\n"
_DESCRIPTOR_CLASS = re.compile(r"L([A-Za-z0-9_$/]+);")


def _read_u4(data: bytes, offset: int) -> int:
    return int.from_bytes(data[offset : offset + 4], "little")


def _read_u2_be(data: bytes, offset: int) -> int:
    return int.from_bytes(data[offset : offset + 2], "big")


def _read_uleb128(data: bytes, offset: int) -> tuple[int, int]:
    result = 0
    shift = 0
    while True:
        byte = data[offset]
        offset += 1
        result |= (byte & 0x7F) << shift
        if not byte & 0x80:
            return result, offset
        shift += 7


def _dex_defined_types(data: bytes) -> set[str]:
    """Return every type descriptor with a class_def entry in this DEX."""
    if data[:4] != _DEX_MAGIC:
        raise ValueError("not a DEX file")
    string_ids_size = _read_u4(data, 0x38)
    string_ids_off = _read_u4(data, 0x3C)
    type_ids_size = _read_u4(data, 0x40)
    type_ids_off = _read_u4(data, 0x44)
    class_defs_size = _read_u4(data, 0x60)
    class_defs_off = _read_u4(data, 0x64)
    if type_ids_off + 4 * type_ids_size > len(data) or class_defs_off + 32 * class_defs_size > len(data):
        raise ValueError("DEX section offsets are out of bounds")

    def string_at(string_id: int) -> str:
        if string_id >= string_ids_size:
            raise ValueError("string id out of range")
        data_off = _read_u4(data, string_ids_off + 4 * string_id)
        _, cursor = _read_uleb128(data, data_off)  # utf16 length
        end = data.index(b"\x00", cursor)
        return data[cursor:end].decode("utf-8", errors="replace")

    def descriptor(type_id: int) -> str:
        if type_id >= type_ids_size:
            raise ValueError("type id out of range")
        return string_at(_read_u4(data, type_ids_off + 4 * type_id))

    defined: set[str] = set()
    for index in range(class_defs_size):
        class_idx = _read_u4(data, class_defs_off + 32 * index)
        defined.add(descriptor(class_idx))
    return defined


def _dex_class_def_offsets(data: bytes) -> dict[str, int]:
    """Map defined descriptor -> offset of its class_def entry."""
    string_ids_off = _read_u4(data, 0x3C)
    type_ids_off = _read_u4(data, 0x44)
    class_defs_size = _read_u4(data, 0x60)
    class_defs_off = _read_u4(data, 0x64)

    def string_at(string_id: int) -> str:
        data_off = _read_u4(data, string_ids_off + 4 * string_id)
        _, cursor = _read_uleb128(data, data_off)
        end = data.index(b"\x00", cursor)
        return data[cursor:end].decode("utf-8", errors="replace")

    result: dict[str, int] = {}
    for index in range(class_defs_size):
        entry = class_defs_off + 32 * index
        # Resolve class_idx -> type_ids[class_idx] -> string_ids[...]
        class_idx = _read_u4(data, entry)
        descriptor_idx = _read_u4(data, type_ids_off + 4 * class_idx)
        result.setdefault(string_at(descriptor_idx), entry)
    return result


def _apk_dex_bytes(apk: Path) -> list[tuple[str, bytes]]:
    with zipfile.ZipFile(apk) as archive:
        names = sorted(n for n in archive.namelist() if re.fullmatch(r"classes\d*\.dex", n))
        return [(name, archive.read(name)) for name in names]


def check_dex(apk: Path) -> list[str]:
    dexes = _apk_dex_bytes(apk)
    if not dexes:
        return [f"{apk}: no classes*.dex entries found"]
    defined: set[str] = set()
    for name, data in dexes:
        try:
            defined.update(_dex_defined_types(data))
        except ValueError as exc:
            return [f"{apk}:{name}: cannot parse DEX class_defs: {exc}"]
    return [f"DEX in {apk} does not define {descriptor}" for descriptor in REQUIRED_DEX_CLASSES if descriptor not in defined]


def self_test(apk: Path) -> list[str]:
    """Prove the DEX check rejects a referenced-but-undefined required class."""
    failures: list[str] = []
    dexes = _apk_dex_bytes(apk)
    if not dexes:
        return [f"{apk}: no classes*.dex entries found for self-test"]
    descriptor = REQUIRED_DEX_CLASSES[0]
    for name, data in dexes:
        offsets = _dex_class_def_offsets(data)
        if descriptor not in offsets:
            continue
        # Re-point this class_def at another already-defined type. The
        # descriptor string stays in string_ids (still referenced), but no
        # class_def defines it any more.
        target = next((d for d in offsets if d != descriptor), None)
        if target is None:
            return [f"{apk}:{name}: self-test needs a second defined class"]
        type_ids_off = _read_u4(data, 0x44)
        string_ids_off = _read_u4(data, 0x3C)
        entry = offsets[descriptor]

        # Find the type index whose descriptor is `target`.
        target_type_idx = None
        for type_idx in range((_read_u4(data, 0x40))):
            string_id = _read_u4(data, type_ids_off + 4 * type_idx)
            data_off = _read_u4(data, string_ids_off + 4 * string_id)
            _, cursor = _read_uleb128(data, data_off)
            end = data.index(b"\x00", cursor)
            if data[cursor:end].decode("utf-8", errors="replace") == target:
                target_type_idx = type_idx
                break
        if target_type_idx is None:
            return [f"{apk}:{name}: self-test could not find target type"]
        mutated = bytearray(data)
        mutated[entry : entry + 4] = target_type_idx.to_bytes(4, "little")
        if descriptor.encode("utf-8") not in mutated:
            failures.append(f"self-test setup: {descriptor} is no longer referenced in {name}")
            continue
        try:
            still_defined = _dex_defined_types(bytes(mutated))
        except ValueError as exc:
            failures.append(f"self-test could not parse mutated DEX: {exc}")
            continue
        if descriptor in still_defined:
            failures.append(f"self-test: mutated DEX still defines {descriptor}")
            continue
        # Run the real gate entry point on an APK that carries the mutated DEX.
        # The descriptor is still present in the string table and in references,
        # so only a class_def-based check can reject it.
        with tempfile.TemporaryDirectory() as temp_dir:
            mutated_apk = Path(temp_dir) / "mutated.apk"
            with zipfile.ZipFile(apk) as source, zipfile.ZipFile(mutated_apk, "w", zipfile.ZIP_DEFLATED) as target_zip:
                for info in source.infolist():
                    payload = bytes(mutated) if info.filename == name else source.read(info.filename)
                    target_zip.writestr(info, payload)
            gate_failures = check_dex(mutated_apk)
        if not any(descriptor in failure and "does not define" in failure for failure in gate_failures):
            failures.append(
                f"self-test: check_dex accepted referenced-but-undefined {descriptor}: {gate_failures}"
            )
        return failures
    failures.append(f"self-test: required class {descriptor} not found in {apk}")
    return failures


def _constant_pool_classes(data: bytes) -> tuple[set[str], set[str]]:
    """Return (class refs, descriptor class refs) declared by a class file."""
    if data[:4] != b"\xca\xfe\xba\xbe":
        return set(), set()

    count = _read_u2_be(data, 8)
    offset = 10
    utf8: dict[int, str] = {}
    class_name_indices: list[int] = []
    descriptor_indices: list[int] = []
    index = 1
    while index < count:
        tag = data[offset]
        offset += 1
        if tag == 1:  # CONSTANT_Utf8
            length = _read_u2_be(data, offset)
            offset += 2
            raw = data[offset : offset + length]
            offset += length
            utf8[index] = raw.decode("utf-8", errors="replace")
        elif tag in (3, 4):  # Integer, Float
            offset += 4
        elif tag in (5, 6):  # Long, Double (take two slots)
            offset += 8
            index += 1
        elif tag == 7:  # Class
            class_name_indices.append(_read_u2_be(data, offset))
            offset += 2
        elif tag == 8:  # String
            offset += 2
        elif tag in (9, 10, 11):  # Fieldref, Methodref, InterfaceMethodref
            offset += 4
        elif tag == 12:  # NameAndType
            offset += 2
            descriptor_indices.append(_read_u2_be(data, offset))
            offset += 2
        elif tag == 15:  # MethodHandle
            offset += 3
        elif tag == 16:  # MethodType
            offset += 2
        elif tag in (17, 18):  # Dynamic, InvokeDynamic
            offset += 4
        elif tag in (19, 20):  # Module, Package
            offset += 2
        else:
            raise ValueError(f"unknown constant pool tag {tag} at {offset}")
        index += 1

    return (
        {utf8[i] for i in class_name_indices if i in utf8},
        {utf8[i] for i in descriptor_indices if i in utf8},
    )


def _normalise(name: str) -> str:
    name = name.replace("/", ".")
    if name.startswith("["):
        name = name.lstrip("[")
        if not name.startswith("L"):
            # Array of primitives (`[B`, `[[I`, ...): not a class dependency.
            return ""
        name = name[1:-1] if name.endswith(";") else name[1:]
    if name.startswith("L") and name.endswith(";"):
        name = name[1:-1]
    if len(name) <= 1 or "." not in name:
        return ""
    return name


def _allowed(name: str) -> bool:
    return name in OPTIONAL_CLASSES or name.startswith(PLATFORM_PREFIXES + OPTIONAL_PREFIXES)


def check_deploy_jar(deploy_jar: Path) -> list[str]:
    with zipfile.ZipFile(deploy_jar) as archive:
        defined: set[str] = set()
        payloads: list[bytes] = []
        for name in archive.namelist():
            if not name.endswith(".class") or name.startswith("META-INF/"):
                continue
            defined.add(_normalise(name[: -len(".class")]))
            payloads.append(archive.read(name))

    referenced: set[str] = set()
    for payload in payloads:
        try:
            classes, descriptors = _constant_pool_classes(payload)
        except ValueError as exc:
            return [f"cannot parse class in {deploy_jar}: {exc}"]
        for name in classes:
            referenced.add(_normalise(name))
        for descriptor in descriptors:
            for match in _DESCRIPTOR_CLASS.findall(descriptor):
                referenced.add(_normalise(match))

    # No inner-class exception: a referenced Foo$Bar must be defined even when
    # Foo is present. Missing inner classes are real runtime failures.
    missing = sorted(name for name in referenced if name and not _allowed(name) and name not in defined)
    return [f"runtime closure is missing {name}" for name in missing]


def main(argv: list[str]) -> int:
    if len(argv) == 3 and argv[1] == "--self-test":
        apk = Path(argv[2])
        failures = self_test(apk)
        if failures:
            print("DEX definition self-test failed:", file=sys.stderr)
            for failure in failures:
                print(f"  - {failure}", file=sys.stderr)
            return 1
        print("DEX definition self-test OK: referenced-but-undefined class is rejected.")
        return 0
    if len(argv) != 3:
        print("usage: apk_runtime_closure.py <app.apk> <app_deploy.jar>", file=sys.stderr)
        return 2
    apk = Path(argv[1])
    deploy_jar = Path(argv[2])
    failures = check_dex(apk) + check_deploy_jar(deploy_jar)
    if failures:
        print("APK runtime closure check failed:", file=sys.stderr)
        for failure in failures:
            print(f"  - {failure}", file=sys.stderr)
        return 1
    print(
        "APK runtime closure OK: required classes are defined in DEX, "
        "no unresolved non-platform references in the deploy jar."
    )
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv))
