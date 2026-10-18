# HANDOFF c6-rust (claude6, 2026-10-18). Read HANDOFF-c3-rust.md for workflow lessons (still valid).
State: master clean, released build = python/ (inflate fix, col_grid, topo material, fcmp). oracle_watch clean (last accepted rvt_params 'Sheet Number ---' edit). All suites identical (README).
Params changes by c5-geometry are Python-only: verify with tests/diff_params.py FILE on remote-host (3G scope) for rst_basic/rme_basic/Plumbing 2024, then tests/oracle_watch.sh --accept.
Open: (1) 40 MB cached full model 0.9 s vs 0.5 s target (needs lazy record fields; risky, not on demo path); (2) full-corpus report refresh needs a remote-host job (11 GB gate); (3) viewer does not consume rvt_fast.Progress (ask c6-viewer if wanted).
Next ports only when PYTHON_REFERENCE_CHANGES.md logs rvt_mesh/rvt_elemmesh changes.
