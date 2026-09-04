# edgalaxy

Command-line explorer for the StellarForge data recovered from `EliteDangerous64.exe`
(see `../README.md`). It parses the files in the parent directory:

| file | content |
|---|---|
| `galaxy_name_tables.json` | 111 weighted prefixes, 36 + 16 infixes, 151 + 35 suffixes, mass-code letters, 485 hand-authored regions with sphere and boxel-grid origin |
| `authored_systems.json.gz` | 143,667 hand-authored system records (96-byte game records: id64, name, HIP/HD/Gliese ids, mass, radius, temperature, absolute magnitude, age, metallicity, class word, sub-boxel position, kind) |
| `overrides.json` | 198 renamed procedural systems (Colonia, Ceos, …) from the game's override map |
| `density/` | eight RLE-decoded 2-D density mips (2048² … 16²), the vertical/radial profile table and the population-weight table, plus the 0x200-byte density object |

The naming algorithm reproduces every one of the 5747 procedural names in seven Ardent
Insight neighbourhoods whose coordinates are consistent (`cargo test --release`).

## Build

```
cargo build --release
```

The data directory is found automatically when the binary or the working directory sits
under `stellarforge/`; otherwise pass `--data-dir` or set `EDGALAXY_DATA`.

## Commands

```
edgalaxy name 3238296097059                      # -> Colonia (authored) ; --at X Y Z for region names
edgalaxy decode 263681363320754                  # mass code, boxel, sector, procedural name
edgalaxy list --sphere 0 0 0 100 --class M        # authored systems in a sphere, filtered
edgalaxy list --box -50 -50 -50 50 50 50 -f json  # or in a box; no area = all 143,667
edgalaxy near Sol -r 20 --procedural              # neighbourhood of a named system / id64
edgalaxy find Achenar --exact -f json             # by name, Gliese id, HIP or HD number
edgalaxy boxels --sphere 0 0 0 100 -m d           # procedural boxel name prefixes in an area
edgalaxy sector Wregoe                            # where a procedural sector name lives
edgalaxy regions --sphere 0 0 0 200               # hand-authored "... Sector" spheres
edgalaxy overrides                               # renamed procedural systems (Colonia, ...)
edgalaxy density -9530.5 -910.28 19808.1          # per-mass-code mass budget and population weights
edgalaxy stats
edgalaxy verify testdata/*.json                   # naming vs Ardent Insight dumps
```

Output formats: `table` (default), `json`, `csv`; `--limit`, `--sort distance|name|mass|address`.

## Notes on fidelity

* id64 decoding, the two-level (sector word / boxel code) naming, and region handling are
  literal ports of the game functions and are validated externally.
* Star properties come straight from the authored records; the class word decodes to
  spectral class, subclass and luminosity class (e.g. Sol `G2 Va`, Sirius B `DA`).
* The density model is a literal port of the game's fixed-point code operating on the dumped
  tables. Its outputs are the raw budget numbers the generator consumes; they are not
  externally validated. When two hand-authored region spheres of identical radius overlap the
  game breaks the tie by octree traversal order; the dumped order is used here.
* Procedural systems themselves (the `n2` members of a boxel) are not generated: that needs
  the full StellarForge generator, whose remaining tables (spectral temperature bands) were
  not extracted. `boxels` lists their name prefixes and the maximum member count instead.
