"""Drop-ins for the viewer (session 187, ~/src/rvt-viewer) built on rvt_fast.Model. Same results as the viewer's own functions:
  load(path, log)        == rvtviewer.reader.load(path, log)     {'recs','eh','W','params','classes','tagbase'}
  em_geometry(model, e)  == rvtviewer.build._em_geometry(GG, src, e) for one element (from the Rust mesh; see README for the equivalence rule)
  em_geometries(model, ids, chunk) -> iterator of (id, result) as meshes finish (progress file updated)
  prestart(model)        start the mesh stream in the background right after model.quick() (em_geometries then consumes it)
  refined_volume(tris, meta, ..., fid) == rvt_mesh.refined_volume, in Rust for em_geometries' tris_raw/meta_raw (Facades 141 meshes: 34 s -> 0.6 s)
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

def prestart(model, ids=None, chunk=128):
    """start the Rust mesh stream early (right after model.quick()), in the background: meshes of `ids` (default model.mesh_ids(): elements with a channel-103 bbox,
    no symbols) compute while the caller runs full() / its tables; the next em_geometries(model, ...) consumes them. Returns at once."""
    import threading
    if getattr(model, '_pre_th', None) is not None: return
    def run():
        try: model._pre = model.start_meshes(list(ids) if ids is not None else model.mesh_ids(), chunk=chunk)
        except Exception: model._pre = None
    model._pre = None; model._pre_th = threading.Thread(target=run, daemon=True); model._pre_th.start()

def _stream(model, ids, chunk):
    """lists of (id, compact mesh) for ids. With a prestarted stream: meshes it already delivered are kept in a buffer on the model, so a caller that stops early
    (or asks for a subset) leaves the rest for the next call; ids the prestarted stream does not cover are computed afterwards by a new stream."""
    th = getattr(model, '_pre_th', None)
    if th is not None:
        th.join()
        buf = model.__dict__.setdefault('_pre_buf', {}); want = dict.fromkeys(int(i) for i in ids)
        def take():
            got = [(e, buf.pop(e)) for e in [e for e in want if e in buf]]
            for e, _ in got: del want[e]
            return got
        got = take()
        if got: yield got
        it = getattr(model, '_pre_it', None)
        if it is None and model._pre is not None: it = model._pre_it = iter(model._pre)
        while want and it is not None:
            part = next(it, None)
            if part is None: model._pre_it = None; model._pre = None; break
            buf.update(part); got = take()
            if got: yield got
        ids = list(want)
        if not ids: return
    yield from model.start_meshes(ids, chunk=chunk)

def em_geometries(model, ids, chunk=128):
    md = None
    for part in _stream(model, ids, chunk):
        for e, r in part:
            full = None
            if _needs_full(r):
                md = md or RF.mesh_doc(model.doc()); full = md.element_mesh(int(e))
            yield e, _result(r, full)

def refined_volume(tris, meta, levels=3, tol=5e-5, centroid=False, fid=None):
    return RF.refined_volume(tris, meta, levels=levels, tol=tol, centroid=centroid, fid=fid)

def body_summary(P, nid, fid=None, surf=None, limit=12):
    """== rvtviewer.build.body_summary (per-body triangles, faces, area, watertight, bounds, surface kinds): welding / edge parity / sums in Rust"""
    import numpy as np, collections
    P = np.ascontiguousarray(np.asarray(P, float)).reshape(-1, 3)
    nid = np.asarray(nid)
    fid_ = [int(x) for x in fid] if fid is not None and len(fid) == len(nid) else None
    rows, tb = RF._rc().body_summary_core(P.tobytes(), [int(x) for x in nid.tolist()], fid_, limit)
    tb = np.frombuffer(tb, dtype=np.int8)
    surfa = np.asarray(surf, dtype=object) if surf is not None and len(surf) == len(nid) else None
    out = []
    for k, (b, ntri, nfaces, area, odd, cs, lo, hi) in enumerate(rows):
        closed = 'yes' if not odd else ('sealed' if float(np.linalg.norm(np.asarray(cs))) < 1e-4 * max(area, 1e-9) else 'open')
        out.append({'id': int(b), 'kind': 'polymesh' if b == 10 ** 6 else ('symbol instance' if b >= 2 * 10 ** 7 else 'own geometry'), 'tris': int(ntri), 'faces': nfaces,
                    'surfaces': dict(collections.Counter(surfa[tb == k].tolist())) if surfa is not None else None, 'area': round(area, 3), 'closed': closed,
                    'lo': np.asarray(lo).round(3).tolist(), 'hi': np.asarray(hi).round(3).tolist()})
    return out
