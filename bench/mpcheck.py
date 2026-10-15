"""does the viewer's per-body material map (body_materials) change any triangle's material once the reader's per-triangle mat is present?"""
import sys, os, numpy as np
sys.path.insert(0, os.path.expanduser('~/src/rvt-viewer')); sys.path.insert(0, os.path.expanduser('~/rvt-research'))
from rvtviewer import build as B, materials as MT, reader
reader._import(); import rvt_geometry as GG
f = sys.argv[1]; VF = B.fast_module(); fm = VF.open(f, progress=False); raw = VF.load(f, lambda *a: None, model=fm)
GG._docs[f] = fm.doc()
idx = MT.load(f, log=lambda *a: None)['index']
ids = list(fm.mesh_ids()); n = nd = nt = nmp = 0; ex = []
for e, a in VF.em_geometries(fm, ids):
    if a.get('confidence') != 'mesh_approx' or a.get('nid') is None or a.get('mat') is None: continue
    mp = B.body_materials(GG, f, e); n += 1
    if not mp: continue
    nmp += 1
    nid = np.asarray(a['nid']); tri = np.array([idx.get(int(x), -1) for x in a['mat']])
    with_mp = np.full(len(nid), -1)
    for b, mid in mp.items():
        if mid in idx: with_mp[nid == b] = idx[mid]
    with_mp = np.where(tri >= 0, tri, with_mp)
    d = int((with_mp != tri).sum())
    if d: nd += 1; nt += d; ex.append((e, d, len(nid)))
print(os.path.basename(f), 'elements', n, 'with body map', nmp, 'elements changed by it', nd, 'triangles', nt, ex[:8])
