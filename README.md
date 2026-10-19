<p align="center"><img src="assets/logo.png" alt="rvt-core logo" width="140"></p>

<h1 align="center">rvt-core</h1>
<p align="center">Rust fast path for <a href="https://github.com/baleti/rvt-reader">rvt-reader</a>, exposed to Python through PyO3.</p>

<p align="center"><a href="LICENSE"><img alt="License: MIT" src="https://img.shields.io/badge/license-MIT-0f766e"></a> <img alt="Status: experimental" src="https://img.shields.io/badge/status-experimental-1e293b"></p>

## What it does

Ports the hot loops of the pure-Python reader to Rust: record decoding, partition extraction, mesh and surface tessellation, schema discovery, parameter resolution and a binary cache. The Python reader remains the reference implementation. Every fast path has the same signature and result as its Python counterpart and is validated by differential tests against it.

## Build

```bash
pip install maturin
maturin develop --release        # or ./build.sh
```

Requires a recent stable Rust toolchain and Python 3.9+ (abi3 wheel).

## Usage

The adapters in `python/rvt_fast.py` are picked up automatically by the reader when the extension is importable.

| Variable | Effect |
|---|---|
| `RVT_NOFAST=1` | Disable the Rust path and run the Python reference |
| `RVT_NOCACHE=1` | Disable the binary cache |
| `RVT_THREADS=N` | Worker threads (default: min(6, cpus)) |

## Layout

See [`README-orig.md`](README-orig.md) for the module-by-module mapping from Python functions to Rust sources, and `tests/` for the differential test suite.

## Measured speed-ups

Differential-tested against the pure-Python reference on 102 corpus files; extract digests are identical.

| Case | Before | After |
|---|---|---|
| 2017 `rme_advanced` cold extract | 43 s | 9.7 s |
| Sheet render, 2020 Residential (6 views/sheets) | 101 s | 27 s |
| Viewer refined-volume check (Facades) | 34 s | 0.6 s |
| 2026 site mesh stage (150 meshes, bit-identical) | 18.2 s | 5.8 s |
| Parameter loop, `rst_basic` | 1.6 s | 0.7 s |

## Pending

- The Rust path currently covers decoding, extraction, parameters, meshing and caching; new reader features land in Python first and are ported once stable.
- Fuzz and robustness tests exist (`tests/fuzz_*.py`) but are not wired into CI.

## The rvt toolkit

| Repo | Role |
|---|---|
| [rvt-reader](https://github.com/baleti/rvt-reader) | Pure-Python reader: file container, schema, elements, parameters, geometry |
| [rvt-core](https://github.com/baleti/rvt-core) | Optional Rust acceleration for the reader (PyO3) |
| [rvt-viewer](https://github.com/baleti/rvt-viewer) | Browser 3D viewer built on the reader |
| [rvt-sheets](https://github.com/baleti/rvt-sheets) | Sheet and view renderer (2D drawings) built on the reader |
| [rvt-qa](https://github.com/baleti/rvt-qa) | Visual-diff QA tooling for rvt-sheets output |
| [rvt-writer](https://github.com/baleti/rvt-writer) | Experimental generator of `.rvt` / `.rte` scenes |

## Status and scope

Research-grade software developed by reverse-engineering the file format through black-box analysis of sample files and comparison with the output of the official Revit API. It is accurate on the files it has been tested against but makes no guarantees for arbitrary files. Test models (Autodesk sample files) and Revit-derived ground-truth data are **not** included in this repository.

## License

MIT, see [LICENSE](LICENSE). Autodesk and Revit are trademarks of Autodesk, Inc. This project is independent and is not affiliated with, endorsed by or sponsored by Autodesk.
