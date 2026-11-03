#!/usr/bin/env python3
"""Writes the third-party software of each build of TypeAlike and its licenses, for the About pane.

Reads the dependency graph of the bin from `cargo metadata` and the license files from the
source of each package. Run it after each change of the dependencies:
python3 packaging/build_licenses.py
"""

import json
import re
import subprocess
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
BIN_PACKAGE = "typealike"
OUTPUT_DIR = ROOT / "crates/typealike/src/platform"
TARGETS = {
    "macos": "aarch64-apple-darwin",
    "windows": "x86_64-pc-windows-msvc",
}
OS_NAMES = {"macos": "macOS", "windows": "Windows"}
LICENSE_FILE = re.compile(r"^(licen[cs]e|copying|copyright|notice|unlicense)", re.IGNORECASE)
# A git checkout keeps the license files at the root of the repository, above the package.
MAX_PARENT_DIRS = 3
# llama-cpp-sys-2 has no license file of its own, but it builds the llama.cpp in its source.
EXTRA_LICENSE_FILES = {"llama-cpp-sys-2": ["llama.cpp/LICENSE"]}
# These packages ship no license file. Each file is the MIT license file of the repository,
# which each license of the packages allows.
REPOSITORY_LICENSE_FILES = {
    "https://github.com/madsmtm/objc2": "objc2-MIT.txt",
    "https://github.com/projectfluent/fluent-langneg-rs": "fluent-langneg-MIT.txt",
    "https://github.com/zbraniecki/pluralrules": "pluralrules-MIT.txt",
    "https://github.com/utilityai/llama-cpp-rs": "llama-cpp-rs-MIT.txt",
}
REPOSITORY_LICENSES_DIR = ROOT / "packaging/licenses"
# Software that the build has outside of Cargo, as (name, version, license, file).
EXTRA_SOFTWARE = {
    "windows": [
        ("Fluent UI System Icons", "", "MIT", ROOT / "crates/typealike_windows/assets/fluent/LICENSE"),
    ],
}


def main():
    for platform, target in TARGETS.items():
        libraries = third_party_libraries(target) + extra_libraries(platform)
        libraries.sort(key=lambda library: (library["name"].lower(), library["version"]))
        path = OUTPUT_DIR / f"{platform}_licenses.rs"
        path.write_text(rust_module(platform, libraries), encoding="utf-8")
        subprocess.run(["rustfmt", "--edition", "2024", str(path)], check=True, cwd=ROOT)
        print(f"{path.relative_to(ROOT)}: {len(libraries)} libraries")


def third_party_libraries(target):
    metadata = json.loads(
        subprocess.run(
            [
                "cargo", "metadata", "--format-version", "1", "--locked", "--all-features",
                "--filter-platform", target,
            ],
            check=True, capture_output=True, cwd=ROOT, text=True,
        ).stdout
    )
    packages = {package["id"]: package for package in metadata["packages"]}
    members = set(metadata["workspace_members"])
    root = next(id for id in members if packages[id]["name"] == BIN_PACKAGE)
    return [library_of(packages[id]) for id in linked_packages(metadata["resolve"], root) - members]


def linked_packages(resolve, root):
    """The packages that the build of `root` links: its normal dependencies, without the build and
    the dev dependencies."""
    nodes = {node["id"]: node for node in resolve["nodes"]}
    linked = set()
    pending = [root]
    while pending:
        id = pending.pop()
        if id in linked:
            continue
        linked.add(id)
        pending.extend(
            dependency["pkg"]
            for dependency in nodes[id]["deps"]
            if any(kind["kind"] is None for kind in dependency["dep_kinds"])
        )
    return linked


def library_of(package):
    package_dir = Path(package["manifest_path"]).parent
    files = (
        license_files(package_dir)
        or declared_license_file(package, package_dir)
        or repository_license_file(package)
    )
    files += [package_dir / name for name in EXTRA_LICENSE_FILES.get(package["name"], [])]
    if not files:
        raise SystemExit(f"no license file for {package['name']} {package['version']}")
    return {
        "name": package["name"],
        "version": package["version"],
        "license": package["license"] or "",
        "texts": [license_text(file) for file in files],
    }


def license_files(package_dir):
    is_git_checkout = "checkouts" in package_dir.parts
    searched = MAX_PARENT_DIRS + 1 if is_git_checkout else 1
    for directory in [package_dir, *package_dir.parents][:searched]:
        files = sorted(
            file for file in directory.iterdir() if file.is_file() and LICENSE_FILE.match(file.name)
        )
        if files:
            return files
    return []


def declared_license_file(package, package_dir):
    return [package_dir / package["license_file"]] if package["license_file"] else []


def repository_license_file(package):
    name = REPOSITORY_LICENSE_FILES.get(package["repository"])
    return [REPOSITORY_LICENSES_DIR / name] if name else []


def extra_libraries(platform):
    return [
        {"name": name, "version": version, "license": license, "texts": [license_text(file)]}
        for name, version, license, file in EXTRA_SOFTWARE.get(platform, [])
    ]


def license_text(file):
    return file.read_text(encoding="utf-8", errors="replace").lstrip("﻿").replace("\r\n", "\n").strip()


def rust_module(platform, libraries):
    texts = list(dict.fromkeys(text for library in libraries for text in library["texts"]))
    index = {text: number for number, text in enumerate(texts)}
    entries = "\n".join(
        f"""    Library {{
        name: {rust_string(library["name"])},
        version: {rust_string(library["version"])},
        license: {rust_string(library["license"])},
        texts: &[{", ".join(f"TEXT_{index[text]}" for text in library["texts"])}],
    }},"""
        for library in libraries
    )
    constants = "\n\n".join(f"const TEXT_{number}: &str = {rust_string(text)};" for number, text in enumerate(texts))
    return f"""//! The third-party software of the {OS_NAMES[platform]} build and its licenses. `packaging/build_licenses.py`
//! writes this file.

use typealike_core::settings_window::about::Library;

/// Each third-party library of the build, by name.
pub(crate) const LIBRARIES: &[Library] = &[
{entries}
];

{constants}
"""


def rust_string(text):
    escaped = []
    for character in text:
        if character == "\\":
            escaped.append("\\\\")
        elif character == '"':
            escaped.append('\\"')
        elif character in "\n\t":
            escaped.append(character)
        elif ord(character) < 0x20 or ord(character) == 0x7F:
            escaped.append(f"\\u{{{ord(character):x}}}")
        else:
            escaped.append(character)
    return '"' + "".join(escaped) + '"'


if __name__ == "__main__":
    main()
