"""Render the ImagineCAD icon set (PNG, ICO, ICNS iconset) with Pillow.

Design: a flat rounded tile with a folded-paper facet and a bold, extruded
"IC" monogram — the ImagineCAD mark.

Usage: python3 packaging/scripts/make_icons.py   (requires `pip install pillow`)
"""
import math, os, shutil, subprocess, sys
from PIL import Image, ImageDraw, ImageFilter

ROOT = os.path.abspath(os.path.join(os.path.dirname(__file__), "..", ".."))
ASSETS = os.path.join(ROOT, "assets")

TILE_TOP = (18, 150, 160)     # teal
TILE_BOTTOM = (6, 78, 96)     # deep teal
FACET = (255, 255, 255, 34)   # folded-corner highlight
LETTER = (250, 252, 255, 255)
EXTRUDE = (3, 42, 52, 255)
ACCENT = (255, 176, 48, 255)  # drafting-orange detail


def monogram_mask(W, u):
    """Bold geometric 'IC': a slab-serif I beside a C ring, centred on the tile."""
    m = Image.new("L", (W, W), 0)
    d = ImageDraw.Draw(m)
    cy = 512 * u
    # C ring with its opening on the right.
    cx, R, t = 618 * u, 252 * u, 104 * u
    d.ellipse([cx - R, cy - R, cx + R, cy + R], fill=255)
    d.ellipse([cx - R + t, cy - R + t, cx + R - t, cy + R - t], fill=0)
    d.polygon([(cx, cy), (cx + R * 1.4, cy - R * 0.70), (cx + R * 1.4, cy + R * 0.70)], fill=0)
    # I with slab serifs.
    ix, w, h = 262 * u, 100 * u, 504 * u
    d.rectangle([ix - w / 2, cy - h / 2, ix + w / 2, cy + h / 2], fill=255)
    for y0 in (cy - h / 2, cy + h / 2 - w * 0.55):
        d.rectangle([ix - w * 0.95, y0, ix + w * 0.95, y0 + w * 0.55], fill=255)
    return m


def render(size=1024):
    k = 4
    W = size * k
    u = W / 1024
    img = Image.new("RGBA", (W, W), (0, 0, 0, 0))
    # Tile with vertical gradient.
    grad = Image.new("RGBA", (W, W))
    gd = ImageDraw.Draw(grad)
    for y in range(W):
        t = y / W
        gd.line([(0, y), (W, y)], fill=tuple(int(TILE_TOP[i] * (1 - t) + TILE_BOTTOM[i] * t) for i in range(3)) + (255,))
    m = int(W * 0.055)
    tile = Image.new("L", (W, W), 0)
    ImageDraw.Draw(tile).rounded_rectangle([m, m, W - m, W - m], radius=int(W * 0.19), fill=255)
    img.paste(grad, (0, 0), tile)
    # Folded-paper facet across the upper-left half.
    facet = Image.new("RGBA", (W, W), (0, 0, 0, 0))
    ImageDraw.Draw(facet).polygon([(0, 0), (W, 0), (0, W)], fill=FACET)
    img = Image.alpha_composite(img, Image.composite(facet, Image.new("RGBA", (W, W), (0, 0, 0, 0)), tile))
    # Extruded monogram: dark offset copies, then the face.
    mono = monogram_mask(W, u)
    depth = int(26 * u)
    for i in range(depth, 0, -max(1, int(2 * u))):
        layer = Image.new("RGBA", (W, W), EXTRUDE)
        img.paste(layer, (i, i), mono)
    shadow = Image.new("RGBA", (W, W), (0, 0, 0, 0))
    shadow.paste(Image.new("RGBA", (W, W), (0, 0, 0, 90)), (depth + int(10 * u), depth + int(14 * u)), mono)
    img = Image.alpha_composite(Image.alpha_composite(Image.new("RGBA", (W, W), (0, 0, 0, 0)), shadow.filter(ImageFilter.GaussianBlur(18 * u))), img)
    img.paste(Image.new("RGBA", (W, W), LETTER), (0, 0), mono)
    # Subtle top-light on the letter face.
    hi = Image.new("L", (W, W), 0)
    ImageDraw.Draw(hi).rectangle([0, 0, W, int(W * 0.5)], fill=40)
    face_hi = Image.new("RGBA", (W, W), (255, 255, 255, 0))
    face_hi.putalpha(Image.composite(hi, Image.new("L", (W, W), 0), mono))
    img = Image.alpha_composite(img, face_hi)
    # Drafting accent: a small crosshair at the C's centre.
    d = ImageDraw.Draw(img)
    cx, cy, r = 618 * u, 512 * u, 44 * u
    d.line([(cx - r, cy), (cx + r, cy)], fill=ACCENT, width=int(16 * u))
    d.line([(cx, cy - r), (cx, cy + r)], fill=ACCENT, width=int(16 * u))
    # Clip everything to the tile.
    out = Image.new("RGBA", (W, W), (0, 0, 0, 0))
    out.paste(img, (0, 0), tile)
    return out.resize((size, size), Image.LANCZOS)


def main():
    os.makedirs(ASSETS, exist_ok=True)
    big = render(1024)
    big.save(os.path.join(ASSETS, "icon.png"))
    for s in (512, 256, 128):
        big.resize((s, s), Image.LANCZOS).save(os.path.join(ASSETS, f"icon-{s}.png"))
    big.save(os.path.join(ASSETS, "icon.ico"), sizes=[(16, 16), (24, 24), (32, 32), (48, 48), (64, 64), (128, 128), (256, 256)])
    iconset = os.path.join(ASSETS, "ImagineCAD.iconset")
    os.makedirs(iconset, exist_ok=True)
    for s in (16, 32, 128, 256, 512):
        big.resize((s, s), Image.LANCZOS).save(os.path.join(iconset, f"icon_{s}x{s}.png"))
        big.resize((s * 2, s * 2), Image.LANCZOS).save(os.path.join(iconset, f"icon_{s}x{s}@2x.png"))
    if shutil.which("iconutil"):
        subprocess.run(["iconutil", "-c", "icns", iconset, "-o", os.path.join(ASSETS, "icon.icns")], check=True)
    shutil.rmtree(iconset)
    print("icons written to", ASSETS)


if __name__ == "__main__":
    sys.exit(main())
