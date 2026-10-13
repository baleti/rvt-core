#!/bin/sh
# smoke differential tests on the dev build, then publish dev/ -> python/ (atomic renames). Full corpus tests: tests/run_all.sh, tests/run_more.sh
set -e
cd "$(dirname "$0")"
PY=${PY:-$HOME/src/rvt-viewer/.venv/bin/python}
C=$HOME/rvt-research/corpus
export RVT_FAST_DIR=$PWD/dev RVT_CORE_NOCACHE=1
$PY tests/diff_decoder.py $C/2022/30_rst_basic_sample_project.rvt --n 300 > /dev/null
$PY tests/diff_extract.py --want-all $C/2022/30_rst_basic_sample_project.rvt
$PY tests/diff_g103.py --n 100 $C/2022/30_rst_basic_sample_project.rvt
$PY tests/diff_params.py $C/2022/30_rst_basic_sample_project.rvt
$PY tests/diff_cache.py $C/2022/30_rst_basic_sample_project.rvt
$PY tests/diff_bodysum.py $C/2022/30_rst_basic_sample_project.rvt 200
$PY bench/mcmp.py $C/2022/30_rst_basic_sample_project.rvt 300 | tee /dev/stderr | grep -q "different 0"
cp dev/rvt_core.abi3.so python/rvt_core.abi3.so.tmp && mv python/rvt_core.abi3.so.tmp python/rvt_core.abi3.so
cp dev/rvt_fast.py python/rvt_fast.py.tmp && mv python/rvt_fast.py.tmp python/rvt_fast.py
cp dev/rvt_viewer_fast.py python/rvt_viewer_fast.py.tmp && mv python/rvt_viewer_fast.py.tmp python/rvt_viewer_fast.py
cp python/rvt_core.abi3.so $HOME/rvt-research/rvt_core.abi3.so.new && mv $HOME/rvt-research/rvt_core.abi3.so.new $HOME/rvt-research/rvt_core.abi3.so
echo released
