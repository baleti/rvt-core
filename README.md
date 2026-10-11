# rvt-core
Rust hot core (PyO3/maturin) for the Revit file reader in ~/rvt-research. Own code only (no ODA, nothing copied from rvt-rs).
Stage 1 (done): `strip_pages`, `joined` (parallel gzip-member inflate using the segment-header sizes), `header_candidates`.
Stage 2 (todo): schema parser + schema-driven record decoder (port of rvt_decode.py), record chain walk.
The Python implementation stays the reference oracle: `tests/diff_core.py` must report 0 differences on all corpus files before any switch-over (stage 1: 183 partitions, 0 differences, 3 fall back to Python).
Build: `. .venv/bin/activate && maturin build --release && pip install --force-reinstall target/wheels/*.whl`
