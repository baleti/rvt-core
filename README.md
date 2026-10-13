# rvt-core

Rust hot core (PyO3, abi3) for the Revit file reader in `~/rvt-research` (session 176). Own code only: no ODA material, nothing copied from rvt-rs or other projects.
The Python reader stays the reference oracle: every fast path has the same signature and result as the oracle function and is checked by a differential test against the
pure-Python oracle (`RVT_NOFAST=1`) over the corpus (2017-2026, kept local).

## Layout
| file | port of |
|---|---|
| `src/dec.rs`, `src/conv.rs` | `rvt_decode.Dec` (GIL-free decoding into a value tree, then identical Python objects; exact error texts incl. CPython `struct.error`) |
| `src/extract.rs` | the partition loop of `rvt_entities.extract` (tagged candidate scan, parallel decode, sequential first-wins merge) |
| `src/g103.rs` | `g103_get.Part` / `build_index` / `get_record` (segments, channel-103 groups, record chains) |
| `src/es.rs` | `rvt_es.find_schemas` (extensible-storage schemas) |
| `src/params.rs` | `rvt_ifc.stored`, `rvt_map2.Src` / `resolve` |
| `src/mesh.rs`, `src/elemmesh.rs` | `rvt_mesh` (surfaces, curves, face loops, tessellation, closed-shell selection, T-junctions) and `rvt_elemmesh.element_mesh` |
| `src/cache.rs`, `src/pack.rs` | binary cache (no pickle): extraction blocks (mmap), packed params / compact meshes |
| `python/rvt_fast.py` | same-signature adapters, `on()` switch, fast `Doc`, `Model` (progressive open), `Progress`, `MeshStream` |

Switch: `RVT_NOFAST=1` = everything runs the oracle. `RVT_NOCACHE=1` / `RVT_CORE_NOCACHE=1` = no cache. `RVT_THREADS` = worker threads (default min(6, cpus)).
Hooks in the oracle and the viewer API: `~/rvt-research/RUST_INTEGRATION.md`.

## Build, test, release
* Long corpus runs go to remote-host: `h1run run NAME --env rvt --dir ~/src/rvt-core -- tests/h1_suite.sh SUITE...` (logs/h1_*.log, pulled back with `h1run pull src/rvt-core/logs`).
* `./build.sh` (cargo -j4 under a 6 GB scope) -> `dev/`. `python/` is the released copy imported by session 176's hooks; only `./release.sh` (smoke differential tests first) writes it.
* Differential tests (oracle = pure Python): `tests/diff_decoder.py`, `diff_extract.py` (`--want-all|--default|--slim`), `diff_g103.py`, `diff_es.py`, `diff_params.py`,
  `diff_mesh.py --impl rust`; corpus runs: `tests/run_all.sh`, `tests/run_more.sh`; oracle edits since the last port: `tests/oracle_watch.sh`.
* Results: decoder, extraction, channel-103 index, ES schemas, params: identical (dict equality incl. key order, types, float bits). Meshes: bit-identical, except where the oracle builds
  face loops through `_cycles_from_polylines` (it iterates Python sets, so cycle start vertices follow CPython's hash layout): there the port gives the same triangle count, area and
  vertex set per face (geometrically equivalent; asked 176 to make that loop order-independent).

## Timings
remote-host (12 threads, at most 6 used), shared with other sessions (load average 5-16 during the runs): each value is the best of two benchmark sessions. `bench/pipeline.py`, `bench/run_bench.sh`.
"before" = the pure-Python reader (RVT_NOFAST=1, its default worker pool inside a 6 GB scope); element_mesh measured on 200 placed elements and extrapolated to all of them.
"first open" = file never opened before (no rvt-core cache, no extensible-storage cache for the file; the Formats/Latest parse cache shared by all files of a Revit build present). "cold" = RVT_NOCACHE=1, no cache at all. "warm" = second open.
Stages of `rvt_fast.Model`: element table + levels + grids (quick), full model = extraction + params (what the viewer's reader.load returns), meshes = all placed elements with saved graphics (started right after the table, run alongside the full model).

| file | size | before: extract / params / graphics index / meshes (est.) / total | first open: table / full model / first meshes / all meshes | cold: table / full / all meshes | warm: table / full / all meshes |
|---|---|---|---|---|---|
| 30_rst_basic | 6 MB | 13.1 / 2.3 / 3.5 / 54 s (6002 el.) / **73 s** | **0.8** / 3.5 / 1.8 / 3.6 s (6002 el., 0.1M tris) | 0.8 / 4.2 / 3.8 s | 0.07 / 0.8 / 0.13 s |
| 43_Snowdon_Towers Structural | 28 MB | 87.8 / 3.6 / 16.6 / 291 s (2705 el.) / **399 s** | **1.9** / 7.5 / 3.9 / 8.4 s (2705 el., 0.6M tris) | 1.7 / 6.1 / 6.7 s | 0.08 / 1.3 / 0.13 s |
| 44_Snowdon_Towers Electrical | 40 MB | 19.7 / 5.3 / 22.8 / 395 s (4435 el.) / **443 s** | **1.6** / 6.1 / 3.5 / 9.4 s (4435 el., 2.1M tris) | 1.6 / 5.8 / 7.1 s | 0.10 / 1.7 / 0.28 s |
| 52_Snowdon_Towers Facades | 53 MB | 9.5 / 2.4 / 21.3 / 74 s (1000 el.) / **107 s** | **0.9** / 3.2 / 2.2 / 4.1 s (1000 el., 0.2M tris) | 0.9 / 3.4 / 4.5 s | 0.07 / 0.9 / 0.10 s |

Per component, same machine (measured during development, single runs):

| component | before | after |
|---|---|---|
| extract(want_all), 40 MB Electrical | 9.7 s (6 workers) / 12.8 s (1) | 1.8 s |
| extract(want_all), 28 MB Structural (pure Python, 1 worker) | 151 s | 2.8 s |
| channel-103 index, 28 MB Structural | 15.7 s | 1.0 s |
| extensible-storage schema scan, all 78 corpus files | 306 s | 8.2 s |
| params, 40 MB Electrical | 4.8 s | 1.0 s |
| element meshes, 2,705 placed elements, 28 MB Structural | 291 s pure Python (est.), ~125 s Python mesh code on the fast Doc | 2.0 s (Rust, 6 threads) |
| extraction reload, 40 MB (pickle of the same object: 1.0 s) | - | 1.0 s (.rvtc, mmap) |

## Against the targets (2026-10-13, remote-host, other sessions active, load 3-6)
| target | measured | status |
|---|---|---|
| 6 MB full model (table + extraction + params) first open < 2 s | 1.74-1.77 s (table 0.72-0.78 s) | met (was 2.3 s: GC frozen before re-enabling) |
| 40 MB usable model < 8 s cold | table 1.5 s, full model 5.4 s with all meshes computing alongside | met |
| cached full model < 0.5 s | 6 MB 0.51 s (was 0.94 s), 40 MB 1.7 s | 6 MB at target; 40 MB bound by building ~25M Python objects (the viewer reopens its own converted model instead) |
| all meshes, 40 MB first open | 12.1 s (2.9M triangles; the reference's curved-face refinement adds 38% triangles), warm 0.3 s | streamed |
| viewer item 13 (Facades prefetch) | Rust stream 1.1 s of the stage; the rest was the viewer's refined_volume check: Rust drop-in 34 s -> 0.6 s | drop-in released |

## Differential status (reference = pure Python, released build)
* Extraction want_all 76/76 files identical (2 files killed by the per-file memory cap), default 26/26, slim 25/25; channel-103 index 78/78; ES schemas 78/78.
* Params: 70/70 identical (8 rme files over the 6 GB per-file cap, re-run at 8 GB); the port mirrors rvt_params.py of 2026-10-12 21:30; any newer rvt_params.py runs the reference loop with the Rust kernels (drift guard).
* Meshes: bit-identical incl. per-triangle `mat` (300-400 sampled elements each: 2020 Residential, 2022 rst_basic, 2026 Facades, 2026 Site); uv containers (list vs tuple) are not compared.
  Corpus run of the current release: remote-host (tests/h1_suite.sh mesh refvol params).
* refined_volume (Rust) vs reference: equal to 1e-9 relative (tests/diff_refvol.py).
* Not supported by the Rust mesh path: the reference's experimental RVT_STRIP mode.
