#!/usr/bin/env python3
"""Writes core/licenses/libraries.txt: the licenses of the libraries the apps
are built with (the Rust crates of desktop and mobile, the Android libraries),
shown at the bottom of the "third-party licenses" screen.

Each license text appears once, followed by the libraries under it with their
copyright lines. Run it after changing dependencies; `--check` fails when the
file is out of date (CI).

    python3 scripts/licenses.py [--check]
"""

import json
import pathlib
import re
import subprocess
import sys

ROOT = pathlib.Path(__file__).resolve().parent.parent
OUT = ROOT / "core" / "licenses" / "libraries.txt"
OURS = {"scout4all-core", "scout4all-desktop", "scout4all-mobile", "scout4all-keys"}
DESKTOP_TARGETS = ["aarch64-apple-darwin", "x86_64-apple-darwin", "x86_64-pc-windows-msvc",
                   "x86_64-pc-windows-gnu", "x86_64-unknown-linux-gnu",
                   "aarch64-unknown-linux-gnu"]
# preferred among alternatives ("MIT OR Apache-2.0"): the shortest texts first
PREFERRED = ["MIT", "Apache-2.0", "BSD-3-Clause", "BSD-2-Clause", "ISC", "Zlib",
             "Unlicense", "CC0-1.0", "MIT-0", "0BSD", "BSL-1.0", "Unicode-3.0"]

MIT = """Permission is hereby granted, free of charge, to any person obtaining a copy
of this software and associated documentation files (the "Software"), to deal
in the Software without restriction, including without limitation the rights
to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
copies of the Software, and to permit persons to whom the Software is
furnished to do so, subject to the following conditions:

The above copyright notice and this permission notice shall be included in all
copies or substantial portions of the Software.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
SOFTWARE."""

# dedications to the public domain: nothing to carry, named for completeness
PUBLIC_DOMAIN = {i: f"{i}: dedicated to the public domain, no conditions."
                 for i in ["CC0-1.0", "Unlicense", "0BSD", "MIT-0"]}

# The Android libraries (releaseRuntimeClasspath, by group): all Apache-2.0
# but the Checker Framework annotations (MIT); JNA is Apache-2.0 OR LGPL.
ANDROID_APACHE = [
    "AndroidX (activity, annotation, appcompat, compose, core, lifecycle, media3, "
    "navigation, room, sqlite, work and the others they use) - The Android Open "
    "Source Project",
    "Kotlin standard library, kotlinx coroutines, JetBrains annotations - JetBrains s.r.o.",
    "Guava, Error Prone annotations, J2ObjC annotations, JSR 305 - Google LLC",
    "JSpecify - The JSpecify Authors",
    "JNA (Java Native Access) - Timothy Wall and contributors",
]
ANDROID_MIT = ["Checker Framework qualifiers - Copyright 2004-present by the "
               "Checker Framework developers"]


def run(*args):
    return subprocess.run(args, cwd=ROOT, check=True, capture_output=True, text=True).stdout


def shipped_crates():
    """name -> versions of the crates in the released builds: the desktop app
    on its targets (mobile comes with it), the Android library on its ABI."""
    desktop = [a for t in DESKTOP_TARGETS for a in ("--target", t)]
    trees = [["-p", "scout4all-desktop", *desktop],
             ["-p", "scout4all-mobile", "--target", "aarch64-linux-android"]]
    crates = {}
    for tree in trees:
        out = run("cargo", "tree", "-e", "normal", "--all-features", *tree,
                  "--prefix", "none", "-f", "{p}")
        for m in (re.match(r"(\S+) v(\S+)", line) for line in out.splitlines()):
            if m and m.group(1) not in OURS:
                crates.setdefault(m.group(1), set()).add(m.group(2))
    return crates


def choose(expression):
    """The licenses to honour: one alternative of an OR, all the parts of an AND."""
    expr = expression.replace("/", " OR ")
    if " AND " in expr and not expr.startswith("("):
        return [i for part in expr.split(" AND ") for i in choose(part.strip(" ()"))]
    if " AND " in expr:
        inner, _, rest = expr[1:].partition(")")
        return choose(inner) + choose(rest.removeprefix(" AND "))
    options = [o.strip(" ()") for o in expr.split(" OR ")]
    ranked = sorted(options, key=lambda o: PREFERRED.index(o) if o in PREFERRED else 99)
    return [ranked[0].split(" WITH ")[0]]


def license_files(directory):
    return sorted(p for p in directory.iterdir()
                  if p.is_file() and re.match(r"(LICEN[CS]E|COPYING|COPYRIGHT|NOTICE|UNLICENSE)",
                                              p.name, re.I))


def copyrights(files):
    lines = []
    for f in files:
        for line in f.read_text(errors="replace").splitlines():
            line = line.strip(" *#/")
            if (re.match(r"(Copyright\b\s*(\(c\)|©|\d)|Copyright \w|\(c\)\s*\d|©\s*\d)", line, re.I)
                    and not re.search(r"[\[{<](yyyy|year)|owner|notice|licen[cs]e|holders|\bthe above\b",
                                      line, re.I)
                    and line not in lines):
                lines.append(line)
    return lines


def text_for(license_id, files):
    """The text of a license as the crate carries it, else empty."""
    key = {"Apache-2.0": "APACHE"}.get(license_id, license_id.split("-")[0].upper())
    texts = [f for f in files if "NOTICE" not in f.name.upper()]
    matching = [f for f in texts if key in f.name.upper()] or (texts if len(texts) == 1 else [])
    return matching[0].read_text(errors="replace").strip() if matching else ""


def canonical(license_id, packages):
    """The text of a license from another crate under it alone, for a crate
    that names its license without carrying the text."""
    texts = (text_for(license_id, license_files(pathlib.Path(p["manifest_path"]).parent))
             for _, p in sorted(packages.items()) if p.get("license") == license_id)
    return next((t for t in texts if t), "")


def bare(text):
    """A license text without its copyright lines and spacing, to tell when
    two crates carry the same license."""
    lines = (l for l in text.splitlines() if not re.match(r"\W*(Copyright|\(c\)|©)", l.strip(), re.I))
    return " ".join(" ".join(lines).split())


def main():
    meta = json.loads(run("cargo", "metadata", "--all-features", "--format-version", "1"))
    packages = {(p["name"], p["version"]): p for p in meta["packages"]}
    apache = next(t for (n, _), p in sorted(packages.items()) if n == "serde"
                  for t in [text_for("Apache-2.0", license_files(pathlib.Path(p["manifest_path"]).parent))]
                  if "END OF TERMS" in t)
    groups = {}  # license text, copyright lines aside -> heading, text, libraries
    notices = []
    for name, versions in sorted(shipped_crates().items()):
        newest = max(versions, key=lambda v: [int(x) if x.isdigit() else 0 for x in re.split(r"[.+-]", v)])
        package = packages[(name, newest)]
        files = license_files(pathlib.Path(package["manifest_path"]).parent)
        authors = [re.sub(r"\s*<[^>]*>", "", a) for a in package.get("authors") or []]
        owners = copyrights(files) or [", ".join(authors or [f"the {name} authors"])]
        library = f"{name} {', '.join(sorted(versions))}"
        for license_id in choose(package.get("license") or "unknown"):
            text = ({"MIT": MIT, "Apache-2.0": apache}.get(license_id)
                    or text_for(license_id, files) or canonical(license_id, packages)
                    or PUBLIC_DOMAIN.get(license_id, ""))
            if not text:
                sys.exit(f"{name} {newest}: no text for {license_id}")
            ids, _, libraries = groups.setdefault(bare(text), (set(), text, []))
            ids.add(license_id)
            if library not in (l for l, _ in libraries):
                libraries.append((library, owners))
        notices += [f"{name} {newest}:\n{f.read_text(errors='replace').strip()}"
                    for f in files if "NOTICE" in f.name.upper()]
    groups[bare(apache)][2].extend((l, []) for l in ANDROID_APACHE)
    groups[bare(MIT)][2].extend((l, []) for l in ANDROID_MIT)

    parts = []
    for ids, text, libraries in sorted(groups.values(), key=lambda g: (-len(g[2]), sorted(g[0]))):
        heading = ", ".join(sorted(ids))
        listed = "\n".join(f"- {lib}" + "".join(f"\n    {c}" for c in owners)
                           for lib, owners in libraries)
        parts.append(f"=== {heading} ===\n\n{listed}\n\n{text}")
    if notices:
        parts.append("=== NOTICE ===\n\n" + "\n\n".join(notices))
    body = "\n\n\n".join(parts) + "\n"

    match sys.argv[1:]:
        case ["--check"]:
            if not OUT.exists() or OUT.read_text() != body:
                sys.exit(f"{OUT.relative_to(ROOT)} is out of date: run scripts/licenses.py")
        case _:
            OUT.parent.mkdir(exist_ok=True)
            OUT.write_text(body)
            print(f"{OUT.relative_to(ROOT)}: {len(body) // 1024} KB, {len(groups)} licenses")


main()
