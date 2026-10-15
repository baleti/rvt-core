# HANDOFF c4b-rust (claude4 ending; successor c5-rust on claude5). Supersedes HANDOFF-c4-rust.md, same content. No user bugs A-D are assigned to rust.

## State (all committed, tested)
* HEAD: rvt-core master. Released build in python/ + ~/rvt-research/rvt_core.abi3.so; demo smoke OK on all 3 demo files after the last release (symbol-check fallback, params reference loop + Rust kernels; hand mirror opt-in RVT_CORE_PARAMS_MIRROR=1). Details: HANDOFF.md, README.md, RUST_INTEGRATION.md (c4-rust 13:30 note).
* oracle_watch snapshot accepted at 12:xx (rvt_params 11:40, rvt_geom mep_curve ends; no Rust port needed).

## remote-host jobs
* `tests/h1_big5.sh` (nohup on remote-host, log logs/h1_big5.out): stream main DONE (77 ok, 1 failed, 0 DIFFERENT); x_stream running/next; then c3 = 2018 05_rme_advanced at 6G (extract/params/slim/mesh_all/refvol_all). Results: ssh remote-host 'tail ~/h1logs/rvt-core/h1_summary.log'. If remote-host reboots: `nohup tests/h1_big5.sh > logs/h1_big5.out 2>&1 < /dev/null &` (resumable).

## Remaining
1. Read the c3 results; any DIFFERENT is a bug. Update README differential table (05_rme_advanced "pending").
2. Nanjing 546245 refvol (remote-host 0.4924 vs ref 0.5220, remote-host equal): run `bench/nanjing_probe.py FILE 546245` via h1run (env rvt) when remote-host MemAvailable >= 11 GB (OOMs on remote-host even at 4G).
3. Speed: 40 MB cached full model 0.9 s -> 0.5 s (compact meshes/mmap cache), viewer consumption of the progress file (ask c4-viewer).
4. Keep porting kernel-relevant reference edits (oracle_watch.sh; rvt_map2 resolve/Src, rvt_ifc.stored, rvt_mesh/rvt_elemmesh).
* x_stream done 12:51: 24 ok, 0 DIFFERENT. Queue (h1_big5.sh) may still be running c3 (rme_advanced 6G): check h1run status / logs/h1_big5.out.
