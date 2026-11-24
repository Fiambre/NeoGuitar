"""Generates the NeoGuitar icon (assets/icon/*). Requires Pillow: pip install pillow

Design: a blue guitar pick on a dark rounded square, crossed by the strings of a tab.
Each size is drawn separately (at 8x, then downscaled) so the small ones stay sharp.
"""
import os
from PIL import Image, ImageDraw

OUT = os.path.join(os.path.dirname(os.path.dirname(os.path.abspath(__file__))), "assets", "icon")
SS = 8  # sobremuestreo

BG_TOP, BG_BOT = (36, 41, 55), (16, 18, 24)
PICK_TOP, PICK_BOT = (140, 192, 255), (66, 128, 238)
STRING_BG = (74, 83, 102)
STRING_ON_PICK = (14, 20, 36)


def lerp(a, b, t):
    return tuple(int(a[i] + (b[i] - a[i]) * t) for i in range(3))


def vgradient(size, top, bottom):
    g = Image.new("RGB", (1, size))
    for y in range(size):
        g.putpixel((0, y), lerp(top, bottom, y / max(1, size - 1)))
    return g.resize((size, size))


def rounded_triangle_mask(size, pts, radius):
    """Triángulo con esquinas redondeadas: polígono reducido + trazo grueso con uniones curvas."""
    cx = sum(p[0] for p in pts) / 3
    cy = sum(p[1] for p in pts) / 3
    # Reducir el triángulo hacia su centro lo suficiente para que el trazo restituya el contorno.
    def shrink(p):
        dx, dy = p[0] - cx, p[1] - cy
        d = (dx * dx + dy * dy) ** 0.5
        k = (d - radius * 1.9) / d
        return (cx + dx * k, cy + dy * k)
    inner = [shrink(p) for p in pts]
    m = Image.new("L", (size, size), 0)
    d = ImageDraw.Draw(m)
    d.polygon(inner, fill=255)
    d.line(inner + [inner[0], inner[1]], fill=255, width=int(radius * 2 * 1.9), joint="curve")
    for p in inner:
        d.ellipse([p[0] - radius * 1.9, p[1] - radius * 1.9, p[0] + radius * 1.9, p[1] + radius * 1.9], fill=255)
    return m


def render(px):
    S = px * SS
    img = vgradient(S, BG_TOP, BG_BOT).convert("RGBA")
    # Fondo: cuadrado redondeado
    mask = Image.new("L", (S, S), 0)
    ImageDraw.Draw(mask).rounded_rectangle([0, 0, S - 1, S - 1], radius=int(S * 0.225), fill=255)
    bg = Image.new("RGBA", (S, S), (0, 0, 0, 0))
    bg.paste(img, (0, 0), mask)
    # Borde sutil
    if px >= 32:
        ImageDraw.Draw(bg).rounded_rectangle([0, 0, S - 1, S - 1], radius=int(S * 0.225), outline=(255, 255, 255, 26), width=max(1, int(S * 0.006)))

    pick_pts = [(0.215 * S, 0.215 * S), (0.785 * S, 0.215 * S), (0.5 * S, 0.865 * S)]
    pick_mask = rounded_triangle_mask(S, pick_pts, S * 0.085)
    pick = vgradient(S, PICK_TOP, PICK_BOT).convert("RGBA")
    layer = Image.new("RGBA", (S, S), (0, 0, 0, 0))
    layer.paste(pick, (0, 0), pick_mask)

    # Cuerdas (6; menos en tamaños chicos)
    n = 6 if px >= 64 else (4 if px >= 32 else 0)
    if n:
        d_bg = ImageDraw.Draw(bg)
        y0, y1 = 0.31 * S, 0.69 * S
        w = max(1, int(S * (0.011 if px >= 64 else 0.02)))
        strings = Image.new("RGBA", (S, S), (0, 0, 0, 0))
        ds = ImageDraw.Draw(strings)
        for i in range(n):
            y = y0 + (y1 - y0) * i / (n - 1)
            ds.line([(0.085 * S, y), (0.915 * S, y)], fill=STRING_BG + (255,), width=w)
        bg = Image.alpha_composite(bg, strings)
        # Sobre la púa, las cuerdas van oscuras
        dark = Image.new("RGBA", (S, S), (0, 0, 0, 0))
        dd = ImageDraw.Draw(dark)
        for i in range(n):
            y = y0 + (y1 - y0) * i / (n - 1)
            dd.line([(0.085 * S, y), (0.915 * S, y)], fill=STRING_ON_PICK + (255,), width=w)
        clipped = Image.new("RGBA", (S, S), (0, 0, 0, 0))
        clipped.paste(dark, (0, 0), pick_mask)
        layer = Image.alpha_composite(layer, clipped)
    out = Image.alpha_composite(bg, layer)
    return out.resize((px, px), Image.LANCZOS)


def main():
    os.makedirs(OUT, exist_ok=True)
    sizes = [16, 24, 32, 48, 64, 128, 256]
    imgs = {s: render(s) for s in sizes}
    render(512).save(os.path.join(OUT, "icon-512.png"))
    imgs[256].save(os.path.join(OUT, "icon-256.png"))
    imgs[256].save(
        os.path.join(OUT, "neoguitar.ico"),
        format="ICO",
        sizes=[(s, s) for s in sizes],
        append_images=[imgs[s] for s in sizes if s != 256],
    )
    print("ok")


if __name__ == "__main__":
    main()
