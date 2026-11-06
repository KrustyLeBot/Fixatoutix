#!/usr/bin/env python3
"""Regenerates every icon from the SVG sources.

    pip install cairosvg pillow
    python tools/make_icons.py                       # uses branding/logo.svg
    python tools/make_icons.py branding/candidates/05-torque.svg   # picks a new logo

Writes src-tauri/icons/icon.ico (16/20/24/32/48/256), icon.png (512) and the four tray icons.
"""
import io
import shutil
import sys
from pathlib import Path

import cairosvg
from PIL import Image

ROOT = Path(__file__).resolve().parent.parent
ICONS = ROOT / "src-tauri" / "icons"
SIZES = [16, 20, 24, 32, 48, 256]


def render(svg: Path, size: int) -> Image.Image:
    png = cairosvg.svg2png(url=str(svg), output_width=size, output_height=size)
    return Image.open(io.BytesIO(png)).convert("RGBA")


def main() -> None:
    logo = ROOT / "branding" / "logo.svg"
    if len(sys.argv) > 1:
        chosen = Path(sys.argv[1]).resolve()
        if chosen != logo.resolve():
            shutil.copyfile(chosen, logo)
    ICONS.mkdir(parents=True, exist_ok=True)

    frames = [render(logo, s) for s in SIZES]
    frames[-1].save(ICONS / "icon.ico", sizes=[(s, s) for s in SIZES], append_images=frames[:-1])
    render(logo, 512).save(ICONS / "icon.png")

    for state in ("guard", "banquet"):
        for taskbar in ("light", "dark"):
            render(ROOT / "branding" / "tray" / f"tray-{state}-{taskbar}.svg", 32).save(
                ICONS / f"tray-{state}-{taskbar}.png"
            )
    print("icons written to", ICONS)


if __name__ == "__main__":
    main()
