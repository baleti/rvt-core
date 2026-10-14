# rvt-core HANDOFF (c4-rust, updated 2026-10-13 after the remote-host reboot)

## State
* Released build (imported live by the reader hooks and rvt-viewer): `python/rvt_core.abi3.so` + `python/rvt_fast.py` + `python/rvt_viewer_fast.py`, .so also in `~/rvt-research/rvt_core.abi3.so`.
  Sources: `src/*.rs`, `pysrc/*.py`. `./build.sh` -> `dev/`, `./release.sh` (smoke diffs incl. params + cache round trip) -> `python/`. Then ALWAYS `tests/demo_smoke_all.sh` (the three DEMO.md files); single-file form:
  `(cd ~/src/rvt-viewer && sh tools/demo_smoke.sh ~/rvt-research/corpus/2022/30_rst_basic_sample_project.rvt 8990)`.
* Mirrors the reference as of 2026-10-13 18:45 (rvt_mesh / rvt_elemmesh; the log in PYTHON_REFERENCE_CHANGES.md is the authority) (rvt_mesh / rvt_elemmesh incl. refine_curved flips, whole-body rule, per-triangle mat) and rvt_params of 2026-10-12 21:30
  (drift guard: any other rvt_params.py runs the reference loop with Rust kernels). Check with `tests/oracle_watch.sh` (snapshot in logs/snap; `--accept` after porting).
* Emergency switch: `RVT_NOFAST=1`.

## Differential status
* README.md has the table: zero differences everywhere it ran. Not run (memory): rme_advanced x5 + two extra samples (2011 Nanjing, 2020 HYPERLOOP) for extraction / params; retry with
  `FILE_MEM=4G tests/h1_suite.sh ...` only when the shared remote-host slice is idle (c4-geometry's fid jobs need ~3 GB headroom; `tests/h1_polite.sh` = sequential, 2G, only while the slice < 4.5 GB).
* Locally (remote-host, capped scopes) 2020 rme_basic / rme_advanced now fit (extraction batching), so mesh checks run here too.
* remote-host's numpy gives other bits than remote-host's for the reference itself; bit-identity is checked on remote-host (bench/mcmp.py, tests/cmp_ids.py).

## Done this session (see PROGRESS.md)
per-triangle mat, params ports, curved refinement ports, refined_volume in Rust (viewer drop-in), fork-safe pool (viewer item 12), GC freeze (6 MB first open 1.75 s, warm 0.51 s).

## Next
0. (2026-10-14) Paused at the weekly budget stop; see HANDOFF-c4-rust.md. The h1_big.sh queue was killed before starting any job; restart it with `nohup tests/h1_big.sh &` (retries the memory-failed files at 4G one job at a time, only when the remote-host slice < 2.5 GB. Results in remote-host:~/h1logs/rvt-core (h1run pull h1logs/rvt-core), then update README table.
1. When c4-geometry logs a change in ~/rvt-research/PYTHON_REFERENCE_CHANGES.md: port, `bench/mcmp.py` on Residential / rst_basic / Facades (+ Site ids), release, demo smoke.
2. 40 MB warm full model 1.7 s (object construction bound); first-open all meshes 12 s on 40 MB (mat resolution needs the material context = rvt_xcache pickle load).
3. Optional: materials/appearance tables in Rust (small), viewer progress-file consumption (ask c4-viewer).
