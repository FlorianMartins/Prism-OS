"""Prism logo: a rounded triangle split into three lit facets (a pyramid seen from above).
Same geometry as crates/prism-ui/src/logo.rs. Needs Pillow."""
import math, sys
from PIL import Image, ImageDraw

CYAN = (34, 211, 238); SKY = (56, 189, 248); VIOLET = (167, 139, 250); PURPLE = (139, 92, 246)
INDIGO = (91, 84, 230); DEEP = (55, 48, 163)

def geometry(cx, cy, R):
    """Same maths as crates/prism-ui/src/logo.rs: corners, rounded outline split in 3 chains, apex."""
    r = 0.16 * R
    ccx, ccy = cx, cy + 0.17 * R  # centroid lower so the shape is centred (top at -0.67 R, bottom at +0.67 R)
    corners = [(ccx + R * math.cos(math.radians(-90 + 120 * k)), ccy + R * math.sin(math.radians(-90 + 120 * k))) for k in range(3)]
    # corner arc centres: corners moved towards the centroid by r / sin(30°) = 2r
    inner = [(ccx + (x - ccx) * (R - 2 * r) / R, ccy + (y - ccy) * (R - 2 * r) / R) for x, y in corners]
    chains = []  # chain k: from tip of corner k to tip of corner k+1 along the outline
    def arc(i, a0, a1, n=10):
        return [(inner[i][0] + r * math.cos(a0 + (a1 - a0) * t / n), inner[i][1] + r * math.sin(a0 + (a1 - a0) * t / n)) for t in range(n + 1)]
    tip_angle = [math.radians(-90 + 120 * k) for k in range(3)]
    for k in range(3):
        j = (k + 1) % 3
        # outward normal of edge k->j
        nx = math.radians(-90 + 120 * k + 60)
        chain = arc(k, tip_angle[k], nx) + arc(j, nx, tip_angle[j] if tip_angle[j] > nx else tip_angle[j] + 2 * math.pi)
        chains.append(chain)
    apex = (ccx - 0.07 * R, ccy - 0.08 * R)
    tips = [(inner[k][0] + r * math.cos(tip_angle[k]), inner[k][1] + r * math.sin(tip_angle[k])) for k in range(3)]
    return apex, chains, tips

def facet_colors():
    # chain 0: top -> bottom right (right facet), chain 1: bottom right -> bottom left (bottom), chain 2: bottom left -> top (left)
    return [(VIOLET, PURPLE), (INDIGO, DEEP), (CYAN, SKY)]

def lerp(a, b, t): return tuple(int(round(a[i] + (b[i] - a[i]) * t)) for i in range(3))

def render(size, ss=8):
    N = size * ss
    im = Image.new("RGBA", (N, N), (0, 0, 0, 0))
    apex, chains, tips = geometry(N / 2, N / 2, N * 0.62)
    d = ImageDraw.Draw(im)
    for k, (c0, c1) in enumerate(facet_colors()):
        poly = [apex] + chains[k]
        # gradient along the chain, approximated by slicing the fan
        pts = chains[k]
        for i in range(len(pts) - 1):
            t = i / (len(pts) - 2)
            d.polygon([apex, pts[i], pts[i + 1]], fill=lerp(c0, c1, t) + (255,))
    if size >= 24:
        layer = Image.new("RGBA", (N, N), (0, 0, 0, 0))
        ld = ImageDraw.Draw(layer)
        w = max(1, int(N * 0.010))
        for k in (0, 2):
            a, b = apex, tips[k]
            ld.line([a, (a[0] + (b[0] - a[0]) * 0.96, a[1] + (b[1] - a[1]) * 0.96)], fill=(255, 255, 255, 120), width=w)
        im = Image.alpha_composite(im, layer)
    return im.resize((size, size), Image.LANCZOS)

if __name__ == "__main__":
    # Regenerates the icons from the repository root: python3 installer/make_icons.py
    sizes = [16, 20, 24, 32, 40, 48, 64, 128, 256]
    imgs = [render(s) for s in sizes]
    imgs[-1].save("installer/prism.ico", format="ICO", sizes=[(s, s) for s in sizes], append_images=imgs[:-1])
    open("crates/prism-ui/assets/prism-64.rgba", "wb").write(render(64).tobytes())
    render(512).save("docs/images/logo.png")
    print("installer/prism.ico, crates/prism-ui/assets/prism-64.rgba, docs/images/logo.png")
