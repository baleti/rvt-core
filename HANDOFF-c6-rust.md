# HANDOFF c6-rust (claude6, 2026-10-18). Read HANDOFF-c3-rust.md for workflow lessons (still valid).
State: master clean, released build = python/ (inflate fix, col_grid, topo material, fcmp). oracle_watch clean (last accepted rvt_params 'Sheet Number ---' edit). All suites identical (README).
Params changes by c5-geometry are Python-only: verify with tests/diff_params.py FILE on remote-host (3G scope) for rst_basic/rme_basic/Plumbing 2024, then tests/oracle_watch.sh --accept.
Open: (1) 40 MB cached full model 0.9 s vs 0.5 s target (needs lazy record fields; risky, not on demo path); (2) full-corpus report refresh needs a remote-host job (11 GB gate); (3) viewer does not consume rvt_fast.Progress (ask c6-viewer if wanted).
Next ports only when PYTHON_REFERENCE_CHANGES.md logs rvt_mesh/rvt_elemmesh changes.

## Update 2026-10-19
* tests/params_watch.sh: run every round; it verifies (rst_basic diff_params) and accepts Python-only rvt_params/rvt_geom edits, stops (exit 2) if any other module changed (then port per PYTHON_REFERENCE_CHANGES.md).
* tests/fuzz_g103.py added (0 panics). All fuzzers 0 panics.
* remote-host gate blocked (known, orchestrator handling). Queued: fullrep = LOGP=f7_ tests/h1_suite.sh g103 params extract_all mesh_all (hours, 4 GB/file), then README table refresh from ~/h1logs/rvt-core/h1_f7_*.log.
* Design sketch for the 40 MB cached open (0.9 s -> 0.5 s): the floor is building ~4.5M Python objects. Only lazy per-element record materialisation avoids it: keep the mmap'd .rvtc and build `fields` on first access in a Mapping subclass of the record dict. Blocker: reference code and json/pickle callers use C-level PyDict access, so a Mapping subclass is not drop-in; would need a reference-side audit by c6-geometry. Not on the demo path (viewer reopens its own converted model).
