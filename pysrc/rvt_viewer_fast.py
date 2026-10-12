"""Drop-ins for the viewer (session 187, ~/src/rvt-viewer) built on rvt_fast.Model. Same results as the viewer's own functions:
  load(path, log)        == rvtviewer.reader.load(path, log)     {'recs','eh','W','params','classes','tagbase'}
  em_geometry(model, e)  == rvtviewer.build._em_geometry(GG, src, e) for one element (from the Rust mesh; see README for the equivalence rule)
  em_geometries(model, ids, chunk) -> iterator of (id, result) as meshes finish (progress file updated)
Usage: sys.path.insert(0, os.path.expanduser('~/src/rvt-core/python')); import rvt_viewer_fast as VF; m = VF.open(path); d = VF.load(path, model=m)"""
import os, sys
HERE = os.path.dirname(os.path.abspath(__file__))
if HERE not in sys.path: sys.path.insert(0, HERE)
import rvt_fast as RF

def open(path, progress=True):
    return RF.Model(path, progress=progress)

def load(path, log=print, model=None):
    import time
    m = model or RF.Model(path); t = time.time()
    x, par = m.full()
    log('reader (rust): %d records, %d headers, %d param sets in %.1fs' % (len(x['recs']), len(x['eh']), len(par), time.time() - t))
    return {'recs': x['recs'], 'eh': x['eh'], 'W': x['W'], 'params': par, 'classes': [c['name'] for c in x['sch'].c], 'tagbase': 12}

def _curved():
    try:
        sys.path.insert(0, os.path.expanduser('~/src/rvt-viewer')); from rvtviewer.build import CURVED_SURFACES; return set(CURVED_SURFACES)
    except Exception: return {'cylinder', 'cone', 'revolution', 'ruled', 'hermite'}

def _result(r, full=None):
    """r: compact mesh; full: element_mesh-form mesh (needed when tris_raw/meta_raw apply: curved faces, <= 1500 triangles)"""
    import numpy as np
    if r is None: return {'confidence': 'none', 'flags': ['decode_failed_or_over_memory_guard']}
    if not len(r['tris']): return {'confidence': 'none', 'flags': list(r.get('flags') or []) or ['no_saved_graphics']}
    P = np.asarray(r['tris'], float).reshape(-1, 3)
    names = r.get('surf_names') or []
    surf = [names[i] for i in r['surf'].tolist()] if 'surf' in r else None
    raw = full is not None and len(r['tris']) <= 1500 and surf is not None and any(k in CURVED for k in surf)
    return {'confidence': 'mesh_approx', 'flags': ['saved_graphics_tessellation'] + list(r['flags']), 'nid': r['nid'].tolist(), 'fid': r['fid'].tolist(),
            'mat': [int(x) for x in r['mat'].tolist()] if r.get('mat') is not None and len(r['mat']) == len(r['tris']) else None,
            'surf': surf, 'tris_raw': full['tris'] if raw else None, 'meta_raw': full['meta'] if raw else None,
            'solids': [{'kind': 'triangle_mesh', 'triangles': P.round(6).tolist(), 'source': r['source']}]}

CURVED = _curved()
def _needs_full(r):
    return r is not None and 0 < len(r['tris']) <= 1500 and any(n in CURVED for n in (r.get('surf_names') or []))

def em_geometry(model, e):
    md = RF.mesh_doc(model.doc())
    r = md.element_meshes([int(e)], compact=True)[0]
    return _result(r, md.element_mesh(int(e)) if _needs_full(r) else None)

def em_geometries(model, ids, chunk=128):
    md = None
    for part in model.start_meshes(ids, chunk=chunk):
        for e, r in part:
            full = None
            if _needs_full(r):
                md = md or RF.mesh_doc(model.doc()); full = md.element_mesh(int(e))
            yield e, _result(r, full)
