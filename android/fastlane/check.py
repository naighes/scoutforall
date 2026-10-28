#!/usr/bin/env python3
"""The Play listing of metadata/android/ within the limits of Play, before
the CI uploads it: the texts, the images and, with --release, the release
notes of the version of Cargo.toml in every language. Prints what is wrong
and fails, or prints nothing. With --version-code, only prints the
versionCode of Cargo.toml.

    python3 -I android/fastlane/check.py [--release | --version-code]
"""

import re
import struct
import sys
from pathlib import Path

HERE = Path(__file__).resolve().parent
METADATA = HERE / "metadata/android"
ROOT = HERE.parent.parent
CARGO = ROOT / "Cargo.toml"
DEFAULT = "en-US"
LANGUAGES = ["en-US", "it-IT", "pt-BR", "pl-PL", "ja-JP"]
# the longest text Play takes, in characters
TEXTS = {"title.txt": 30, "short_description.txt": 80, "full_description.txt": 4000}
RELEASE_NOTES = 500
SCREENSHOTS = (2, 8)
# the graphics of the default language, the same in the others
GRAPHICS = {"icon.png": (512, 512), "featureGraphic.png": (1024, 500)}


def version_code():
    """As app/build.gradle.kts: major, minor and patch in two digits each."""
    package = CARGO.read_text().split("[workspace.package]", 1)[1]
    version = re.search(r'(?m)^version\s*=\s*"(\d+)\.(\d+)\.(\d+)"', package)
    major, minor, patch = (int(part) for part in version.groups())
    return major * 10000 + minor * 100 + patch


def png_size(path):
    header = path.read_bytes()[:24]
    if header[:8] != b"\x89PNG\r\n\x1a\n":
        return None
    return struct.unpack(">II", header[16:24])


def text(path):
    # the newline closing the file is not part of the text
    return path.read_text(encoding="utf-8").rstrip("\n")


def check_text(path, limit):
    if not path.exists():
        return [f"{path}: missing"]
    content = text(path)
    if not content.strip():
        return [f"{path}: empty"]
    if len(content) > limit:
        return [f"{path}: {len(content)} characters, at most {limit}"]
    return []


def check_screenshot(path):
    size = png_size(path)
    if size is None:
        return [f"{path}: not a PNG"]
    short, long = sorted(size)
    if short < 320 or long > 3840 or long > 2 * short:
        return [f"{path}: {size[0]}x{size[1]}, sides from 320 to 3840 px, the long one at most twice the short one"]
    return []


def check_language(language, release):
    folder = METADATA / language
    problems = [p for name, limit in TEXTS.items() for p in check_text(folder / name, limit)]
    screenshots = sorted((folder / "images/phoneScreenshots").glob("*"))
    low, high = SCREENSHOTS
    if not low <= len(screenshots) <= high:
        problems.append(f"{folder}/images/phoneScreenshots: {len(screenshots)} screenshots, from {low} to {high}")
    problems += [p for s in screenshots for p in check_screenshot(s)]
    if release is not None:
        problems += check_text(folder / f"changelogs/{release}.txt", RELEASE_NOTES)
    return problems


def check_graphics():
    problems = []
    for name, expected in GRAPHICS.items():
        path = METADATA / DEFAULT / "images" / name
        size = png_size(path) if path.exists() else None
        if size != expected:
            problems.append(f"{path}: {size}, {expected[0]}x{expected[1]} PNG expected")
    return problems


def main():
    if "--version-code" in sys.argv[1:]:
        print(version_code())
        return
    release = version_code() if "--release" in sys.argv[1:] else None
    found = sorted(p.name for p in METADATA.iterdir() if p.is_dir())
    problems = [f"{METADATA}: languages {found}, expected {sorted(LANGUAGES)}"] if found != sorted(LANGUAGES) else []
    problems += check_graphics()
    problems += [p for language in LANGUAGES for p in check_language(language, release)]
    for problem in problems:
        print(problem.replace(f"{ROOT}/", ""))
    sys.exit(1 if problems else 0)


if __name__ == "__main__":
    main()
