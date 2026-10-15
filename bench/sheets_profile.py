"""profile the reader calls of rvt-sheets (rvtsheets.reader.File + drawings.Model) on one file: stage timings and the top cumulative reader functions.
usage: sheets_profile.py FILE [--prof]   (run on remote-host via h1run; python = ~/src/rvt-sheets-dev/.venv/bin/python)"""
import os, sys, time, cProfile, pstats
sys.path.insert(0, os.path.expanduser('~/src/rvt-sheets-dev')); sys.path.insert(0, os.path.expanduser('~/rvt-research'))
sys.path.insert(0, os.path.expanduser('~/src/rvt-core/python'))
f = os.path.abspath(sys.argv[1]); prof = '--prof' in sys.argv
def stage(name, fn):
    t = time.time(); r = fn(); print('%-28s %6.2fs' % (name, time.time() - t), flush=True); return r
from rvtsheets import reader
F = stage('File()', lambda: reader.File(f))
stage('classes/params()', lambda: F.params())
stage('graphics Doc', lambda: F.g)
ids = [r['id'] for r in F.recs('SWall', 'FamilyInstance')][:300] if F.recs('SWall', 'FamilyInstance') else []
stage('graphics(300 elems)', lambda: [F.graphics(i) for i in ids])
from rvtsheets import drawings
pr = cProfile.Profile() if prof else None
if pr: pr.enable()
M = stage('drawings.Model(file)', lambda: drawings.Model(f))
dec, doc = stage('decoder() (Decoder + doc)', lambda: M.decoder())
from rvtsheets.drawings import DRAWABLE
vs = [v for v in doc['views'].values() if not v.get('template') and v.get('kind') in DRAWABLE and v.get('name') and not v.get('error')]
ss = list(doc['sheets'].values())
byk = {}
for v in vs: byk.setdefault(v['kind'], []).append(v['id'])
views = [ids_[0] for ids_ in byk.values()][:int(os.environ.get('NV', '6'))]
print('view kinds', {k: len(v) for k, v in byk.items()}, 'sheets', len(ss))
for i in views: stage('render view %s %s' % (doc['views'][i]['kind'], i), lambda: drawings.render(M, 'view', i))
for sh in ss[:2]: stage('render sheet %s' % sh['id'], lambda: drawings.render(M, 'sheet', sh['id']))
if pr:
    pr.disable(); pstats.Stats(pr).sort_stats('cumulative').print_stats(45)
