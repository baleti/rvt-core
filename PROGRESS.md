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
