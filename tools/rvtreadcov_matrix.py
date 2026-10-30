#!/usr/bin/env python3
"""Aggregate streams.jsonl / records.jsonl (rvtreadcov output) into the RVT read table of COVERAGE-MATRIX.md (stdout). Measured cells only."""
import sys, json, collections
def load(p):
    try: return [json.loads(l) for l in open(p)]
    except FileNotFoundError: return []
S = load(sys.argv[1]); R = load(sys.argv[2]) if len(sys.argv) > 2 else []
yrs = sorted({r.get('year') for r in S if r.get('year')})
print('## RVT read (c3x-rvtread, local corpus, Autodesk samples + templates; counts = files ok/total)\n')
print('| year | files | opens | streams inflate/unzip ok | opaque stream kinds | records: elements complete+exact (files measured) | failing files |')
print('|---|---|---|---|---|---|---|')
for y in yrs:
    f = [r for r in S if r.get('year') == y]; op = [r for r in f if r['open'] == 'ok']
    sok = [r for r in op if not any(v[1].startswith('error') for v in r['streams'].values())]
    opq = sorted({n for r in op for n, v in r['streams'].items() if v[1] == 'opaque'})
    rr = [r for r in R if r.get('year') == y and isinstance(r.get('rec'), dict)]
    ex = sum(r['rec']['el_exact'] for r in rr); el = sum(r['rec']['elements'] for r in rr)
    rec = f'{ex}/{el} ({len(rr)} files)' if rr else 'not measured'
    bad = sorted(r['sha8'] for r in f if r not in sok)
    print(f'| {y} | {len(f)} | {len(op)}/{len(f)} | {len(sok)}/{len(op)} | {", ".join(opq)} | {rec} | {", ".join(bad) or "none"} |')
print('\n2016: no corpus file (not measured). Edit/write columns: see other roles.')
