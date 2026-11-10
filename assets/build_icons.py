#!/usr/bin/env python3
"""Makes the icon files of TypeAlike from logo.svg and the tray glyphs in tray/.

Needs macOS (iconutil, tiffutil) and resvg (`cargo install resvg` or `brew install resvg`).
Usage: python3 assets/build_icons.py
"""

import copy
import os
import shutil
import struct
import subprocess
import tempfile
import xml.etree.ElementTree as ET
from pathlib import Path

ASSETS = Path(__file__).resolve().parent
ROOT = ASSETS.parent
LOGO = ASSETS / "logo.svg"
SVG_NAMESPACE = "http://www.w3.org/2000/svg"

MAC_ICON = ROOT / "packaging/macos/TypeAlike.icns"
MAC_MENU_BAR = ROOT / "crates/typealike_macos/assets/menu_bar"
MAC_SETUP_LOGO = ROOT / "crates/typealike_macos/assets/logo.png"
WINDOWS_ICON = ROOT / "crates/typealike/TypeAlike.ico"
WINDOWS_SETUP_LOGO = ROOT / "crates/typealike_windows/assets/logo.png"
WINDOWS_TRAY = ROOT / "crates/typealike_windows/assets/tray"

MARK_PARTS = ["earL", "earR", "stem", "ball", "head", "visor", "eyes", "letterI"]
HEAD_PARTS = ["earL", "earR", "stem", "ball", "head", "visor", "eyes"]

MAC_CANVAS = 1024
MAC_PLATE = 824
MAC_PLATE_RADIUS = 185.4
MAC_MARK_WIDTH = 600
MAC_HEAD_WIDTH = 600
MAC_SMALL_POINTS = 32
MAC_ICON_POINTS = [16, 32, 128, 256, 512]
# The setup window shows the app icon at 64 points, from a bitmap at 2x.
MAC_SETUP_LOGO_POINTS = 64

WINDOWS_ICON_SIZES = [16, 20, 24, 32, 40, 48, 64, 256]
WINDOWS_SMALL_SIZE = 24
# The setup window shows the logo at 56 DIPs, from a bitmap of four times that size.
WINDOWS_SETUP_LOGO_SIZE = 224
FLUENT_KEYLINE = 44 / 48
WINDOWS_HEAD_STROKE = "#8EA6CC"
SMALL_EYE_STROKE = 54

TRAY_GRIDS = {16: 16, 20: 20, 24: 24, 32: 16, 40: 20, 48: 24, 64: 16}
TRAY_STATES = ["active", "paused"]
TRAY_COLORS = {"black": "#000000", "white": "#FFFFFF"}
# The caution fill of Fluent: the light theme color on a light taskbar, the dark theme color on a dark one.
STOPPED_COLORS = {"amber": "#9D5D00", "yellow": "#FCE100"}
MENU_BAR_POINTS = 16

APPLE_CORNER = [
    (1.52866483, 0),
    (1.08849296, 0),
    (0.86840694, 0),
    (0.63149399, 0.07491100),
    (0.37282392, 0.16905899),
    (0.16905899, 0.37282392),
    (0.07491100, 0.63149399),
    (0, 0.86840694),
    (0, 1.08849296),
    (0, 1.52866483),
]

ET.register_namespace("", SVG_NAMESPACE)


def tag(name):
    return f"{{{SVG_NAMESPACE}}}{name}"


def resvg_path():
    found = os.environ.get("RESVG") or shutil.which("resvg")
    if not found:
        raise SystemExit("resvg is missing: install it with `cargo install resvg` or `brew install resvg`")
    return found


class Renderer:
    def __init__(self, folder):
        self.folder = Path(folder)
        self.resvg = resvg_path()
        self.count = 0

    def png(self, svg_text, side):
        self.count += 1
        source = self.folder / f"{self.count}.svg"
        target = self.folder / f"{self.count}.png"
        source.write_text(svg_text)
        subprocess.run([self.resvg, "-w", str(side), "-h", str(side), str(source), str(target)], check=True)
        return target.read_bytes()

    def part_boxes(self, svg_path):
        output = subprocess.run(
            [self.resvg, "--query-all", str(svg_path)], check=True, capture_output=True, text=True
        ).stdout
        boxes = {}
        for line in output.splitlines():
            name, *numbers = line.split(",")
            boxes[name] = tuple(float(number) for number in numbers)
        return boxes


class Logo:
    def __init__(self, renderer):
        root = ET.parse(LOGO).getroot()
        self.defs = ET.tostring(root.find(tag("defs")), encoding="unicode")
        self.parts = {element.get("id"): element for element in root if element.get("id")}
        self.boxes = renderer.part_boxes(LOGO)

    def bounds(self, names):
        boxes = [self.boxes[name] for name in names]
        left = min(box[0] for box in boxes)
        top = min(box[1] for box in boxes)
        right = max(box[0] + box[2] for box in boxes)
        bottom = max(box[1] + box[3] for box in boxes)
        return left, top, right - left, bottom - top

    def group(self, names, center, width, head_stroke=None, eye_stroke=None):
        left, top, part_width, part_height = self.bounds(names)
        scale = width / part_width
        x = center[0] - (left + part_width / 2) * scale
        y = center[1] - (top + part_height / 2) * scale
        elements = []
        for name in names:
            element = copy.deepcopy(self.parts[name])
            if name == "head" and head_stroke:
                element.set("stroke", head_stroke)
            if name == "eyes" and eye_stroke:
                element.set("stroke-width", str(eye_stroke))
            elements.append(ET.tostring(element, encoding="unicode"))
        return f'<g transform="translate({x:.3f} {y:.3f}) scale({scale:.5f})">{"".join(elements)}</g>'

    def fitted_width(self, names, side):
        _, _, width, height = self.bounds(names)
        return side * min(1, width / height)


def svg_document(side, defs, body):
    return (
        f'<svg xmlns="{SVG_NAMESPACE}" width="{side}" height="{side}" viewBox="0 0 {side} {side}">'
        f"<defs>{defs}</defs>{body}</svg>"
    )


def continuous_rounded_rect(x, y, width, height, radius):
    right, bottom = x + width, y + height
    corners = [
        lambda a, b: (right - a * radius, y + b * radius),
        lambda a, b: (right - b * radius, bottom - a * radius),
        lambda a, b: (x + a * radius, bottom - b * radius),
        lambda a, b: (x + b * radius, y + a * radius),
    ]
    commands = []
    for corner in corners:
        points = [corner(a, b) for a, b in APPLE_CORNER]
        commands.append(("M" if not commands else "L") + "{:.3f} {:.3f}".format(*points[0]))
        for start in (1, 4, 7):
            commands.append("C" + " ".join("{:.3f} {:.3f}".format(*point) for point in points[start : start + 3]))
    return "".join(commands) + "Z"


def mac_icon_svg(logo, is_small):
    inset = (MAC_CANVAS - MAC_PLATE) / 2
    plate = continuous_rounded_rect(inset, inset, MAC_PLATE, MAC_PLATE, MAC_PLATE_RADIUS)
    defs = (
        logo.defs
        + '<linearGradient id="plate" x1="0" y1="0" x2="0" y2="1">'
        '<stop offset="0" stop-color="#2A4369"/><stop offset="1" stop-color="#101A2F"/></linearGradient>'
        '<filter id="plateShadow" x="-10%" y="-10%" width="120%" height="125%">'
        '<feDropShadow dx="0" dy="10" stdDeviation="10" flood-color="#000" flood-opacity="0.3"/></filter>'
    )
    center = (MAC_CANVAS / 2, MAC_CANVAS / 2)
    if is_small:
        mark = logo.group(HEAD_PARTS, center, MAC_HEAD_WIDTH, eye_stroke=SMALL_EYE_STROKE)
    else:
        mark = logo.group(MARK_PARTS, center, MAC_MARK_WIDTH)
    body = f'<path d="{plate}" fill="url(#plate)" filter="url(#plateShadow)"/>{mark}'
    return svg_document(MAC_CANVAS, defs, body)


def windows_icon_svg(logo, side):
    is_small = side <= WINDOWS_SMALL_SIZE
    names = HEAD_PARTS if is_small else MARK_PARTS
    width = logo.fitted_width(names, side * FLUENT_KEYLINE)
    eye_stroke = SMALL_EYE_STROKE if is_small else None
    mark = logo.group(names, (side / 2, side / 2), width, WINDOWS_HEAD_STROKE, eye_stroke)
    return svg_document(side, logo.defs, mark)


def tray_svg(grid, state, color):
    root = ET.parse(ASSETS / "tray" / f"tray-{grid}.svg").getroot()
    for group in root.iter(tag("g")):
        if group.get("id") in TRAY_STATES:
            group.set("display", "inline" if group.get("id") == state else "none")
    red, green, blue = (int(color[index : index + 2], 16) / 255 for index in (1, 3, 5))
    glyph = "".join(ET.tostring(element, encoding="unicode") for element in root)
    defs = (
        '<filter id="color" color-interpolation-filters="sRGB">'
        f'<feColorMatrix type="matrix" values="0 0 0 0 {red:g} 0 0 0 0 {green:g} 0 0 0 0 {blue:g} 0 0 0 1 0"/>'
        "</filter>"
    )
    return (
        f'<svg xmlns="{SVG_NAMESPACE}" width="{grid}" height="{grid}" viewBox="0 0 {grid} {grid}">'
        f'<defs>{defs}</defs><g filter="url(#color)">{glyph}</g></svg>'
    )


def ico_bytes(images):
    directory = struct.pack("<HHH", 0, 1, len(images))
    offset = len(directory) + 16 * len(images)
    payload = b""
    for side, png in sorted(images.items()):
        entry_side = 0 if side >= 256 else side
        directory += struct.pack("<BBBBHHII", entry_side, entry_side, 0, 0, 1, 32, len(png), offset + len(payload))
        payload += png
    return directory + payload


def build_mac_icon(renderer, logo, folder):
    iconset = Path(folder) / "TypeAlike.iconset"
    iconset.mkdir()
    for points in MAC_ICON_POINTS:
        svg = mac_icon_svg(logo, points <= MAC_SMALL_POINTS)
        (iconset / f"icon_{points}x{points}.png").write_bytes(renderer.png(svg, points))
        (iconset / f"icon_{points}x{points}@2x.png").write_bytes(renderer.png(svg, points * 2))
    subprocess.run(["iconutil", "--convert", "icns", "--output", str(MAC_ICON), str(iconset)], check=True)


def build_mac_setup_logo(renderer, logo):
    svg = mac_icon_svg(logo, MAC_SETUP_LOGO_POINTS <= MAC_SMALL_POINTS)
    MAC_SETUP_LOGO.write_bytes(renderer.png(svg, MAC_SETUP_LOGO_POINTS * 2))


def build_windows_icon(renderer, logo):
    images = {side: renderer.png(windows_icon_svg(logo, side), side) for side in WINDOWS_ICON_SIZES}
    WINDOWS_ICON.write_bytes(ico_bytes(images))


def build_windows_setup_logo(renderer, logo):
    svg = windows_icon_svg(logo, WINDOWS_SETUP_LOGO_SIZE)
    WINDOWS_SETUP_LOGO.write_bytes(renderer.png(svg, WINDOWS_SETUP_LOGO_SIZE))


def build_tray_icon(renderer, state, color, name):
    images = {side: renderer.png(tray_svg(grid, state, color), side) for side, grid in TRAY_GRIDS.items()}
    (WINDOWS_TRAY / f"{state}-{name}.ico").write_bytes(ico_bytes(images))


def build_tray_icons(renderer):
    WINDOWS_TRAY.mkdir(parents=True, exist_ok=True)
    for state in TRAY_STATES:
        for name, color in TRAY_COLORS.items():
            build_tray_icon(renderer, state, color, name)
    for name, color in STOPPED_COLORS.items():
        build_tray_icon(renderer, "paused", color, name)


def build_menu_bar_images(renderer, folder):
    MAC_MENU_BAR.mkdir(parents=True, exist_ok=True)
    for state in TRAY_STATES:
        svg = tray_svg(MENU_BAR_POINTS, state, TRAY_COLORS["black"])
        normal = Path(folder) / f"{state}.png"
        retina = Path(folder) / f"{state}@2x.png"
        normal.write_bytes(renderer.png(svg, MENU_BAR_POINTS))
        retina.write_bytes(renderer.png(svg, MENU_BAR_POINTS * 2))
        output = MAC_MENU_BAR / f"{state}.tiff"
        subprocess.run(["tiffutil", "-cathidpicheck", str(normal), str(retina), "-out", str(output)], check=True)


def main():
    with tempfile.TemporaryDirectory() as folder:
        renderer = Renderer(folder)
        logo = Logo(renderer)
        build_mac_icon(renderer, logo, folder)
        build_mac_setup_logo(renderer, logo)
        build_windows_icon(renderer, logo)
        build_windows_setup_logo(renderer, logo)
        build_tray_icons(renderer)
        build_menu_bar_images(renderer, folder)


if __name__ == "__main__":
    main()
