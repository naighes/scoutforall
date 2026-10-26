#!/usr/bin/env python3
"""The icon (512x512) and the feature graphic (1024x500) of the Play listing,
from the vectors of the app: the logo of the launcher icon and the wordmark,
white on the surface color, in 32-bit PNG as Play asks. Needs rsvg-convert and
ImageMagick.

    python3 -I android/fastlane/graphics.py
"""

import re
import subprocess
from pathlib import Path

HERE = Path(__file__).resolve().parent
RES = HERE.parent / "app/src/main/res"
OUT = HERE / "metadata/android/en-US/images"
SURFACE = re.search(r'name="surface">#FF([0-9A-F]{6})<', (RES / "values/colors.xml").read_text()).group(1)


def paths(drawable):
    return re.findall(r'android:pathData="([^"]+)"', (RES / f"drawable/{drawable}.xml").read_text())


def viewport(drawable):
    text = (RES / f"drawable/{drawable}.xml").read_text()
    return tuple(float(re.search(rf'android:viewport{a}="([\d.]+)"', text).group(1)) for a in ("Width", "Height"))


# the logo in its own coordinates (the path of the site, 517 units wide)
LOGO = paths("ic_launcher_foreground")
LOGO_SIZE = 517.255
WORDMARK = paths("wordmark")
WORDMARK_W, WORDMARK_H = viewport("wordmark")


def group(path_data, x, y, scale):
    shapes = "".join(f'<path fill="#fff" d="{d}"/>' for d in path_data)
    return f'<g transform="translate({x:.2f} {y:.2f}) scale({scale:.5f})">{shapes}</g>'


def render(name, width, height, body):
    svg = (f'<svg xmlns="http://www.w3.org/2000/svg" width="{width}" height="{height}">'
           f'<rect width="100%" height="100%" fill="#{SURFACE}"/>{body}</svg>')
    OUT.mkdir(parents=True, exist_ok=True)
    png = subprocess.run(["rsvg-convert"], input=svg.encode(), check=True, capture_output=True).stdout
    subprocess.run(["magick", "png:-", "-define", "png:color-type=6", str(OUT / name)], input=png, check=True)


def icon():
    # Play cuts the corners itself: the logo keeps the margins of the
    # adaptive icon, a little larger
    size = 512 * 0.66
    render("icon.png", 512, 512, group(LOGO, (512 - size) / 2, (512 - size) / 2, size / LOGO_SIZE))


def feature_graphic():
    # the logo and the name side by side, in the middle: Play may lay a
    # button over the bottom, and crops the sides on some screens
    logo = 220
    gap = 48
    name_h = 96
    name_w = name_h * WORDMARK_W / WORDMARK_H
    x = (1024 - logo - gap - name_w) / 2
    render("featureGraphic.png", 1024, 500,
           group(LOGO, x, (500 - logo) / 2, logo / LOGO_SIZE)
           + group(WORDMARK, x + logo + gap, (500 - name_h) / 2, name_h / WORDMARK_H))


icon()
feature_graphic()
