# HANDOFF c4-rust (written 2026-10-14 at the weekly budget stop; resume 2026-10-18)

## State
* Everything is committed (last: tests/h1_big.sh). Released build in python/ and ~/rvt-research/rvt_core.abi3.so mirrors the reference up to 2026-10-13 18:45. Demo smoke passes (tests/demo_smoke_all.sh). Details: HANDOFF.md, README.md (differential table), PROGRESS.md.
* Running services: none. The background tests/h1_big.sh queue was killed before it started any remote-host job (slice was busy with c4-geometry fid8/fid9).

## Remaining work
1. Retry the memory-failed files on remote-host, one job at a time, only when the shared slice (ssh remote-host 'systemctl --user show rvt-offload.slice -p MemoryCurrent --value') is below ~2.5 GB: `nohup tests/h1_big.sh &` (4G per file; jobs b1 main extract_all+params, b2 EXTRA=1 all suites, b3 main mesh+refvol). Logs are resumable in remote-host:~/h1logs/rvt-core (h1run pull h1logs/rvt-core). Files: rme_advanced x5, rme_basic 22/28 (params), 2011 Nanjing, 2020 HYPERLOOP. Then update the README table.
2. Check `tests/oracle_watch.sh` and ~/rvt-research/PYTHON_REFERENCE_CHANGES.md for new c4-geometry changes: port, bench/mcmp.py on Residential/rst_basic/Facades, ./build.sh, ./release.sh, tests/demo_smoke_all.sh, then `oracle_watch.sh --accept` and commit.
3. Check RUST_INTEGRATION.md / ~/src/rvt-viewer/NEEDS-FROM-READER.md for new c4-viewer requests.
4. Optional perf: 40 MB warm full model 1.7 s; first-open all meshes 12 s on 40 MB; Rust materials/rebar only if profiles ask.

## Open problems
* Reference is not bit-reproducible across CPUs; check bit identity on remote-host.
* remote-host slice is 9 GB shared; honour c4-geometry's request to leave ~3 GB.
