# StellarForge: star generation, placement and naming (EliteDangerous64.exe 4.4.1.x)

Recovered 2026-09-04 from the IDB `EliteDangerous64 (1).exe` (hash 10faff59fe1d, addresses
verified byte-for-byte against the on-disk 4.4.1.1 executable) plus a read-only dump of the
running game through its embedded Frida gadget. Everything below was cross-checked against
Ardent Insight (`api.ardent-insight.com/v2/system/name/...`): the re-implementation in
`stellarforge/edname.py` reproduces **5747 of 5753** real system names in seven
neighbourhoods; the six misses all carry the same placeholder coordinates in Ardent and
match once only the id64 is used. 30 functions were renamed in the IDB (names used below).

## 1. SystemAddress (id64) layout — `GalaxyNames_SystemAddressToName` 0x143CEE810

| bits | field |
|---|---|
| 0-2 | mass code `mc` (0=a … 7=h). Boxel edge = `10 << mc` ly |
| 3 … | z boxel index, `14-mc` bits |
| next | y boxel index, `13-mc` bits (bit `17-mc`) |
| next | x boxel index, `14-mc` bits (bit `30-2mc`) |
| `44-3mc` … | n2 = system number inside the boxel, `3mc+11` bits |

Boxel indices are in units of the boxel edge; `<< mc` converts to 10-ly units. Galaxy frame
origin is in-game `(-49985, -40985, -24105)` (Sol's authored record sits at 1/32-ly offset
(2080,800,800) inside boxel origin (-65,-25,-25), i.e. exactly (0,0,0)). Sectors are 1280 ly
(7 bits x, 6 bits y, 7 bits z). Internal positions are 1/32-ly integers; the galaxy extent
check in `SystemAddress_FromCoords32` (0x143C570E0) is x,z < 163840 ly, y < 81920 ly.

## 2. Naming

### 2a. Procedural sector word(s) — `SectorName_Generate` 0x144CA0A80
1. `key = xs | ys<<7 | zs<<14` (sector indices).
2. `h = wang32(key)` (Thomas Wang: `k+=k<<12; k^=k>>22; k+=k<<4; k^=k>>9; k+=k<<10; k^=k>>2; k+=k<<7; k^=k>>12`).
3. Word count = `1 + h % 2`, forced to 1 when `key < 0x4000` (z-sector 0).
4. The key's bits are dealt round-robin into the words (word k gets bits k, k+n, …), each
   word is built by `SectorName_GenerateWord`, words joined by a space.

### 2b. Word builder — `SectorName_GenerateWord` 0x144CA0FA0
Tables (dumped, in `stellarforge/galaxy_name_tables.json`): 111 weighted prefixes
(`Th`35, `Eo`35, `Oo`35, `Eu`31, `Tr`35, `Sly`4, `Dry`35, …; cumulative weights),
36 weighted infixes used after a vowel-initial fragment, 16 after a consonant-initial one,
151 suffixes after vowel-initial, 35 after consonant-initial. Algorithm, table = prefixes:
```
total = table[-1].cum; rem = v % total; q = v / total
i = upper_bound(cum, rem); emit table[i].str; lo = cum[i-1] (0 if i==0); hi = cum[i]
v = rem + q*(hi-lo) - lo
if str[0] in AEIOUaeiou: (v < len(suffix_v)) ? emit suffix_v[v], stop : table = infix_v, loop
else:                    (v < len(suffix_c)) ? emit suffix_c[v], stop : table = infix_c, loop
```
(The branch keys on the **first** letter of the fragment just emitted.)

### 2c. Boxel code
`idx = bx | by<<7 | bz<<14` with `b* = boxel index mod (128>>mc)` relative to the sector
origin (`Boxel_IndexWithinSector` 0x143CFC200). `n1, l = divmod(idx, 17576)`; letters =
`chr(65+l%26) chr(65+l/26%26) '-' chr(65+l/676)`. Output `"<sector> <AB-C> <mc><n1>-<n2>"`,
or `"<sector> <AB-C> <mc><n2>"` when n1 == 0. Mass-code letters table = `"abcdefgh"`.
The reverse parser is `GalaxyNames_ParseProceduralName` 0x143CDCAE0 (rebuilds the address as
`base | n2 << (44-3mc)`).

### 2d. Hand-authored "… Sector" regions
Before the procedural path, the star position is tested against an octree of spheres
(naming manager +360; nodes: entry list at +16, 8 children +24..+80, centre +112, half-size
+128; entries: centre +56/+60/+64, radius² +68, region struct at +80 with `char* name` at 0
and the 1/32-ly cube origin at +24/+28/+32). The smallest containing sphere wins (ties by
traversal order). The boxel index is then taken relative to that region's origin snapped to the
boxel grid: `d = |x10 - (origin/320 & ~((1<<mc)-1))| >> mc`. 485 named regions were dumped
(e.g. Col 285 Sector r = 326 ly at in-game origin (-379.5, -269.7, -345.4); Core Sys and
Jastreb r = 50; Col 173 r = 500; Pleiades r = 100; Hyades r = 144). Catalogue names
(HIP 113,795; HD 97,102; Gliese 3,336) and 143,667 authored system records (96 bytes:
address, name ptr +8, HIP +16, HD +20, GL ptr +24, mass×256 +40, radius +52, age Myr +60,
metallicity +62, class bits +64, 1/32-ly position +66/68/70, type +88: 5 = star,
0 = cluster sphere) are in the same naming object (+16 buckets, +48/+64/+80 catalogues).

## 3. Star generation and placement — `GalaxyOctree_GenerateBoxel` 0x143C33F60
Triggered by `GalaxyOctree::SectorGenerators` jobs (vtable 0x1455360E8); boxels are generated
top-down (h → a), each child needing its parent (`sub_143C32AD0`).
1. **Mass budget**: `GalaxyDensity_BoxelMass` (0x144CA2790) = bilinear sample of a 2D
   density mip for that mass code (2048² for a … 16² for h, galaxy+248, from
   `GalaxyDensityMapResource`) × a cylindrical (radius, height) summed-area factor
   (`GalaxyDensity_CylSummedArea` 0x144CA2E40). Budget × (1 − parent's consumed fraction at
   boxel+200) × per-mass-code factor (`qword_145FDBC60`/`CA0` set in
   `StellarForge_InitGenerationConstants` 0x143C404D0); star mass range per mass code from
   `dword_145FDB658`. Population type 0/1/2 (`GalaxyDensity_RegionType` 0x143C3CB30) picks the
   age/metallicity tables (hard-coded in 0x143C43AD0 / 0x143C447A0, 100-entry Myr lists).
2. **RNG**: MT19937 seeded from the boxel address (`MT19937_Seed64` 0x142013010); each
   system additionally gets a MINSTD (a=48271, m=2³¹−1) seeded with the Wang 64→32 hash of its
   full SystemAddress, so a system's stars depend only on its address.
3. **Authored content first**: records from `AuthoredSystems_LookupBoxel` (0x144C9FFB0) are
   placed at their stored 1/32-ly position and their mass is deducted; type-0 records define
   cluster spheres whose per-mass-code budgets come from `qword_145FDBC20..58`
   (`GalaxyOctree_GenerateClusterSystem` 0x143C3AD80). h-boxels in the galactic-plane row get
   extra young massive stars (age 100–225 Myr) within ±40 ly of the plane.
4. **Procedural fill** (`GalaxyOctree_GenerateOneSystem` 0x143C399E0): n2 increments from
   the boxel's counter; position = random cell of a 32³ grid (one system per cell, up to 5
   retries; 50 % chance for mc<6 to sit within ±3 cells of the previous system); mass uniform
   in the mass-code range (10 % of a-boxel objects become class 0x2E sub-stellar); age from the
   population table (50 % as-is, 25 % ×1.4, 25 % halved); metallicity uniform in range; stars
   inside an authored nebula sphere get young ages (85 %). Stops when the budget or
   `1 << (3mc+11)` systems is reached.
5. **Evolution** (`Star_Evolve` 0x143C3BF60): MS lifetime `11000·M^-2.5` Myr; T =
   `5778·M^1.25·U(0.9,1.1)·metallicity factor`; class from temperature tables
   (`Star_ClassFromTemperature` 0x143C3DA60, tables filled by 0x143C44DC0). Branches:
   pre-MS → TTS (≤3 M☉) / AeBe; giants and carbon/S stars (classes 0x11–0x18); Wolf-Rayet
   WN/WNC/WC/WO (0xD–0x10, 50 %); remnants → white dwarf subtypes 0x1A–0x26, neutron star
   0x28, black hole 0x29 (`Star_Remnant_WDorNSorBH` 0x143C388E0); 0.5–1 % of >70 M☉ stars
   become Nebula objects (0x2C); mass lost by an evolved star spawns a RemnantNebula (0x2D)
   companion record at the same position. Class-name enum at 0x145E9AB70
   (O,B,A,F,G,K,M,L,T,TTS,AeBe,Y,W,WN,WNC,WC,WO,CS,C,CN,CJ,CH,CHd,MS,S,D,DA,DAB,DAO,DAZ,DAV,
   DB,DBZ,DBV,DO,DOV,DQ,DC,DCV,DX,N,H,SupermassiveBlackHole,X,Nebula,RemnantNebula,BigText,
   NoSystem); luminosity classes I…VII at 0x145E9AA98 (bits 10-14 of the class word).

## 4. The data files
- `Win64/xx/<sha1>`: 102,534 blobs. ~48k start `78 9C` (zlib → a `02 00 01 00` container);
  ~54k start `FREA 00 00` followed by high-entropy data. FREA is deterministic (identical
  content ⇒ identical bytes under different names) and is neither zlib, Oodle (Oodle in this
  exe is network-only) nor referenced by any `FREA` constant in the code; names are not SHA-1
  of content or of the plain resource path. Not decoded statically.
- `Win64/data.ovx`: integrity manifest (`DataOvx_ParseManifest` 0x1428BA300): 170-byte
  lines = 40-hex SHA-1 name + 128-hex digest + CRLF (digest is not SHA-1/256/512 of the file).
- Relevant resources named in the exe: `Win64/Shared/StellarForge/Galaxies/<galaxy>/
  {StarSystems/StarSystems, MassiveStars/MassiveStars, GalaxyRegions/GalaxyRegions,
  Overrides/Overrides}`, `GalaxyDensityMapResource`, `Nebulae`. The name-fragment tables were
  found in **no** zlib blob (all 47,310 scanned), so they live in FREA blobs; they were taken
  from live memory instead (`stellarforge/frida_dump_naming_tables.js`, `frida_dump_regions.js`,
  attach with `frida -R -F -l <script> -q`; the naming manager is found by scanning rw- memory
  for vtable `base+0x553CA48`, then +88 → galaxy, galaxy+384 → tables, galaxy+248 → density map).

## 5. Files
`stellarforge/edname.py` (reference implementation + CLI), `galaxy_name_tables.json`
(fragments, mass codes, 485 regions), `authored_systems.json.gz` (143,667 records),
`density_map_dump.json.gz` (density object + all eight mips), the two Frida dump scripts.
