# rvt-core HANDOFF (c4-rust, updated 2026-10-13 after the remote-host reboot)

## State
* Released build (imported live by the reader hooks and rvt-viewer): `python/rvt_core.abi3.so` + `python/rvt_fast.py` + `python/rvt_viewer_fast.py`, .so also in `~/rvt-research/rvt_core.abi3.so`.
  Sources: `src/*.rs`, `pysrc/*.py`. `./build.sh` -> `dev/`, `./release.sh` (smoke diffs incl. params + cache round trip) -> `python/`. Then ALWAYS the demo smoke:
  `(cd ~/src/rvt-viewer && sh tools/demo_smoke.sh ~/rvt-research/corpus/2022/30_rst_basic_sample_project.rvt 8990)`.
* Mirrors the reference as of 2026-10-13 04:40 (rvt_mesh / rvt_elemmesh incl. refine_curved flips, whole-body rule, per-triangle mat) and rvt_params of 2026-10-12 21:30
  (drift guard: any other rvt_params.py runs the reference loop with Rust kernels). Check with `tests/oracle_watch.sh` (snapshot in logs/snap; `--accept` after porting).
* Emergency switch: `RVT_NOFAST=1`.

## Differential status
* remote-host suites (logs in remote-host:~/h1logs/rvt-core, NOT in the synced tree): extract want_all 76/76, default 26/26, slim 25/25, g103 78/78, params 69/69, mesh 25/25 (0 different),
  refvol 25/25; rme files killed by memory are being re-run (job h1-rvtcore-r, FILE_MEM=8G, diff_params now holds one extraction).
* remote-host's numpy gives other bits than remote-host's for the reference itself ("geometrically equivalent" lines on remote-host); bit-identity is checked on remote-host (bench/mcmp.py, tests/cmp_ids.py).

## Done this session (see PROGRESS.md)
per-triangle mat, params ports, curved refinement ports, refined_volume in Rust (viewer drop-in), fork-safe pool (viewer item 12), GC freeze (6 MB first open 1.75 s, warm 0.51 s).

## Next
1. When c4-geometry logs a change in ~/rvt-research/PYTHON_REFERENCE_CHANGES.md: port, `bench/mcmp.py` on Residential / rst_basic / Facades (+ Site ids), release, demo smoke.
2. 40 MB warm full model 1.7 s (object construction bound); first-open all meshes 12 s on 40 MB (mat resolution needs the material context = rvt_xcache pickle load).
3. Optional: materials/appearance tables in Rust (small), viewer progress-file consumption (ask c4-viewer).
