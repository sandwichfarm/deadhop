"""Generate deadhop's logo SVGs: mark, outlined wordmark, lockups, favicon.

The wordmark is Schibsted Grotesk at weight 800, shaped with HarfBuzz, tracked
-3.5%, and converted to outlines so the files need no font.
usage: make_logos.py <SchibstedGrotesk[wght].ttf> <brand dir>
Writes <brand dir>/logo/*.svg and <brand dir>/icons/{favicon,icon,icon-light}.svg.
Needs: pip install fonttools uharfbuzz
"""
import sys, os, re
import uharfbuzz as hb
from fontTools.ttLib import TTFont
from fontTools.varLib.instancer import instantiateVariableFont
from fontTools.pens.svgPathPen import SVGPathPen
from fontTools.pens.transformPen import TransformPen
from fontTools.pens.boundsPen import BoundsPen

src, out = sys.argv[1], sys.argv[2]
os.makedirs(out, exist_ok=True)

GRAPHITE, FOG, ROUTE, ROUTE_DK, CONCRETE, NIGHT = '#16191b', '#e6ebe9', '#0b6e69', '#5fc6b9', '#e9e8e3', '#0e1214'

# ---- the mark, on its 32-unit grid (Hop to drop: a dot, a dashed hop, a slotted drop box) ----
MARK_BODY = (
    '<circle cx="4.6" cy="20" r="3.1" fill="{c}"/>'
    '<path d="M7.4 16.2C10.5 5 20 2.6 24.5 14.8" fill="none" stroke="{c}" stroke-width="2.8" '
    'stroke-linecap="round" stroke-dasharray="4.2 3.6"/>'
    '<path d="M18 18.5H22.3V20.2a1 1 0 0 0 1 1H25.7a1 1 0 0 0 1-1V18.5H31V28.5a1.5 1.5 0 0 1-1.5 1.5H19.5A1.5 1.5 0 0 1 18 28.5Z" fill="{c}"/>'
)
# Visible bounds of the mark on the grid (stroke included)
MARK_X0, MARK_X1, MARK_Y0, MARK_Y1 = 1.5, 31.0, 5.27, 30.0

def mark(c):
    return MARK_BODY.format(c=c)

def write(name, svg):
    sub = 'icons' if name.startswith(('favicon', 'icon')) else 'logo'
    os.makedirs(os.path.join(out, sub), exist_ok=True)
    with open(os.path.join(out, sub, name), 'w') as f:
        f.write(svg.strip() + '\n')

# ---- wordmark outlines ----
font = TTFont(src)
static = instantiateVariableFont(font, {'wght': 800}, inplace=False)
tmp = os.path.join(out, '.sg800.ttf')
static.save(tmp)
blob = hb.Blob.from_file_path(tmp)
face = hb.Face(blob)
hbfont = hb.Font(face)
upem = face.upem
buf = hb.Buffer()
buf.add_str('deadhop')
buf.guess_segment_properties()
hb.shape(hbfont, buf, {'kern': True, 'liga': True})
gs = static.getGlyphSet()
order = static.getGlyphOrder()
track = -0.035 * upem

pen = SVGPathPen(gs)
bpen = BoundsPen(gs)
x = 0
for info, pos in zip(buf.glyph_infos, buf.glyph_positions):
    name = order[info.codepoint]
    # font units are y-up; flip to SVG y-down with the baseline at y=0
    t = (1, 0, 0, -1, x + pos.x_offset, -pos.y_offset)
    gs[name].draw(TransformPen(pen, t))
    gs[name].draw(TransformPen(bpen, t))
    x += pos.x_advance + track
os.remove(tmp)
word_d = re.sub(r'-?\d+\.\d+', lambda m: f'{float(m.group()):.0f}', pen.getCommands())
wx0, wy0, wx1, wy1 = bpen.bounds  # y-down: wy0 is the ascender top (negative)
asc_top = wy0           # top of d/h ascenders
base = 0                # baseline
desc = wy1              # bottom of p
word_w = wx1 - wx0

# ---- lockup geometry ----
# Mark scaled so its visible height runs from the baseline up to the ascender top.
scale = (base - asc_top) / (MARK_Y1 - MARK_Y0)
gap = 0.30 * (base - asc_top)
mark_w = (MARK_X1 - MARK_X0) * scale
pad = 0.12 * (base - asc_top)

def lockup(mark_c, word_c, bg=None):
    W = pad + mark_w + gap + word_w + pad
    top = asc_top - pad
    H = (desc + pad) - top
    mx = pad - MARK_X0 * scale
    my = (base - MARK_Y1 * scale)  # mark's visible bottom on the baseline
    wx = pad + mark_w + gap - wx0
    bg_rect = f'<rect x="0" y="{top:.1f}" width="{W:.1f}" height="{H:.1f}" fill="{bg}"/>' if bg else ''
    return f'''<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 {top:.1f} {W:.1f} {H:.1f}" role="img" aria-label="deadhop">
{bg_rect}<g transform="translate({mx:.2f} {my:.2f}) scale({scale:.4f})">{mark(mark_c)}</g>
<path transform="translate({wx:.2f} 0)" fill="{word_c}" d="{word_d}"/>
</svg>'''

def wordmark(c):
    top = asc_top - pad
    H = (desc + pad) - top
    W = word_w + 2 * pad
    return f'''<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 {top:.1f} {W:.1f} {H:.1f}" role="img" aria-label="deadhop">
<path transform="translate({pad - wx0:.2f} 0)" fill="{c}" d="{word_d}"/>
</svg>'''

def mark_svg(c, title=True):
    return f'<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 32 32" role="img" aria-label="deadhop">{mark(c)}</svg>'

def tile(fg, bg):
    # App icon: mark centred on a rounded square, for avatars and touch icons.
    s = 0.66
    off = (32 - 32 * s) / 2
    # nudge so the visible mark (not its grid) is centred
    cx = (MARK_X0 + MARK_X1) / 2 * s + off
    cy = (MARK_Y0 + MARK_Y1) / 2 * s + off
    dx, dy = 16 - cx, 16 - cy
    return (f'<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 32 32" role="img" aria-label="deadhop">'
            f'<rect width="32" height="32" rx="7" fill="{bg}"/>'
            f'<g transform="translate({off + dx:.3f} {off + dy:.3f}) scale({s})">{mark(fg)}</g></svg>')

write('mark.svg', mark_svg(GRAPHITE))
write('mark-fog.svg', mark_svg(FOG))
write('mark-route.svg', mark_svg(ROUTE))
write('mark-currentcolor.svg', mark_svg('currentColor'))
write('wordmark.svg', wordmark(GRAPHITE))
write('wordmark-fog.svg', wordmark(FOG))
write('lockup.svg', lockup(ROUTE, GRAPHITE))
write('lockup-dark.svg', lockup(ROUTE_DK, FOG))
write('lockup-mono.svg', lockup(GRAPHITE, GRAPHITE))
write('lockup-mono-fog.svg', lockup(FOG, FOG))
write('icon.svg', tile(ROUTE_DK, NIGHT))
write('icon-light.svg', tile(GRAPHITE, CONCRETE))
# Favicon: graphite mark, switching to fog when the browser UI is dark.
write('favicon.svg', f'''<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 32 32">
<style>.m{{color:{GRAPHITE}}}@media (prefers-color-scheme:dark){{.m{{color:{FOG}}}}}</style>
<g class="m">{mark('currentColor')}</g></svg>''')
print('ok', round(word_w), round(asc_top), round(desc), 'scale', round(scale, 2))
