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
* `./build.sh` (cargo -j4 under a 6 GB scope) -> `dev/`. `python/` is the released copy imported by session 176's hooks; only `./release.sh` (smoke differential tests first) writes it.
* Differential tests (oracle = pure Python): `tests/diff_decoder.py`, `diff_extract.py` (`--want-all|--default|--slim`), `diff_g103.py`, `diff_es.py`, `diff_params.py`,
  `diff_mesh.py --impl rust`; corpus runs: `tests/run_all.sh`, `tests/run_more.sh`; oracle edits since the last port: `tests/oracle_watch.sh`.
* Results: decoder, extraction, channel-103 index, ES schemas, params: identical (dict equality incl. key order, types, float bits). Meshes: bit-identical, except where the oracle builds
  face loops through `_cycles_from_polylines` (it iterates Python sets, so cycle start vertices follow CPython's hash layout): there the port gives the same triangle count, area and
  vertex set per face (geometrically equivalent; asked 176 to make that loop order-independent).

## Timings
TIMINGS_TABLE
