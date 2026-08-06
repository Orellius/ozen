#!/usr/bin/env python3
"""Generate every icon Ozen ships: the app icon set and the menu-bar frames.

Why a generator instead of checked-in art: the icons ARE the orb (a squircle of dark glass
with an asymmetric light inside it), and the orb's geometry lives in code. Deriving both from
the same numbers is what keeps the app icon, the Dock tile and the live orb looking like one
object. Re-run after changing the palette or the tile radius.

    python3 scripts/gen-icons.py

Writes src-tauri/icons/. Requires Pillow only.
"""
from __future__ import annotations

import math
import os
import subprocess
import tempfile

from PIL import Image, ImageDraw, ImageFilter

ICONS = os.path.join(os.path.dirname(os.path.dirname(os.path.abspath(__file__))), "src-tauri", "icons")

# The tile, matching public/pill.html: a superellipse, not a rounded rectangle. macOS app
# icons sit in an 824/1024 content box, so everything scales off that.
SQUIRCLE_N = 5.0
CONTENT = 824 / 1024

GLASS_TOP = (44, 50, 68)
GLASS_BOTTOM = (20, 23, 32)
CORE = (232, 242, 255)
ACCENT = (110, 168, 254)


def squircle(cx: float, cy: float, half: float, n: float = SQUIRCLE_N, steps: int = 720):
    """Superellipse |x|^n + |y|^n = 1, the Apple corner. A plain rounded rectangle reads as
    subtly wrong next to real macOS icons; the continuous curvature is the whole tell."""
    pts = []
    for i in range(steps):
        t = 2 * math.pi * i / steps
        ct, st = math.cos(t), math.sin(t)
        x = math.copysign(abs(ct) ** (2 / n), ct)
        y = math.copysign(abs(st) ** (2 / n), st)
        pts.append((cx + x * half, cy + y * half))
    return pts


def blob(cx: float, cy: float, r: float, wob: float, phase: float, steps: int = 360):
    """The same harmonic deformation the live orb uses (2/3/5/7, no common factor), frozen at
    one phase so the icon is a still frame of the thing you actually see."""
    pts = []
    for i in range(steps):
        a = 2 * math.pi * i / steps
        d = (
            math.sin(a * 2 + phase) * 0.55
            + math.sin(a * 3 - phase * 1.4) * 0.30
            + math.sin(a * 5 + phase * 0.7) * 0.16
            + math.sin(a * 7 - phase * 2.1) * 0.08
        )
        rr = r * (1 + d * wob)
        pts.append((cx + math.cos(a) * rr, cy + math.sin(a) * rr))
    return pts


def lerp(a, b, k):
    return tuple(int(round(a[i] + (b[i] - a[i]) * k)) for i in range(3))


def app_icon(size: int = 1024) -> Image.Image:
    img = Image.new("RGBA", (size, size), (0, 0, 0, 0))
    half = size * CONTENT / 2
    c = size / 2

    # Tile: vertical glass gradient, drawn as a full-bleed ramp then masked to the squircle.
    ramp = Image.new("RGBA", (size, size))
    rd = ImageDraw.Draw(ramp)
    for y in range(size):
        rd.line([(0, y), (size, y)], fill=lerp(GLASS_TOP, GLASS_BOTTOM, y / size) + (255,))
    mask = Image.new("L", (size, size), 0)
    ImageDraw.Draw(mask).polygon(squircle(c, c, half), fill=255)
    img.paste(ramp, (0, 0), mask)

    # Sound arcs, echoing the menu-bar glyph so the two read as one brand. Drawn behind the
    # light and dim, so they are texture rather than a second subject.
    arcs = Image.new("RGBA", (size, size), (0, 0, 0, 0))
    ad = ImageDraw.Draw(arcs)
    for i in range(3):
        r = size * (0.20 + 0.105 * i)
        ad.arc(
            [c - r, c - r, c + r, c + r], start=-58, end=58,
            fill=ACCENT + (54 - i * 12,), width=max(3, int(size * 0.011)),
        )
    img.alpha_composite(Image.composite(arcs, Image.new("RGBA", (size, size), (0, 0, 0, 0)), mask))

    # The light, decomposed so the asymmetry survives: the SILHOUETTE is the blob, painted
    # through it as a mask, while the SHADING is a plain radial ramp whose origin sits up-left
    # of centre. Shading with shrinking blobs (the first attempt) averages the harmonics away
    # and leaves a symmetric cloud - the exact thing that made this look generic.
    r0 = size * 0.255
    shape = Image.new("L", (size, size), 0)
    ImageDraw.Draw(shape).polygon(blob(c, c, r0, 0.16, 1.9), fill=255)

    ramp2 = Image.new("RGBA", (size, size), (0, 0, 0, 0))
    r2d = ImageDraw.Draw(ramp2)
    lx, ly = c - r0 * 0.34, c - r0 * 0.40
    steps = 260
    for i in range(steps):
        k = i / steps               # 0 = outer edge, 1 = hot core
        r = r0 * 1.45 * (1 - k)
        col = lerp(ACCENT, CORE, min(1.0, k ** 0.75 * 1.35))
        alpha = int(8 + 247 * (k ** 1.7))
        r2d.ellipse([lx - r, ly - r, lx + r, ly + r], fill=col + (alpha,))
    body = Image.new("RGBA", (size, size), (0, 0, 0, 0))
    body.paste(ramp2, (0, 0), shape)

    # The spill is the light landing on the glass around the blob, so it is blurred from the
    # blob's own silhouette rather than from a circle.
    spill = body.filter(ImageFilter.GaussianBlur(size * 0.055))
    lit = Image.alpha_composite(spill, body)
    img.alpha_composite(Image.composite(lit, Image.new("RGBA", (size, size), (0, 0, 0, 0)), mask))

    # Edge treatment: specular hairline along the top, hairline outline overall.
    edge = Image.new("RGBA", (size, size), (0, 0, 0, 0))
    ed = ImageDraw.Draw(edge)
    ed.line(squircle(c, c - size * 0.0035, half) + [squircle(c, c, half)[0]],
            fill=(255, 255, 255, 90), width=max(2, size // 340))
    ed.line(squircle(c, c, half) + [squircle(c, c, half)[0]],
            fill=(255, 255, 255, 38), width=max(1, size // 512))
    img.alpha_composite(Image.composite(edge, Image.new("RGBA", (size, size), (0, 0, 0, 0)), mask))
    return img


def tray_frame(arcs: int, size: int = 44, dim: bool = False) -> Image.Image:
    """A menu-bar template image: pure black, meaning carried entirely by the alpha channel
    (macOS recolours it for light/dark bars automatically). A dot plus N sound arcs - the arc
    count is driven by live mic level at runtime, so the menu bar becomes a level meter and
    you can see that you are actually being heard without looking anywhere else."""
    ss = 4  # supersample; menu-bar glyphs are tiny and alias badly otherwise
    w = size * ss
    img = Image.new("RGBA", (w, w), (0, 0, 0, 0))
    d = ImageDraw.Draw(img)

    cx, cy = w * 0.30, w * 0.5
    base = 255 if not dim else 105

    d.ellipse([cx - w * 0.085, cy - w * 0.085, cx + w * 0.085, cy + w * 0.085], fill=(0, 0, 0, base))

    stroke = max(2, int(w * 0.055))
    for i in range(3):
        r = w * (0.20 + 0.115 * i)
        on = i < arcs
        a = base if on else (0 if not dim else 52)
        if a == 0:
            continue
        d.arc([cx - r, cy - r, cx + r, cy + r], start=-52, end=52, fill=(0, 0, 0, a), width=stroke)

    return img.resize((size, size), Image.LANCZOS)


def write_icns(icon: Image.Image) -> None:
    with tempfile.TemporaryDirectory() as tmp:
        iconset = os.path.join(tmp, "icon.iconset")
        os.makedirs(iconset)
        for px in (16, 32, 64, 128, 256, 512, 1024):
            icon.resize((px, px), Image.LANCZOS).save(os.path.join(iconset, f"icon_{px}x{px}.png"))
            half = px // 2
            if half >= 16:
                icon.resize((px, px), Image.LANCZOS).save(
                    os.path.join(iconset, f"icon_{half}x{half}@2x.png")
                )
        subprocess.run(
            ["iconutil", "-c", "icns", iconset, "-o", os.path.join(ICONS, "icon.icns")],
            check=True,
        )


def main() -> None:
    os.makedirs(ICONS, exist_ok=True)
    icon = app_icon()

    icon.save(os.path.join(ICONS, "icon.png"))
    for name, px in (("32x32.png", 32), ("128x128.png", 128), ("128x128@2x.png", 256)):
        icon.resize((px, px), Image.LANCZOS).save(os.path.join(ICONS, name))
    icon.resize((256, 256), Image.LANCZOS).save(os.path.join(ICONS, "icon.ico"))
    write_icns(icon)

    # idle is dimmed with a single arc: present, clearly not listening.
    tray_frame(1, dim=True).save(os.path.join(ICONS, "tray-idle.png"))
    for n in range(4):
        tray_frame(n).save(os.path.join(ICONS, f"tray-{n}.png"))
    # Kept so an old build referencing icons/tray.png still resolves.
    tray_frame(1, dim=True).save(os.path.join(ICONS, "tray.png"))

    print(f"wrote app icon set + 5 tray frames to {ICONS}")


if __name__ == "__main__":
    main()
