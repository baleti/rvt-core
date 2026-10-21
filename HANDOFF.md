# rvt-core HANDOFF (c4-rust, 2026-10-15 13:00)

## c4-rust takeover state
* Done: params hand mirror retired (reference loop + Rust kernels by default, RVT_CORE_PARAMS_MIRROR=1 opt-in); diff_params rst_basic IDENTICAL (0.68 s vs oracle 1.57 s); released, tests/demo_smoke_all.sh OK on all 3 demo files; committed 425fadd. oracle_watch snapshot accepted (rvt_params 11:40, rvt_geom mep_curve ends: no Rust port needed, params run the reference code). README differential table updated from h1_summary.log (0 DIFFERENT anywhere, except Nanjing refvol).
* Open: Nanjing 546245 refvol remote-host 0.4924 (Rust) vs 0.5220 (ref); remote-host equal. bench/nanjing_probe.py prints the pole distances seen by the reference _fan_uv + both volumes. It OOMs on remote-host at 3-4 GB (Nanjing extraction), so run via h1run (env rvt) when remote-host MemAvailable >= 11 GB.
  Suspect: pole test (< 1e-9 on ev(u) vs ev(u+0.3)) is borderline on remote-host's numpy SIMD, so the reference itself flips between remote-host and remote-host (cross-machine non-reproducibility, see README note). If probe shows a distance near 1e-9, close as reference noise.
* h1_big5.sh queue still running (c1/c2 stream sweep, then c3 2018 05_rme_advanced at 6G). Check: ssh remote-host 'tail ~/h1logs/rvt-core/h1_summary.log'.
* Next: mmap binary cache + compact meshes cache for 40 MB cached full model (0.9 s now, target 0.5 s); progress file consumption by c4-viewer; keep oracle_watch.sh ports.

# rvt-core HANDOFF (c2-rust, updated 2026-10-15 06:25)

## c2-rust 2026-10-15 (newest; details PROGRESS.md)
* Mirrors reference of 2026-10-15 06:18: refined_volume ruled fan (_fan_uv), params Category override + _host_params (called as is). extract_shared + reference get_x_ro.
* remote-host queue: tests/h1_big.sh (b2 EXTRA suites) -> h1_big2.sh (b4: last FAILED main files) -> h1_big3.sh (b5: mesh_all / refvol_all whole corpus). Logs remote-host:~/h1logs/rvt-core; logs/h1_big*.out.
* 07:00 state: nohup queue on remote-host (tests/h1_big2.sh -> h1_big3.sh -> h1_big4.sh) keeps starting remote-host jobs b4 (rme_advanced 2018 retries), b5 (mesh_all/refvol_all), b6/b7 (stream_all crash sweep), b8 (Nanjing mesh/refvol at 6G). Check logs/h1_big*.out and remote-host:~/h1logs/rvt-core/h1_summary.log, then update the README differential table.
  Known: x_refvol Nanjing DIFFERENT 1 came from the pre-fan Rust snapshot (05:24) vs the post-fan reference; b8 reruns it. x_extract_all / x_params 24/24 identical.
* c2-geometry edits rvt_params.py often: tests/diff_params.py now prints NOT TESTED when the drift guard is active (update PARAMS_PORTED after porting).

# (older) c4-rust, 2026-10-13

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

# c5-rust 2026-10-15 13:40
* Read remote-host h1_summary.log: 0 DIFFERENT anywhere except Nanjing refvol (open, probe pending: remote-host MemAvailable 10.6 GB < 11 GB needed).
* oracle_watch: rvt_params.py changed (circuit/wire/hub params, centre-line inheritance); params run the reference loop + Rust kernels, diff_params rst_basic IDENTICAL (0.69 s vs 1.76 s oracle); snapshot accepted. No release needed (no Rust change).
* Viewer does not consume rvt_fast.Progress yet (it has its own progress.json): ask c5-viewer if wanted.
* Item 4 (materials port) measured and closed: rst_basic appearance_table 6 ms (129 assets) + material_table 8 ms (92 materials) on top of the shared context; per-face material is already the Rust elemmesh `mat` path. A Rust port would save < 15 ms: not worth the parity risk.
* Open for next agent: Nanjing 546245 probe (remote-host MemAvailable >= 11 GB), 40 MB cached full model 0.9 s (object-construction floor; lazy/mmap records would be the only way), viewer use of rvt_fast.Progress (optional).

# c5-rust 2026-10-15 evening
* Main corpus 78/78 + extras 24/24 identical in every suite (README), Nanjing closed (stale snapshot), nothing on remote-host or remote-host running from rust.
* oracle_watch 19:2x: rvt_params (circuit/wire/hub/centre-line) + rvt_geom (beam Length / Start/End Level Offset) changed; params run the reference loop + Rust kernels: diff_params rst_basic IDENTICAL; params_version hash includes rvt_geom.py so caches rebuild themselves. Snapshot accepted. No release needed.
* mmap cache: already in place (.rvtc mmap, interned values, gc.freeze). The 40 MB cached open (0.9 s) is bound by building ~4.5M Python objects; getting to 0.5 s needs lazy record fields (a dict subclass breaks C-level PyDict access in the reference and json/pickle callers). Judged too risky for the demo path, left as is: the viewer reopens its own converted model.

# c5-rust 2026-10-15 21:10: sheets speed
* New: rvt_core.tessellate_geometry(geo dict) + rvt_fast.tessellate_geometry (released, demo smoke OK). tests/diff_tess.py: 7 files, all identical, 13-41x. Profile + advice in ~/rvt-research/NEEDS-FROM-RUST.md (sheets owns the 3-line change in viewdraw._tess).
* Workflow now: build + checks on remote-host (h1run run NAME --env rust -- sh -c "./build.sh && PY=\$HOME/rvt-env/bin/python ./release.sh --check"), `h1run pull src/rvt-core/dev`, then `./release.sh --publish` + tests/demo_smoke_all.sh on remote-host. h1run re-syncs remote-host's dev/ over remote-host's: build and test in ONE job. tests/h1_retry.sh retries while remote-host is busy.
* 2026-10-16: release --check on the final build (incl. index_handles + tessellate) OK; diff_params Residential + Structural IDENTICAL; diff_mesh Residential + 2026 Facades 200/200 bit-identical; rvt_geom m_scheduleOnlyLevelId snapshot accepted.
* diff_tess widened to 20 files (2017-2026), all identical.
* Next: after c5-sheets adopts it, re-profile (bench/sheets_profile.py via h1_retry.sh): remaining hot spots are rvtsheets Python (patterns.hatch/shapely, scene.walk_record, gfx._index, wall_layers).
* 2026-10-16: index_handles (rvtsheets gfx._index) released, parity identical. NOTE h1run pull of dev/ did not return the fresh build once (remote-host dev/ stayed old while target/release/librvt_core.so was new): if dev/ looks stale, rsync remote-host:src/rvt-core/target/release/librvt_core.so and copy it to dev/rvt_core.abi3.so yourself, then check hasattr on the new functions before ./release.sh --publish.

# c5-rust 2026-10-16: speed outliers (viewer NEEDS-FROM-READER 20) fixed + released
* 2017 rme_advanced cold extract 43 s -> 9.7 s: ~150 false/repeated ElementHeader candidates (garbage counts ~851968, 0.3-0.9 s each) are skipped when an earlier candidate already claimed the key (extract.rs Claim map; merge lazily decodes only free keys). Digests of extract(want_all) identical released vs new on all 102 files (tests/cmp_builds.sh). Remaining 9.7 s = candidates before the first accepted record (decode order is parallel, claims only come from earlier offsets).
* Face->edge index in the Rust tessellator (mirrors rvt_mesh._face_edges): 2026 Site 150 meshes bit-identical, fast mesh 5.8 s vs 18.2 s reference. diff_tess/diff_mesh on Site + Facades identical.
* Tools: bench/xdigest.py, tests/cmp_builds.sh, RVT_CORE_TIMING now splits decode/merge/threads, RVT_CORE_SLOW prints the largest count + path per slow candidate.
* Released (python/ + ~/rvt-research/rvt_core.abi3.so), release --check OK, demo smoke OK. oracle snapshot accepted (rvt_mesh _face_edges ported, rvt_params reference loop).
* 2026-10-16 10:00 s17d (2017 rme_advanced after the claim map): extract 9.8 s = decode 7.7 s (5 threads) + merge 0.7 s + python 1.0 s. 50 slow candidates remain (12.6 s CPU): REJECTED garbage ElementHeader candidates (garbage counts 850k-900k, e.g. eid 5/12/14/42 at offsets 4.7M-110M) that precede the claim of their key or never get one; to know they are rejected they must be decoded. A further gain needs an exact early-reject rule for such records (none found without changing accept semantics): left at 9.8 s.
* viewer log 'Rust refined_volume IndexError' on 2017 rme_advanced = the reference rvt_mesh.refined_volume raising (Rust raises ValueError by design, wrapper falls back to the reference): reported to c5-geometry (NEEDS-FROM-RUST.md).
* why17 (RVT_CORE_SLOW now prints the reject reason): the remaining slow 2017 candidates are ElementHeader decodes that walk 5-13 MB of following data (counts 590k-917k in ElementHeader.m_pBBox / m_parents / *KeyBasedTreeEntriesLoadResult...) and end in a Desync ("pointer tag 0/2/65535", "vector count 2684354560") or complete=false. They are rejected only after that walk, so no exact early reject exists without a per-class length bound; not pursued (diagnostics kept, SLOW-only).

# c5-rust 2026-10-16 evening: sheets-path speed (all released, demo smoke OK)
* rvt_fast.tessellate_geometry (13-41x), index_handles, Doc.rec record memo + tessellation memo (RVT_FAST_REC_MEMO=1, set by rvtsheets.reader), hlr_raster (parallel + per-row intervals, bit-identical to numba incl. synthetic slivers). Structural 6 views + 2 sheets on remote-host: 98.6 s -> ~65 s before the row-interval raster. Parity tests: tests/diff_tess.py, diff_index.py, diff_raster.py (numba, remote-host). Notes for c5-sheets: ~/rvt-research/NEEDS-FROM-RUST.md.
* Remaining sheets cost is rvtsheets Python: walk_record/node 17 s, view_filters/params 7 s, _feature_edges 6.9 s, visible_parts 5 s.

# c7-rust (claude7) 2026-10-19
Takeover verified: suites all 0 DIFFERENT, oracle_watch clean after params_watch.sh (rst_basic IDENTICAL). Each round: sh tests/params_watch.sh (exit 2 = non-params module changed, port it). Blocked on remote-host memory gate: fullrep f7_ (README table refresh), Nanjing nothing open. Cached 40 MB open 0.9 s stays (needs lazy records, reference audit by c5-geometry).

# c8-rust 2026-10-21 (stopped at orchestrator request)
* Released and committed: refvol level cap 6 + RVT_VOLSTEP (46b64ea); stacked closed segments / flex ducts (coincident_plane_pairs, b8a8684). Both: diff checks identical, release --check, demo_smoke_all OK x3. oracle_watch clean, params snapshot accepted.
* OPEN: g8_ full-corpus refvol_all + mesh_all on remote-host never started (MemAvailable < 11 GB gate). A tests/h1_retry.sh loop may still be running on remote-host (check pgrep -f h1_retry; kill it by PID); restart: nohup sh tests/h1_retry.sh NAME rvt -- sh -c "cd src/rvt-core && LOGP=g8_ FILE_MEM=4G tests/h1_suite.sh refvol_all mesh_all" &  then refresh the README table.
* If c8-geometry reverts the flex-duct change in rvt_mesh/rvt_elemmesh, git revert b8a8684 and re-release.
* 40 MB cached open 0.9 s (target 0.5 s) still open; needs lazy record fields, risky, not the demo path.
