"""Writes `src/catalog/pinned.rs`: the dictionaries of wooorm/dictionaries at the commit COMMIT,
with the size and the SHA-256 digest of each file.

Usage: python make_catalog.py FOLDER src/catalog/pinned.rs, where FOLDER holds a copy of the
`dictionaries` folder of that commit (each language folder with `index.aff`, `index.dic`, and
`license`, or `readme.md` for a dictionary without a license file)."""
import hashlib
import os
import sys

root, out = sys.argv[1], sys.argv[2]
SKIP = {"tlh", "tlh-Latn", "ie"}
TAGS = {"en": "en-US", "pt": "pt-BR"}

rows = []
for folder in sorted(os.listdir(root)):
    if folder in SKIP:
        continue
    license = "license" if os.path.exists(os.path.join(root, folder, "license")) else "readme.md"
    files = []
    for name in ["index.aff", "index.dic", license]:
        data = open(os.path.join(root, folder, name), "rb").read()
        files.append((name, len(data), hashlib.sha256(data).hexdigest()))
    rows.append((TAGS.get(folder, folder), folder, files))

lines = [
    "//! The dictionaries of `wooorm/dictionaries` at the commit [`COMMIT`], with the size and the",
    "//! SHA-256 digest of each file, from `make_catalog.py`.",
    "",
    "use crate::catalog::{CatalogEntry, PinnedFile};",
    "",
    "/// The commit of `wooorm/dictionaries` that each file comes from.",
    'pub(crate) const COMMIT: &str = "8cfea406b505e4d7df52d5a19bce525df98c54ab";',
    "",
    "/// Each dictionary, by its language.",
    f"pub(crate) const ENTRIES: [CatalogEntry; {len(rows)}] = [",
]
for tag, folder, files in rows:
    lines += ["    CatalogEntry {", f'        language: "{tag}",', f'        folder: "{folder}",', "        files: ["]
    for name, size, digest in files:
        lines.append(f'            PinnedFile {{ name: "{name}", size: {size:_}, sha256: "{digest}" }},')
    lines += ["        ],", "    },"]
lines += ["];", ""]
open(out, "w", encoding="utf-8", newline="\n").write("\n".join(lines))
print(len(rows), "dictionaries")
