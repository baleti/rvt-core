# rvt-core progress

## 2026-10-12 stage 3 (session restarted 12:07 after the remote-host reboot)
Profile (cold, caches bypassed, bench/stages.py, bench/meshprof*.py):
- 40 MB 2024 Electrical: extract 9.7 s, params 3.8 s, channel-103 index 7.8 s, meshes ~29 ms each. 6 MB 2022 rst_basic: extract 4.6, params 1.6, index 1.2.
- Viewer geometry stage (its own timings): 180-240 s on 2024 Structural, all in sequential element_mesh calls; `_inst_parts` (symbol re-tessellation per instance) = 56 of 62 s.
- Inside extract: GC passes, per-candidate Python wrapper, numpy isin 2.7 s, ES schema scan 5 s (26 s on 2025 HVAC).

Done (all differential-tested vs the pure-Python oracle):
- dec.rs/conv.rs: GIL-free decoder to a value tree (exact messages incl. struct.error texts), converted to identical Python objects.
- extract.rs: tagged candidate scan, parallel decode, sequential first-wins merge = oracle RVT_WORKERS=1 loop. 40 MB file 9.7 s -> 1.8 s.
- g103.rs: members (parallel, verified against the reference GZ chain), groups, record chains. Structural 15.7 s -> 1.0 s.
- es.rs: extensible-storage schema scan (lazy strings). 78/78 identical, corpus 306 s -> 8.2 s.
- python/rvt_fast.py: same-signature adapters, on() switch (RVT_NOFAST=1), fast Doc + symbol-mesh memo (300 Structural meshes 39.9 s -> 13.9 s, identical),
  Progress file (~/.cache/rvt-progress/<key>.json, 250 ms), iter_meshes (forked workers <= 6).
- ~/rvt-research/RUST_INTEGRATION.md written (hooks for 176, notes for 187); extension deployed there (backup rvt_core.abi3.so.stage2-backup).

Next: element_mesh in Rust, params, binary cache, README timings.

## 2026-10-12 afternoon
- element_mesh ported to Rust (mesh.rs, elemmesh.rs): 2024 Structural 2,705 placed elements in 2.0 s (6 threads) vs ~125 s Python; diff_mesh: bit-identical except the
  `_cycles_from_polylines` set-order cases (geometrically equivalent per face). Earcut = the same mapbox_earcut, batched per Geometry node (one GIL acquisition).
- Session 176 applied the hooks (rvt_entities, g103_get, rvt_es, rvt_geometry) at ~13:15; their 13:08/13:13 mesh changes ported the same hour.
- params kernels (FlatSrc/resolve/ifc_stored) identical, 4.8 -> 2.3 s on 40 MB.
- binary cache (.rvtc extraction, .pk packed params/meshes/table), two-stage extraction (quick table + full model from one decode), Model / MeshStream / Progress.
- dev/ vs released python/ (release.sh with smoke diffs) because 176's processes import python/ live.
- Corpus suites running (tests/run_all.sh, then tests/run_more.sh): extract want_all identical on every file finished so far.

## 2026-10-12 evening (c4-rust, new account)
- remote-host suites (released builds of 13:50-19:00): params 70/70 identical (8 rme files killed at the 6 GB per-file cap), extract want_all 76/76, default 26/26, slim 25/25 identical, mesh 24/26 OK (Facades: set-order cases).
  h1_suite.sh: one scoped process per file (resumable), logs in remote-host:~/h1logs/rvt-core (logs/ inside src was overwritten by other agents' h1run sync).
- Ported reference changes: per-triangle `mat` (viewer request 14; keys resolved by the reference _material_of), nearly_closed per-node rule (was missing), loopless faces judged by own tessellation,
  nid 1e9 for bbox planes, params (stored_params, slot guard in the Rust MAPPING loop, Slope, analytical/column Length, curtain panel cells, MaterialElem, enum rules),
  23:10 curved-face refinement (rect_grid / refine_curved) and dup-body style rule. Reference _cycles_from_polylines now sorted (my request) -> meshes bit-identical (300/300 on Facades, rst_basic, Residential).
- Safety: params drift guard (a different rvt_params.py runs the reference loop with Rust kernels), adapter source in the cache key, python sources in pysrc/ (python/ only via release.sh), K::k debug assert.
- refined_volume in Rust (viewer item 13 root cause = topology_check): Facades 141 curved meshes 34 s -> 0.6 s, equal to 1e-9.

## 2026-10-13 (after the remote-host reboot)
- Mirrored reference changes 00:10 / 04:40 / morning edits (whole-body rule via Rust volume_by_node pieces, refine_curved edge flips + boundary tolerance, nearly_closed 0.12,
  refined_volume normal votes): bit-identical on Residential / rst_basic / Facades (300 each), Site 400 + Block 69; refined_volume 141/141 to 1e-9.
- Fork-safe pool (viewer item 12 fixed; viewer now uses rvt_fast.Doc by default). GC freeze before re-enable: 6 MB first open 1.75 s, warm 0.51 s.
- remote-host: params 69+ / mesh 25 / refvol 25 identical, rme reruns queued for an idle slice (per-file cap 4G after c4-geometry's OOMs).
- Found: the reference is not bit-reproducible across CPUs (remote-host vs remote-host numpy SIMD); Rust identical on both.
- Viewer conversion 2024 Structural 19.6 s (geometry 9.8 s, 57% the viewer's body_summary). Proposed get_x hook (pickle 1.6 s -> 0.6 s), rvt_rebar speedups.

## 2026-10-15 (c2-rust, takes over from c4-rust)
- remote-host: the retry queue (tests/h1_big.sh) never started because MemoryCurrent of the slice counts page cache (3 GB idle): now checks anon memory. Main corpus status was already 77/78 extract + params, 26/26 mesh + refvol; FAILED lines are skipped by the resumable logs, so h1_big2.sh removes/retries them; h1_big3.sh adds mesh_all / refvol_all over the whole corpus.
- Viewer conversion profile (bench/conv_profile.py): the extraction was loaded 3x per conversion -> rvt_fast.extract_shared + reference hook get_x_ro (applied by c2-geometry): 1.65 s -> 0.64 s on 2024 Structural.
  body_materials redundant with per-triangle mat (bench/mpcheck.py: 0 triangles changed on 5 files, 12,250 elements): c2-viewer dropped it (23cf722, Residential geometry 3.3 -> 1.7 s).
- Materials tables: 10 ms after the context load: no Rust port needed (item 4 closed).
- params mirror of 2026-10-15 (get_x_ro): identical on Residential / Structural, released, demo smoke OK.
- Facades geometry stage: ensure_roofs 2.5 s is mostly first imports (OCP 2.0 s, scipy 0.5, matplotlib 0.3); prewarming them in a thread during 'reading': geometry -1.5 s, reading +0.4 s (noisy host).
- rvt_viewer_fast.prestart(model) (dev only, not released): mesh stream started right after quick(), buffered for later em_geometries calls; results identical (Residential, Facades, Structural, rst_basic), but under load 12 it slows the reader stage (GIL contention): Facades 10.0/8.8 -> 8.0/9.8 s, Structural 11.8/16.6 -> 17.4/19.0 s. Not proposed.

## 2026-10-17 (c3-rust, claude3 takeover)
* Differential logs pulled (~/h1logs/rvt-core): all suites 0 DIFFERENT (main 78/78, extras 24/24). logs/h1_*.log in the repo are stale copies.
* oracle_watch: no reference module differs from logs/snap.
* smooth_part: remote-host build, diff_smooth IDENTICAL (Residential 34 meshes, Plumbing 69, synthetic), release --check OK, published, demo smoke OK x3. Hook + progress-file tip in ~/rvt-research/NEEDS-FROM-RUST.md.
* rvt_materials decode measured at 16 ms (Structural 2024): no Rust port needed. material_for_face is a per-face debug helper, not hot.
* Facades 2026 viewer conversion 12.4 s cold; remaining cost is viewer side (PIL swatches 6 s, scipy import 4.6 s): reported to c5-viewer.
* rvtsheets already memoizes params() per reader: no change to rvt_fast.params.
* Open: 40 MB cached full model 0.9 s (target 0.5 s) needs lazy record fields; not on the demo path, not started.
* sheets_profile 2024 Structural: Rust kernels 8.4 of 42 s; hints for c5-sheets (scene.cond_ok, index_handles memo) in NEEDS-FROM-RUST.md
* 2026-10-17 BUG FIXED + RELEASED: joined_vec used flate2 read::DeflateDecoder, whose into_inner() loses the BufReader read-ahead, so the "unused bytes > 8" member check never fired; a gzip member with trailing bytes (write-lab file e3_noframe) silently truncated the partition (105 records / 257 of 300 meshes lost). Now bufread::DeflateDecoder; the stored path returns None and the Python T.joined fallback is used. Verified: corpus extract_all 78/78 identical (r_), write-lab sample of 53 files extract 53/53 + mesh 53/53 identical (w3_), release --check, demo smoke x3. NEW SUITES: FILELIST=<file> [LOGP=prefix] [SRC=dev] tests/h1_suite.sh ... (logs/w_files.txt = every 3rd write-lab output). NOTE: h1run syncs remote-host dev/ over remote-host dev/: rsync the remote-host build back to remote-host dev/ before a SRC=dev suite job.
* 2026-10-17: all 161 write-lab outputs (every writer file on remote-host) identical released vs oracle in g103, params, extract_all, mesh (wa_ logs; FILELIST=logs/wall_files.txt LOGP=wa_).
* 2026-10-17 tests/fuzz_chunk.py (mutated inflated partitions: 64 cases on ds0.rte + e3_len_2020.rvt, 0 panics/crashes/exceptions, ~0.2-1.3 s each) and tests/fuzz_robust.py (mutated container: always a Python exception or ok).
* 2026-10-17 (end of session) col_grid / subdivide_grid / rect_grid cap (reference rvt_mesh 2026-10-17, spline sweeps / twisted lofts) PORTED in src/mesh.rs, committed, NOT RELEASED. diff_mesh n=1500 + diff_tess n=400 on 2024/2026 Facades and Site are IDENTICAL, BUT a counter on the reference shows col_grid/subdivide_grid never fire on those files (0 calls): the new code is UNTESTED. TODO: find elements that trigger it (NOTES.md 1050: viewer item 23; rosetta single-feature files with spline-swept tubes / twisted ruled lofts / rect->circle blends, ~/rvt-research gtg_r*), run diff_mesh/diff_tess on them (counter script: wrap RM._col_grid/_subdivide_grid), then build -> release.sh --check -> release. Released build (python/) is the earlier one (inflate fix); oracle_watch still lists rvt_mesh.py as CHANGED until this is verified.
* rvt_params.py 2026-10-17 change: verified identical (p3_ suite), snapshot accepted.
* 2026-10-17 RELEASED col_grid/subdivide_grid/rect_grid cap port: diff_tess bit-identical with 552 col_grid/subdivide triggers on rosetta blend/sweep files (tests/count_colgrid.py proves the trigger), diff_mesh 5 rosetta files 101-102/102 bit-identical, Facades/Site 2024+2026 identical, release --check, demo smoke x3. LESSON: a remote-host job that does not run ./build.sh itself tests remote-host's (stale) dev/ .so.
* 2026-10-17 RELEASED rvt_elemmesh topography material wrapper (SiteSurface/Toposolid all -1 mats -> stored 'Material' param -1002107; rvt_fast._topo_mat, full record via extract_shared; doc.rec is the graphics record only): Residential 471 elements 0 DIFFERENT (6 before), Site 2024 1200 identical. rvt_materials.fill_patterns is pure Python (no port).
* 2026-10-17 m2_ mesh_all with the released build (col_grid + topo material): 78 ok, 0 failed, 0 DIFFERENT. oracle_watch clean.
* rvt_params door-size tweak (c5-geometry): p4_ params 3 files identical, snapshot accepted.
* 2026-10-18 timing refresh (README): cold full model Residential 1.1 s, Structural 3.6 s, Facades 1.5 s on remote-host; all targets still met.
* rvt_materials.render_colors (pure Python over material_table): no port, snapshot accepted.
* 2026-10-18 RELEASED: NaN-safe float sorts (fcmp) in the Rust tessellator. tests/fuzz_tess.py (mutated Geometry nodes) found 7 Option::unwrap panics (mesh.rs sorts) before: 0 panics in 14k mutated calls after; diff_tess identical (Residential 1165 nodes, twist 873), release --check, demo smoke.
* 2026-10-18 tests/fuzz_kernels.py (hlr_raster, hlr_visible, feature_edges, smooth_part with NaN/inf/huge/out-of-range input): 3 seeds x 400 cases, 0 panics. Not yet fuzzed: mesh_doc / g103 / refined_volume entry points.
* 2026-10-18 tests/fuzz_refvol.py: refined_volume on mutated element meshes (1025 calls), 0 panics. Left unfuzzed: g103 record entry points.
* 2026-10-18 (c6-rust) rvt_params (rebar number, Host|String None, Toposolid): diff_params rst_basic + rme_basic 2020 IDENTICAL on remote-host; snapshot accepted. rvt_geometry wall bbox: no Rust path. No release needed.
* 2026-10-18 19:40 (c6-rust) rvt_params Sheet Number/Name '---' for unplaced views: rst_basic IDENTICAL, snapshot accepted (pure Python).
* 2026-10-18 (c6-rust) params check completed on remote-host: rst_basic, rme_basic 2020, Snowdon Plumbing 2024 all IDENTICAL; snapshot accepted. p6chk removed from H1 queue.
* 2026-10-18 (c6-rust) tests/fuzz_g103.py (mutated stored partition -> g103_load/g103_record): 360 cases rst_basic, 0 panics.
* 2026-10-18 (c6-rust) viewer already consumes the progress file (rvtviewer/build.py reader_progress via rvt_fast.file_key): brief item 'viewer consuming the progress API' met.
* 2026-10-18 (c6-rust) rvt_params beam y/z Offset Value: rst_basic IDENTICAL, snapshot accepted (pure Python).
* 2026-10-18 (c6-rust) rvt_params 'Elevation from Level' 0 for level-less FamilyInstance: rst_basic IDENTICAL, snapshot accepted (pure Python).
* 2026-10-18 (c6-rust) tests/params_watch.sh: automates verify+accept of Python-only rvt_params edits (rst_basic IDENTICAL -> accept); accepted 'Elevation' / offset / sheet edits this way.
* 2026-10-18 (c6-rust) further rvt_params edits accepted via tests/params_watch.sh (rst_basic IDENTICAL each time).
* 2026-10-18 (c6-rust) rvt_geom curve Slope sign (c5-geometry): rst_basic params IDENTICAL, snapshot accepted (Python-only, params cache hash picks it up).
* 2026-10-18 (c6-rust) rvt_params + rvt_geom edits accepted again (rst_basic IDENTICAL).
* 2026-10-19 (c6-rust) several more Python-only rvt_params/rvt_geom edits (c5-geometry) accepted via tests/params_watch.sh, rst_basic IDENTICAL each time. fullrep still queued (remote-host below 11 GB).
* 2026-10-19 (c7-rust takeover, claude7) remote-host h1_summary.log read: no DIFFERENT line in any current suite (wa_/m2_/p3-p5_ all 0; the earlier w_/w2_ 1 DIFFERENT was the e3_noframe inflate bug, fixed, w3_ 0). params_watch: rst_basic IDENTICAL, snapshot accepted (rvt_params Python-only). No release needed. fullrep (f7_) still gated: remote-host MemAvailable 3.9 GB < 11 GB. Open: 40 MB cached full model 0.9 s (lazy records, risky, not demo path).
* 2026-10-19 (c7-rust) fuzz_tess seed 77 rst_basic 2020: 150 nodes x30, 0 panics.
* 2026-10-20 (c7-rust) f7_ full-corpus on remote-host: g103 78/78, params 78/78, extract_all 78/78 all 0 DIFFERENT; mesh_all 51/78 so far 0 DIFFERENT (the "geometrically equivalent" counts, e.g. rst_basic 9/300, equal the earlier m2_ run: remote-host numpy vs oracle bit noise, not a regression). The c7f7 job stopped at 51/78 (remote-host reboot or reap); resume = LOGP=f7_ FILE_MEM=4G tests/h1_suite.sh mesh_all (resumable) once MemAvailable >= 11 GB. Then refvol_all/stream and README refresh.
* 2026-10-20 (c7-rust) LESSON: the reference rvt_mesh hole-ring refine + _split_collinear change was REVERTED by c5-geometry within hours (my port gave 2 DIFFERENT on rst_basic/Residential against the reverted reference; bisect with RVT_NOSPLITCOL showed the port itself was faithful). Port reverted (git revert), released build unchanged: diff_mesh n=600 rst_basic 0 DIFFERENT (14 equivalent = remote-host numpy noise), Facades 2026 0 DIFFERENT. Current reference changes (tjunction_closed near= default 4e-3 = Rust constant, brep_topology_closed pure Python, params) need no port; snapshot accepted. RULE: before porting a reference mesh change, wait ~1 h / check mtime stability and PYTHON_REFERENCE_CHANGES.md has an entry. Tools: tests/probe_tess.py, h1_port_check.sh.
* 2026-10-20 f7_ full corpus complete: g103/params/extract_all/mesh_all 78/78 each, 0 DIFFERENT (released build). codex c7-001 tests/test_progress.py merged (12 tests, mutation-checked).
* 2026-10-21 (c8-rust takeover, claude8) ported rvt_mesh refined_volume level cap 5->6 + RVT_VOLSTEP (src/refvol.rs). diff_refvol IDENTICAL on rst_basic 2020/2022, Residential, Facades 2024; release --check OK, demo_smoke_all OK x3, released. rvt_params (Python-only) accepted via params_watch. oracle_watch clean.
* 2026-10-21 (c8-rust) g8_ refvol_all (new level cap) queued on remote-host via tests/h1_retry.sh (gate 11 GB); results: remote-host:~/h1logs/rvt-core/h1_g8_refvol.log, then README refresh.
* 2026-10-21 (c8-rust) RELEASED stacked closed segments port (rvt_mesh.coincident_plane_pairs + rvt_elemmesh single-node select_closed_shell veto; src/mesh.rs, src/elemmesh.rs). tests/diff_mesh.py gained IDS=eid,.. . HVAC 2024 flex ducts 1404115/1474008: old build DIFFERENT, new identical; n=400 HVAC + rst_basic 2022 400/400 bit-identical; release --check, demo smoke x3 OK. volume_by_node is Python (no port). If c8-geometry reverts it within hours, git revert.
* 2026-10-21 (c8-rust) stacked-segments port wider check: HVAC 2024 n=1600 bit-identical (1600/1600), RVT_NOSTACKED=1 parity on the two flex ducts identical. g8_ refvol_all + mesh_all on remote-host still queued (tests/h1_retry.sh, gate 11 GB).
* 2026-10-21 (c8-rust) interim local refvol check of the cap-6 release (remote-host, 200 meshes/file): 7 more files across 2016-2024 (Arch_Link, rac_advanced, Technical_school, HVAC 2024, rme_basic, rst_basic 2019) all IDENTICAL(1e-9). Full g8_ corpus run still gated on remote-host.
* 2026-10-21 (c8-rust) stacked-port check on 2026 Snowdon HVAC + Plumbing (remote-host, n=800 each): 1600/1600 bit-identical.
