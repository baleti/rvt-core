#!/bin/sh
# usage: tests/h1_retry.sh NAME ENV -- CMD...   retry h1run every 5 min while host1 is busy (max ~8 h); output is h1run's
cd "$(dirname "$0")/.."
N=$1; E=$2; shift 3; i=0
while [ $i -lt 96 ]; do
  out=$(~/.local/share/offload/h1run run "$N" --env "$E" --dir "$PWD" -- "$@" 2>&1)
  case "$out" in *busy*) i=$((i+1)); sleep 300;; *) echo "$out"; exit 0;; esac
done
echo gave up
