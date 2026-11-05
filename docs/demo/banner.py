"""Generates the animated README banner: docs/assets/banner-{dark,light}.svg.

A full disk empties as stale build output is cleared, one item at a time, on an 8 s loop.
Plain SVG + CSS animation (renders in <img> on GitHub); motion stops for prefers-reduced-motion.
Run: python3 docs/demo/banner.py
"""
from pathlib import Path

W, H = 1280, 300
THEMES = {
    "dark": dict(bg0="#0d1117", bg1="#161b22", line="#30363d", fg="#e6edf3", muted="#8b949e",
                 accent="#7ee787", accent2="#79c0ff", track="#21262d", full="#f85149", ok="#3fb950",
                 chip="#161b22"),
    "light": dict(bg0="#ffffff", bg1="#f6f8fa", line="#d0d7de", fg="#1f2328", muted="#59636e",
                  accent="#1a7f37", accent2="#0969da", track="#eaeef2", full="#cf222e", ok="#1a7f37",
                  chip="#ffffff"),
}
ITEMS = [  # label, size, cleared at (fraction of the loop)
    ("~/work/api/target", "41.2 GB", 0.22),
    ("~/web/node_modules", "15.1 GB", 0.36),
    ("~/.cache/go-build", "14.0 GB", 0.50),
]
MONO = "ui-monospace, SFMono-Regular, 'SF Mono', Menlo, Consolas, 'Liberation Mono', monospace"
SANS = "-apple-system, BlinkMacSystemFont, 'Segoe UI', Helvetica, Arial, sans-serif"


def pct(x: float) -> str:
    return f"{x * 100:.1f}%"


def svg(t: dict) -> str:
    bar_x, bar_y, bar_w, bar_h = 700, 92, 500, 22
    # Used share of the disk over the loop: full, three drops, hold, back to full.
    steps = [(0, 0.92), (0.18, 0.92)]
    used = 0.92
    for _, _, at in ITEMS:
        drop = {0.22: 0.20, 0.36: 0.17, 0.50: 0.17}[at]
        steps += [(at, used), (at + 0.06, used - drop)]
        used -= drop
    steps += [(0.86, used), (0.97, 0.92), (1, 0.92)]
    bar_frames = "\n".join(
        f"  {pct(k)} {{ width: {v * bar_w:.1f}px; fill: {t['full'] if v > 0.8 else t['accent2'] if v > 0.5 else t['ok']}; }}"
        for k, v in steps
    )
    rows = []
    css_items = []
    for i, (label, size, at) in enumerate(ITEMS):
        y = 158 + i * 38
        rows.append(f'''
  <g class="item i{i}">
    <rect x="{bar_x}" y="{y - 22}" width="{bar_w}" height="30" rx="6" fill="{t['chip']}" stroke="{t['line']}"/>
    <text x="{bar_x + 14}" y="{y - 2}" font-family="{MONO}" font-size="15" fill="{t['fg']}">{label}</text>
    <text x="{bar_x + bar_w - 14}" y="{y - 2}" font-family="{MONO}" font-size="15" fill="{t['muted']}" text-anchor="end">{size}</text>
    <line class="strike" x1="{bar_x + 10}" y1="{y - 7}" x2="{bar_x + bar_w - 10}" y2="{y - 7}" stroke="{t['ok']}" stroke-width="2"/>
  </g>''')
        appear = 0.04 + i * 0.03
        css_items.append(f"""
.i{i} {{ animation: item{i} 8s ease-in-out infinite; }}
.i{i} .strike {{ stroke-dasharray: {bar_w - 20}; animation: strike{i} 8s ease-in-out infinite; }}
@keyframes item{i} {{
  0%, {pct(appear)} {{ opacity: 0; transform: translateX(16px); }}
  {pct(appear + 0.05)}, {pct(at + 0.04)} {{ opacity: 1; transform: translateX(0); }}
  {pct(at + 0.10)}, 100% {{ opacity: 0.28; transform: translateX(0); }}
}}
@keyframes strike{i} {{
  0%, {pct(at)} {{ stroke-dashoffset: {bar_w - 20}; }}
  {pct(at + 0.05)}, 92% {{ stroke-dashoffset: 0; }}
  100% {{ stroke-dashoffset: {bar_w - 20}; }}
}}""")
    labels = [("92% used", 0.0, 0.24), ("72% used", 0.26, 0.38), ("55% used", 0.40, 0.52), ("38% used", 0.54, 0.95)]
    label_svg, label_css = [], []
    for j, (text, a, b) in enumerate(labels):
        label_svg.append(
            f'<text class="u u{j}" x="{bar_x + bar_w}" y="{bar_y - 12}" font-family="{MONO}" font-size="15" '
            f'fill="{t["fg"]}" text-anchor="end">{text}</text>')
        label_css.append(f"\n.u{j} {{ opacity: 0; animation: u{j} 8s linear infinite; }}")
        if j:
            label_css.append(f"""
@keyframes u{j} {{ 0%, {pct(a - 0.01)} {{ opacity: 0; }} {pct(a)}, {pct(b)} {{ opacity: 1; }} {pct(b + 0.01)}, 100% {{ opacity: 0; }} }}""")
    # The first label must also show at the very end of the loop, when the disk is full again.
    label_css.append(f"""
@keyframes u0 {{ 0%, {pct(0.24)} {{ opacity: 1; }} {pct(0.25)}, {pct(0.96)} {{ opacity: 0; }} {pct(0.97)}, 100% {{ opacity: 1; }} }}""")
    return f'''<svg xmlns="http://www.w3.org/2000/svg" width="{W}" height="{H}" viewBox="0 0 {W} {H}" role="img" aria-label="diskgc: a background disk cleaner for developer machines">
<style>
.bar {{ animation: bar 8s ease-in-out infinite; }}
@keyframes bar {{
{bar_frames}
}}
.cursor {{ animation: blink 1.1s steps(1) infinite; }}
@keyframes blink {{ 50% {{ opacity: 0; }} }}
{"".join(css_items)}
{"".join(label_css)}
@media (prefers-reduced-motion: reduce) {{
  .bar, .item, .strike, .u, .cursor {{ animation: none !important; }}
  .item, .u0 {{ opacity: 1; }}
}}
</style>
<defs>
  <linearGradient id="bg" x1="0" y1="0" x2="1" y2="1">
    <stop offset="0" stop-color="{t['bg0']}"/><stop offset="1" stop-color="{t['bg1']}"/>
  </linearGradient>
  <pattern id="grid" width="24" height="24" patternUnits="userSpaceOnUse">
    <path d="M24 0H0V24" fill="none" stroke="{t['line']}" stroke-width="0.6" opacity="0.55"/>
  </pattern>
</defs>
<rect width="{W}" height="{H}" rx="18" fill="url(#bg)"/>
<rect width="{W}" height="{H}" rx="18" fill="url(#grid)" opacity="0.5"/>
<rect x="0.5" y="0.5" width="{W - 1}" height="{H - 1}" rx="18" fill="none" stroke="{t['line']}"/>

<g transform="translate(72 0)">
  <text x="0" y="128" font-family="{MONO}" font-size="76" font-weight="700" fill="{t['fg']}" letter-spacing="-2">disk<tspan fill="{t['accent']}">gc</tspan></text>
  <rect class="cursor" x="266" y="72" width="34" height="60" rx="3" fill="{t['accent']}" opacity="0.85"/>
  <text x="2" y="176" font-family="{SANS}" font-size="23" fill="{t['muted']}">A background disk cleaner for developer machines</text>
  <text x="2" y="208" font-family="{SANS}" font-size="23" fill="{t['muted']}">that knows what it is deleting — and tells you why.</text>
  <text x="2" y="252" font-family="{MONO}" font-size="15" fill="{t['accent2']}">macOS · Linux · Windows   ·   Rust   ·   Apache-2.0</text>
</g>

<text x="{bar_x}" y="{bar_y - 12}" font-family="{MONO}" font-size="15" fill="{t['muted']}">disk</text>
{"".join(label_svg)}
<rect x="{bar_x}" y="{bar_y}" width="{bar_w}" height="{bar_h}" rx="6" fill="{t['track']}"/>
<rect class="bar" x="{bar_x}" y="{bar_y}" width="{0.92 * bar_w:.1f}" height="{bar_h}" rx="6" fill="{t['full']}"/>
{"".join(rows)}
</svg>
'''


out = Path(__file__).resolve().parents[1] / "assets"
for name, theme in THEMES.items():
    (out / f"banner-{name}.svg").write_text(svg(theme))
    print(out / f"banner-{name}.svg")
