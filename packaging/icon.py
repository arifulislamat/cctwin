# Draws the app icon (two pixel critters) as a 1024px PNG. Regenerate packaging/cctwin.icns with:
#   python3 packaging/icon.py icon.png && mkdir cctwin.iconset && for s in 16 32 128 256 512; do
#     sips -z $s $s icon.png --out cctwin.iconset/icon_${s}x${s}.png
#     sips -z $((s*2)) $((s*2)) icon.png --out cctwin.iconset/icon_${s}x${s}@2x.png; done && iconutil -c icns cctwin.iconset -o packaging/cctwin.icns
import struct, zlib, sys
N = 1024
ART = ["..########..","..########..","..#.####.#..","..#.####.#..","############","############","..########..","..########..","....#..#....","....#..#...."]
BG, A, B = (0x1F,0x24,0x33), (0xE0,0x7A,0x5F), (0xF2,0xCC,0x8F)  # navy, coral, sand
CELL, GAP = 26, 2                      # 2 critters of 12 cells + 2-cell gap = 26 cells wide
w, h = (12*2+GAP)*CELL, 10*CELL
ox, oy = (N-w)//2, (N-h)//2 + CELL     # nudge down a little: legs are thin
x0, r = 100, 185                       # Apple icon grid: 824px rounded square, ~185px corners
def inside(x, y):
    if not (x0 <= x < N-x0 and x0 <= y < N-x0): return False
    cx = min(max(x, x0+r), N-x0-r); cy = min(max(y, x0+r), N-x0-r)
    return (x-cx)**2 + (y-cy)**2 <= r*r
rows = []
for y in range(N):
    row = bytearray(b"\0")
    for x in range(N):
        px = (0,0,0,0)
        if inside(x, y):
            px = BG + (255,)
            cx, cy = (x-ox)//CELL, (y-oy)//CELL
            if 0 <= cy < 10 and 0 <= x-ox < w:
                if cx < 12 and ART[cy][cx] == "#": px = A + (255,)
                elif cx >= 12+GAP and ART[cy][cx-12-GAP] == "#": px = B + (255,)
        row += bytes(px)
    rows.append(bytes(row))
def chunk(t, d): return struct.pack(">I", len(d)) + t + d + struct.pack(">I", zlib.crc32(t+d) & 0xffffffff)
png = b"\x89PNG\r\n\x1a\n" + chunk(b"IHDR", struct.pack(">IIBBBBB", N, N, 8, 6, 0, 0, 0)) + chunk(b"IDAT", zlib.compress(b"".join(rows), 9)) + chunk(b"IEND", b"")
open(sys.argv[1], "wb").write(png)
