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
