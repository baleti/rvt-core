# HANDOFF c3-rust (claude3, 2026-10-18). Supersedes HANDOFF-c5-rust.md for state; its Workflow section is still valid.

## State (everything committed on master; released build = python/ + ~/rvt-research/rvt_core.abi3.so; demo smoke OK x3 at the last release)
* Differential suites: main corpus 78/78 + extras 24/24 identical (extract, g103, params, mesh, refvol, stream); mesh_all re-run 2026-10-17 after the last mesh port: 78/78. Write-lab outputs (161 files, FILELIST=logs/wall_files.txt LOGP=wa_): g103 / params / extract / mesh all identical.
* Released this session: smooth_part (viewer adopted), inflate member check fix (bufread DeflateDecoder: a gzip member with trailing bytes truncated the partition, write-lab e3_noframe), col_grid / subdivide_grid / rect_grid cap (reference rvt_mesh 2026-10-17), topography material wrapper (rvt_fast._topo_mat, needs the FULL record via extract_shared: Doc.rec is the graphics record only), NaN-safe sort comparator fcmp (fuzz_tess found Option::unwrap panics).
* Fuzzers (all 0 panics now): tests/fuzz_chunk.py (mutated inflated partitions), fuzz_robust.py (mutated container), fuzz_tess.py, fuzz_kernels.py (hlr_raster/hlr_visible/feature_edges/smooth_part), fuzz_refvol.py. Not fuzzed: g103 record readers (compressed input fails at inflate first).
* tests/count_colgrid.py: proves a differential run actually triggers col_grid/subdivide_grid in the reference (a clean diff that never triggers proves nothing).
* README timing refresh 2026-10-18: cold full model Residential 1.1 s, Structural 3.6 s, Facades 1.5 s (remote-host).

## PENDING
* rvt_params.py (c5-geometry, 'Host|String' = 'None' when no host and no level): Python post-processing, no port. Run `tests/h1_suite.sh params` on logs/p3_files.txt (3 files; FILELIST=... LOGP=p5_) when remote-host has 11 GB; if 0 DIFFERENT: `cp -p ~/rvt-research/rvt_params.py logs/snap/` (tests/oracle_watch.sh then clean).
* Open target: 40 MB CACHED full model open 0.9 s vs 0.5 s (needs lazy record fields; risky for the demo path; the viewer reopens its own converted model, so not needed). Not started.

## Workflow lessons (2026-10)
* h1run syncs remote-host dev/ over remote-host dev/ at job start: a job that tests dev/ must run ./build.sh in the SAME job (sh -c './build.sh >/dev/null 2>&1; ...'), else it tests remote-host's stale .so (cost two false DIFFERENTs). Release: build+check job, rsync remote-host:src/rvt-core/target/release/librvt_core.so -> dev/rvt_core.abi3.so, cp pysrc/*.py dev/, ./release.sh --publish, tests/demo_smoke_all.sh.
* h1_suite.sh: FILELIST=file (custom file list, log prefix w_), LOGP=prefix (fresh logs), SRC=dev|python (build under test). h1_retry.sh loops are discouraged now (remote-host memory guard); call h1run once, if busy note it in ~/demo-state/H1-QUEUE.md and retry on the next pass.
* oracle_watch: tests/oracle_watch.sh lists reference modules changed since logs/snap; accept with cp -p after verifying. Python-only post-processing in rvt_params / rvt_materials needs no port (params runs the reference loop with Rust kernels).
