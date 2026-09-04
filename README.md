# stellarforge — Elite Dangerous galaxy data, recovered

Everything in this directory comes from reverse-engineering `EliteDangerous64.exe` 4.4.1.x
(IDB `EliteDangerous64 (1).exe`, hash 10faff59fe1d; addresses verified byte-for-byte against the
on-disk 4.4.1.1 executable) and from read-only dumps of the running game taken through its
embedded Frida gadget on 2026-09-04. The game keeps its galaxy definition in `FREA` blobs under
`Win64/xx/<sha1>` that could not be decoded statically (section 7), so the tables were lifted from
live memory instead. The recovered naming algorithm reproduces 5747 of 5753 real system names
across seven Ardent Insight neighbourhoods; the six misses are rows whose Ardent coordinates lie
outside their own boxel (placeholder data) and they match on id64 alone.

## 1. What is in this directory

| path | what it is | produced by |
|---|---|---|
| `edgalaxy/` | Rust crate (library + CLI) that parses everything below; see section 2 and `edgalaxy/README.md` | hand-written |
| `galaxy_name_tables.json` | procedural-name fragment tables (111 weighted prefixes, 36 + 16 infixes, 151 + 35 suffixes), the mass-code letters `abcdefgh`, and 485 hand-authored regions (name, sphere centre/radius in the galaxy frame, 1/32-ly cube origin) | `frida_dump_naming_tables.js` + `frida_dump_regions.js` |
| `authored_systems.json.gz` | 143,667 hand-authored system records (Sol, Achenar, HIP/HD/Gliese catalogue stars, cluster spheres, …) with the raw 96-byte game record and decoded fields | `frida_dump_naming_tables.js` |
| `overrides.json` | 198 procedural systems the game renames (Colonia, Ceos, Beagle Point's neighbours, "Dr. Kay's Heart", …): id64, flags, name | live dump of the naming manager's override map |
| `density/` | the galaxy density model tables, RLE-decoded to flat binary arrays; see section 3 | live dump (RLE rows walked in Frida) |
| `density_map_dump.json.gz` | superseded first attempt at the density dump (object header + map headers only, rows missing); kept for provenance | — |
| `edname.py` | stand-alone Python reference implementation of the naming algorithm (needs `galaxy_name_tables.json` next to it); the Rust crate supersedes it | hand-written |
| `frida_dump_naming_tables.js`, `frida_dump_regions.js` | the read-only Frida scripts used for the dumps (section 5) | hand-written |

## 2. The `edgalaxy` crate

```
cd edgalaxy && cargo build --release        # zero warnings, `cargo test --release` re-runs the Ardent validation
target/release/edgalaxy stats
target/release/edgalaxy near Sol -r 20 --procedural
target/release/edgalaxy list --sphere -9530 -910 19808 100 --class M -f json
target/release/edgalaxy list -f csv > all_authored.csv      # all 143,667 records, ~0.6 s
target/release/edgalaxy name 3238296097059                  # Colonia (override) ; --at X Y Z for "... Sector" names
target/release/edgalaxy decode 263681363320754              # Boewnst KS-S c20-959: mass code, boxel, sector
target/release/edgalaxy boxels --sphere 0 0 0 100 -m d      # procedural boxel name prefixes in an area
target/release/edgalaxy sector Wregoe                       # reverse lookup over all 2^21 sector keys
target/release/edgalaxy regions --sphere 0 0 0 200
target/release/edgalaxy overrides
target/release/edgalaxy density 0 0 0                       # per-mass-code mass budget + population weights
```

The data directory is found automatically when the binary or the working directory is under
`stellarforge/`; otherwise pass `--data-dir` or set `EDGALAXY_DATA`. Output formats are table,
JSON and CSV. The crate loads all data in about 0.4 s and keeps a 128-ly grid index over the
authored systems for sphere/box queries. Modules: `id64` (address bit layout, boxel geometry),
`names` (sector words), `regions` (sphere octree contents), `records` (96-byte record decode),
`classes` (class-word enums), `spatial` (grid index), `density` (fixed-point port of the density
functions), `galaxy` (aggregate + queries).

What it cannot do: generate the procedural members (`n2`) of a boxel. That needs the full
StellarForge generator (section 6); the remaining tables it depends on (spectral temperature
bands filled by `StellarForge_InitSpectralTables` 0x143C44DC0) were not extracted.

## 3. `density/` — the galaxy density model

The generator's mass budget for every boxel comes from one object (galaxy + 248, class
`GalaxyDensityMapResource`). Its tables live in memory as run-length-encoded rows: each map is
`{u32 w; u32 h; u64 total; ptr rows}` with a per-row pointer table at +16, and each row is a
stream of run headers where the top bit marks a constant run (`u32 len|flag<<31` for the mips,
`u16 len|flag<<15` for the 16-bit tables) followed by one value (constant run) or `len` values.
The dump script walked every row and wrote flat little-endian arrays:

| file | shape | element | meaning |
|---|---|---|---|
| `density_object.bin` | 0x200 bytes | — | the object header: `u32[0]` overall scale, `u32[1]`/`u32[4]` x/z sampling scales, `u32[2]`/`u32[3]` height/radius scales for the cylindrical table, `u32[11..13]` galaxy centre ×20 (→ 50000, 40955, 50000 ly in the galaxy frame), `u32[83..97]` population thresholds/slopes |
| `mip_mc0.u32` … `mip_mc7.u32` | 2048², 1024², …, 16² | u32 | 2-D (x,z) stellar-mass density, one mip per mass code a…h |
| `cyl.u16` | 2048 × 2048 | u16 | summed-area profile indexed by (scaled height, scaled radius); eight corner look-ups give the boxel's vertical/radial factor |
| `weights.u16` | 2048 × 2048 | u16 | population-mix control map; thresholds in the object turn it into three population weights (age/metallicity table selection) |
| `manifest.json` | — | — | names and dimensions of the above |

`edgalaxy density X Y Z` and `edgalaxy::density` port the game's fixed-point arithmetic literally
(`GalaxyDensity_BoxelMass` 0x144CA2790, `GalaxyDensity_CylSummedArea` 0x144CA2E40, bilinear
sampler 0x144CA3350 with 9-bit fractional coordinates and 15-bit weights, population weights
0x143C3CB30). The outputs are the raw numbers the generator consumes (budget ×256 → 1<<24 = one
unit of the per-mass-code factor); they are not externally validated, unlike the naming code.

## 4. Provenance and validation

- Static analysis via the IDA MCP bridge; 30 functions renamed in the IDB (names used below).
- Dynamic dumps: `frida -R -F -l <script> -q` (the game embeds a Frida gadget; `-p`/`-n` fail;
  no instruction-level hooks on this Wine build). The scripts only read memory. The naming
  manager subobject is found by scanning rw- memory for the vtable pointer `base + 0x553CA48`;
  from it: +88 → galaxy object, galaxy+384 → naming tables/authored records/catalogues,
  galaxy+248 → density object, +232 → override map, +360 → region octree, +72 → mass-code letters.
- Validation data: Ardent Insight `v2/system/name/<name>/nearby` dumps for Sol, Colonia,
  Beagle Point, Maia, Betelgeuse, HIP 22460 and Sadr (`edgalaxy/testdata/`), checked by
  `edgalaxy verify` and the integration test.

## 5. Re-dumping

With the game running: `frida -R -F -l frida_dump_naming_tables.js -q` writes the raw dump
(`dump.json` path inside the script), `frida_dump_regions.js` walks the region octree. The
density and override dumps were one-off probes with the same structure (scan for the vtable,
follow the offsets above); their logic is described in sections 3 and 6d.

## 6. How the game does it (reverse-engineering notes)

### 6a. SystemAddress (id64) layout — `GalaxyNames_SystemAddressToName` 0x143CEE810

| bits | field |
|---|---|
| 0-2 | mass code `mc` (0=a … 7=h). Boxel edge = `10 << mc` ly |
| 3 … | z boxel index, `14-mc` bits |
| next | y boxel index, `13-mc` bits (bit `17-mc`) |
| next | x boxel index, `14-mc` bits (bit `30-2mc`) |
| `44-3mc` … | n2 = system number inside the boxel, `3mc+11` bits |

Boxel indices are in units of the boxel edge; `<< mc` converts to 10-ly units. The galaxy frame
origin is in-game `(-49985, -40985, -24105)` (Sol's authored record sits at 1/32-ly offset
(2080,800,800) inside boxel origin (-65,-25,-25), i.e. exactly (0,0,0)). Sectors are 1280 ly
(7 bits x, 6 bits y, 7 bits z). Internal positions are 1/32-ly integers; the galaxy extent check
in `SystemAddress_FromCoords32` (0x143C570E0) is x,z < 163840 ly, y < 81920 ly.

### 6b. Name resolution order
`GalaxyNames_SystemAddressToName` tries, in order: (1) the override map (+232, section 6d) —
a renamed procedural system such as Colonia; (2) the region octree (+360) — a hand-authored
"… Sector" name with its own boxel-grid origin; (3) the procedural sector word(s). Authored
systems (Sol, Achenar, …) carry their own name in their record. `edgalaxy name` follows the same
order.

### 6c. Procedural sector word(s) — `SectorName_Generate` 0x144CA0A80
1. `key = xs | ys<<7 | zs<<14` (sector indices).
2. `h = wang32(key)` (Thomas Wang: `k+=k<<12; k^=k>>22; k+=k<<4; k^=k>>9; k+=k<<10; k^=k>>2; k+=k<<7; k^=k>>12`).
3. Word count = `1 + h % 2`, forced to 1 when `key < 0x4000` (z-sector 0).
4. The key's bits are dealt round-robin into the words (word k gets bits k, k+n, …); each word
   is built by `SectorName_GenerateWord` (0x144CA0FA0); words are joined by a space.

Word builder, starting with table = prefixes (cumulative weights):
```
total = table[-1].cum; rem = v % total; q = v / total
i = upper_bound(cum, rem); emit table[i].str; lo = cum[i-1] (0 if i==0); hi = cum[i]
v = rem + q*(hi-lo) - lo
if str[0] in AEIOUaeiou: (v < len(suffix_v)) ? emit suffix_v[v], stop : table = infix_v, loop
else:                    (v < len(suffix_c)) ? emit suffix_c[v], stop : table = infix_c, loop
```
(The branch keys on the **first** letter of the fragment just emitted.)

Boxel code: `idx = bx | by<<7 | bz<<14` with `b* = boxel index mod (128>>mc)` relative to the
sector origin (`Boxel_IndexWithinSector` 0x143CFC200). `n1, l = divmod(idx, 17576)`; letters =
`chr(65+l%26) chr(65+l/26%26) '-' chr(65+l/676)`. Output `"<sector> <AB-C> <mc><n1>-<n2>"`, or
`"<sector> <AB-C> <mc><n2>"` when n1 == 0. The reverse parser is
`GalaxyNames_ParseProceduralName` 0x143CDCAE0.

### 6d. Hand-authored regions, authored systems, overrides
- Regions: an octree of spheres (nodes: entry list at +16, 8 children +24..+80, centre +112,
  half-size +128; entries: centre +56/+60/+64, radius² +68, region struct at +80 with `char*`
  name at 0 and the 1/32-ly cube origin at +24/+28/+32). The smallest containing sphere wins
  (ties by traversal order). The boxel index is relative to that origin snapped to the boxel grid:
  `d = |x10 - (origin/320 & ~((1<<mc)-1))| >> mc`. Examples: Col 285 Sector r = 326 ly at in-game
  origin (-379.5, -269.7, -345.4); Core Sys and Jastreb r = 50; Col 173 r = 500; Pleiades r = 100.
- Authored records (96 bytes; hashed by Wang64 of the coordinate part into 8 per-mass-code
  bucket tables at naming-object+16; catalogue arrays at +48/+64/+80): `+0` id64, `+8 char*` name,
  `+16` HIP, `+20` HD, `+24 char*` Gliese, `+36` cluster id (-1 none), `+40` mass×256, `+44` absolute
  magnitude×65536 (signed), `+48` temperature K, `+52` radius×32768, `+60` age Myr, `+62`
  metallicity index, `+64` class word (bits 0-5 class, 6-9 subclass, 10-14 luminosity class),
  `+66/68/70` position in the boxel in 1/32 ly, `+88` kind (5 star system, 0 cluster sphere).
- Overrides (+232 object, vtable 0x14553D400): two-level intrusive hash map, outer keyed by the
  coordinate part (107 entries, 163 buckets), inner by full id64; value = record with id64 at +0,
  `char*` name at +8, flag 0x8000 at +60. 198 renamed systems.

### 6e. Star generation and placement — `GalaxyOctree_GenerateBoxel` 0x143C33F60
Triggered by `GalaxyOctree::SectorGenerators` jobs (vtable 0x1455360E8); boxels are generated
top-down (h → a), each child needing its parent (`sub_143C32AD0`).
1. **Mass budget**: `GalaxyDensity_BoxelMass` (section 3) × (1 − parent's consumed fraction at
   boxel+200) × per-mass-code factor (`qword_145FDBC60`/`CA0` set in
   `StellarForge_InitGenerationConstants` 0x143C404D0); star mass range per mass code from
   `dword_145FDB658`. Population type 0/1/2 (`GalaxyDensity_RegionType` 0x143C3CB30) picks the
   age/metallicity tables (hard-coded in 0x143C43AD0 / 0x143C447A0, 100-entry Myr lists).
2. **RNG**: MT19937 seeded from the boxel address (`MT19937_Seed64` 0x142013010); each system
   additionally gets a MINSTD (a=48271, m=2³¹−1) seeded with the Wang 64→32 hash of its full
   id64, so a system's stars depend only on its address.
3. **Authored content first**: records from `AuthoredSystems_LookupBoxel` (0x144C9FFB0) are
   placed at their stored position and their mass is deducted; kind-0 records define cluster
   spheres whose per-mass-code budgets come from `qword_145FDBC20..58`
   (`GalaxyOctree_GenerateClusterSystem` 0x143C3AD80). h-boxels in the galactic-plane row get
   extra young massive stars (age 100–225 Myr) within ±40 ly of the plane.
4. **Procedural fill** (`GalaxyOctree_GenerateOneSystem` 0x143C399E0): n2 increments from the
   boxel's counter; position = random cell of a 32³ grid (one system per cell, up to 5 retries;
   50 % chance for mc<6 to sit within ±3 cells of the previous system); mass uniform in the
   mass-code range (10 % of a-boxel objects become class 0x2E sub-stellar); age from the
   population table (50 % as-is, 25 % ×1.4, 25 % halved); metallicity uniform in range; stars
   inside an authored nebula sphere get young ages (85 %). Stops when the budget or
   `1 << (3mc+11)` systems is reached.
5. **Evolution** (`Star_Evolve` 0x143C3BF60): MS lifetime `11000·M^-2.5` Myr; T =
   `5778·M^1.25·U(0.9,1.1)·metallicity factor`; class from temperature tables
   (`Star_ClassFromTemperature` 0x143C3DA60, filled by 0x143C44DC0). Branches: pre-MS → TTS
   (≤3 M☉) / AeBe; giants and carbon/S stars (classes 0x11–0x18); Wolf-Rayet WN/WNC/WC/WO
   (0xD–0x10, 50 %); remnants → white dwarf subtypes 0x1A–0x26, neutron star 0x28, black hole
   0x29 (`Star_Remnant_WDorNSorBH` 0x143C388E0); 0.5–1 % of >70 M☉ stars become Nebula objects
   (0x2C); mass lost by an evolved star spawns a RemnantNebula (0x2D) companion record at the
   same position. Class-name enum at 0x145E9AB70 (O,B,A,F,G,K,M,L,T,TTS,AeBe,Y,W,WN,WNC,WC,WO,
   CS,C,CN,CJ,CH,CHd,MS,S,D,DA,DAB,DAO,DAZ,DAV,DB,DBZ,DBV,DO,DOV,DQ,DC,DCV,DX,N,H,
   SupermassiveBlackHole,X,Nebula,RemnantNebula,BigText,NoSystem); luminosity classes I…VII at
   0x145E9AA98.

## 7. The game's data files
- `Win64/xx/<sha1>`: 102,534 blobs. ~48k start `78 9C` (zlib → a `02 00 01 00` container);
  ~54k start `FREA 00 00` followed by high-entropy data. FREA is deterministic (identical content
  ⇒ identical bytes under different names) and is neither zlib, Oodle (Oodle in this exe is
  network-only) nor referenced by any `FREA` constant in the code; names are not SHA-1 of the
  content or of the plain resource path. Not decoded statically.
- `Win64/data.ovx`: integrity manifest (`DataOvx_ParseManifest` 0x1428BA300): 170-byte lines =
  40-hex SHA-1 name + 128-hex digest + CRLF (digest is not SHA-1/256/512 of the file).
- Relevant resources named in the exe: `Win64/Shared/StellarForge/Galaxies/<galaxy>/
  {StarSystems/StarSystems, MassiveStars/MassiveStars, GalaxyRegions/GalaxyRegions,
  Overrides/Overrides}`, `GalaxyDensityMapResource`, `Nebulae`. The name-fragment tables were
  found in none of the 47,310 zlib blobs, so they live in FREA blobs — hence the live dumps.
