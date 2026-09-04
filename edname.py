#!/usr/bin/env python3
"""Reference re-implementation of Elite Dangerous' system naming algorithm.

Recovered from EliteDangerous64.exe (4.4.1.x):
  GalaxyNames_SystemAddressToName  0x143CEE810   address -> "<sector> <AB-C> <mc><n1>-<n2>"
  SectorName_Generate              0x144CA0A80   sector key -> 1 or 2 procedural words
  SectorName_GenerateWord          0x144CA0FA0   mixed-radix word builder over fragment tables
  Boxel_IndexWithinSector          0x143CFC200   boxel index relative to sector/region origin
  SystemAddress_FromCoords32       0x143C570E0   1/32-ly integer coords -> address at mass code

Tables (prefix/infix/suffix fragments, mass-code letters, hand-authored regions) were
dumped from the running game (galaxy object +384 and the region octree at the naming
manager +360) and live in galaxy_name_tables.json next to this file.

Validated against Ardent Insight: 5747/5753 systems in seven neighbourhoods
(Sol, Colonia, Beagle Point, Maia, Betelgeuse, HIP 22460, Sadr); the six misses
carry placeholder coordinates in the source data and match once the address alone is used.

Usage:  edname.py <id64> [x y z]     (x y z = in-game ly coordinates, needed only for
                                      hand-authored "... Sector" regions)
"""
import json, os, re, sys

_HERE = os.path.dirname(os.path.abspath(__file__))
_T = json.load(open(os.path.join(_HERE, 'galaxy_name_tables.json')))
PREFIXES = [tuple(x) for x in _T['prefixes']]                      # (fragment, cumulative weight)
INFIX_V = [tuple(x) for x in _T['infix_after_vowel_initial']]
INFIX_C = [tuple(x) for x in _T['infix_after_consonant_initial']]
SUFFIX_V = _T['suffix_after_vowel_initial']
SUFFIX_C = _T['suffix_after_consonant_initial']
MASSCODES = _T['masscode_letters']                                  # "abcdefgh"
REGIONS = [(r['name'], r['sphere_center_galaxy_frame_ly'], r['radius_ly'] ** 2, r['origin_1_32_ly'])
           for r in _T['regions']]
VOWELS = set('AEIOUaeiou')
GALAXY_ORIGIN = (49985.0, 40985.0, 24105.0)   # in-game (0,0,0) = Sol, expressed in the galaxy frame


def wang32(k):
    """Thomas Wang 32-bit integer hash, exactly as compiled in SectorName_Generate."""
    k &= 0xFFFFFFFF
    k = (k * 4097) & 0xFFFFFFFF; k ^= k >> 22     # k += k << 12
    k = (k * 17) & 0xFFFFFFFF;   k ^= k >> 9      # k += k << 4
    k = (k * 1025) & 0xFFFFFFFF; k ^= k >> 2      # k += k << 10
    k = (k * 129) & 0xFFFFFFFF;  k ^= k >> 12     # k += k << 7
    return k


def gen_word(value):
    """SectorName_GenerateWord: mixed-radix decomposition of `value` over the fragment tables."""
    out = ''
    table = PREFIXES
    while True:
        total = table[-1][1]
        rem, quot = value % total, value // total
        lo_i, hi_i = 0, len(table)                 # upper_bound(cum, rem)
        while lo_i < hi_i:
            m = (lo_i + hi_i) // 2
            if table[m][1] <= rem: lo_i = m + 1
            else: hi_i = m
        frag, hi = table[lo_i]
        lo = table[lo_i - 1][1] if lo_i else 0
        out += frag
        value = rem + quot * (hi - lo) - lo        # residue re-encoded inside the chosen bucket
        if frag[0] in VOWELS:
            if value < len(SUFFIX_V): return out + SUFFIX_V[value]
            table = INFIX_V
        else:
            if value < len(SUFFIX_C): return out + SUFFIX_C[value]
            table = INFIX_C


def sector_name(key):
    """SectorName_Generate: key = xs | ys<<7 | zs<<14 (sector indices, 1280 ly sectors)."""
    nwords = 1 + (wang32(key) % (2 if key >= 0x4000 else 1))
    words, pos, i = [0] * nwords, [0] * nwords, 0
    while key:                                     # deal key bits round-robin into the words
        words[i] |= (key & 1) << pos[i]
        pos[i] += 1
        key >>= 1
        i = (i + 1) % nwords
    return ' '.join(gen_word(w) for w in words)


def decode(a):
    """SystemAddress bit layout: mc | z | y | x | n2 (widths 3, 14-mc, 13-mc, 14-mc, 3mc+11)."""
    mc = a & 7
    zb = (a >> 3) & ((1 << (14 - mc)) - 1)
    yb = (a >> (17 - mc)) & ((1 << (13 - mc)) - 1)
    xb = (a >> (30 - 2 * mc)) & ((1 << (14 - mc)) - 1)
    n2 = (a >> (44 - 3 * mc)) & ((1 << (3 * mc + 11)) - 1)
    return mc, xb, yb, zb, n2


def boxel_body(mc, idx, n2):
    n1, l = divmod(idx, 17576)                     # 26^3
    letters = chr(65 + l % 26) + chr(65 + (l // 26) % 26) + '-' + chr(65 + l // 676)
    return f'{letters} {MASSCODES[mc]}{n1}-{n2}' if n1 else f'{letters} {MASSCODES[mc]}{n2}'


def boxel_origin_ly(a):
    """In-game coordinates of the boxel's minimum corner and its size in ly."""
    mc, xb, yb, zb, _ = decode(a)
    return ((xb << mc) * 10 - GALAXY_ORIGIN[0], (yb << mc) * 10 - GALAXY_ORIGIN[1],
            (zb << mc) * 10 - GALAXY_ORIGIN[2], 10 << mc)


def procedural_name(a):
    mc, xb, yb, zb, n2 = decode(a)
    x10, y10, z10 = xb << mc, yb << mc, zb << mc              # coordinates in 10-ly units
    key = ((x10 >> 7) & 0x7F) | (((y10 >> 7) & 0x3F) << 7) | (((z10 >> 7) & 0x7F) << 14)
    m = (128 >> mc) - 1
    idx = (xb & m) | ((yb & m) << 7) | ((zb & m) << 14)
    return f'{sector_name(key)} {boxel_body(mc, idx, n2)}'


def find_region(x, y, z):
    """Smallest hand-authored region sphere containing the in-game point (x,y,z)."""
    gx, gy, gz = x + GALAXY_ORIGIN[0], y + GALAXY_ORIGIN[1], z + GALAXY_ORIGIN[2]
    best = None
    for name, (cx, cy, cz), r2, org in REGIONS:
        if (gx - cx) ** 2 + (gy - cy) ** 2 + (gz - cz) ** 2 < r2 and (best is None or r2 < best[1]):
            best = ((name, org), r2)
    return best[0] if best else None


def system_name(a, pos=None):
    """Full name. `pos` = in-game (x,y,z) of the star; without it regions cannot be applied."""
    reg = find_region(*pos) if pos is not None else None
    if reg is None:
        return procedural_name(a)
    name, (ox, oy, oz) = reg
    mc, xb, yb, zb, n2 = decode(a)
    snap = ~((1 << mc) - 1)                                   # region origin snapped to the boxel grid
    x10o, y10o, z10o = (ox // 320) & snap, (oy // 320) & snap, (oz // 320) & snap
    dx, dy, dz = (abs((xb << mc) - x10o) >> mc, abs((yb << mc) - y10o) >> mc,
                  abs((zb << mc) - z10o) >> mc)
    idx = (dx & 0x7F) | ((dy & 0x7F) << 7) | (dz << 14)
    return f'{name} {boxel_body(mc, idx, n2)}'


if __name__ == '__main__':
    if len(sys.argv) < 2:
        print(__doc__); sys.exit(1)
    a = int(sys.argv[1], 0)
    pos = tuple(map(float, sys.argv[2:5])) if len(sys.argv) >= 5 else None
    print(system_name(a, pos))
    print('boxel origin/size (ly):', boxel_origin_ly(a))
