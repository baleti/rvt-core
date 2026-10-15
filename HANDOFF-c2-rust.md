# HANDOFF c2-rust (2026-10-15 11:25, session-limit stop; resumes automatically)

## State
* All work committed; released build in python/ + ~/rvt-research/rvt_core.abi3.so (md5 4e4d7b52, identical on remote-host). Demo smoke (tests/demo_smoke_all.sh) passed after the last release.
* Mirrors the reference up to 2026-10-15 06:18 (refined_volume ruled fan, params sha1 2c5940f5). Details: HANDOFF.md, PROGRESS.md.
* remote-host queue running from remote-host: `tests/h1_big5.sh` (nohup, log logs/h1_big5.out): c1 stream (crash sweep, main corpus), c2 stream (extra samples),
  then c3 = 2018 05_rme_advanced extract/params/slim/mesh_all/refvol_all at 6G. Results: remote-host:~/h1logs/rvt-core/h1_summary.log + h1_*.log.

## Open findings
1. Nanjing (2011 sample) refvol element 546245: remote-host suite said Rust 0.49236 vs reference 0.52205, but on remote-host the SAME released .so gives 0.52205 = reference.
   Same .so md5 on both hosts. Next: rerun scratchpad one_rv.py equivalent on remote-host (bench script: element_mesh(546245) -> M.refined_volume vs RC.refined_volume);
   suspects: remote-host's rvt_mesh.py version (synced 05:43, has _fan_uv) vs the element_mesh input differing on remote-host (different numpy => different meta/uv), or remote-host's .so loaded from a stale RVT_FAST_DIR snapshot.
2. rvt_params.py changed again (11:40 c2-geometry: mep_curve ends, _view_sheet_params, parameter-id keys, ...). DECISION PROPOSED: retire the hand mirror of the params loop.
   Measured: reference loop + Rust kernels 0.74 s (rst_basic) / 1.17 s (40 MB Electrical) vs mirror ~0.6-1.3 s: no real gain, and c2-geometry edits the loop several times a day.
   The drift guard already does this automatically (correct by construction); to make it explicit: in pysrc/rvt_fast.params use _params_impl only when RVT_CORE_PARAMS_MIRROR=1, update README/RUST_INTEGRATION.md, release, smoke.
3. README: fill the differential table: main corpus mesh 77/78 + refvol 77/78 identical (whole corpus now; 05 rme_advanced pending c3), extra extract/params 24/24, x_mesh 24/24, x_refvol 23/24 (Nanjing, finding 1); stream sweep pending.

## Restart commands
* queue status: `cat logs/h1_big5.out; ssh remote-host 'tail ~/h1logs/rvt-core/h1_summary.log'`; restart: `nohup tests/h1_big5.sh > logs/h1_big5.out 2>&1 < /dev/null &`
* port check: `tests/oracle_watch.sh` (+ PYTHON_REFERENCE_CHANGES.md), then ./build.sh, tests (diff_params / diff_refvol / bench/mcmp.py), ./release.sh, tests/demo_smoke_all.sh, `oracle_watch.sh --accept`.
