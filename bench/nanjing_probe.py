"""probe of the host-dependent refined_volume of a ruled fan (Nanjing 546245): pole distances seen by the reference _fan_uv and both volumes. usage: nanjing_probe.py FILE EID"""
import os, sys
HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, os.environ.get('RVT_FAST_DIR') or os.path.join(os.path.dirname(HERE), 'python'))
import numpy as np, rvt_fast as RF, rvt_mesh as M
f, e = sys.argv[1], int(sys.argv[2])
md = RF.mesh_doc(RF.Doc(f)); RC = RF._rc(); m = md.element_mesh(e)
orig = M._fan_uv
def spy(kind, ev, uv, n):
    d = [float(np.linalg.norm(np.asarray(ev(float(a), float(b))) - np.asarray(ev(float(a) + 0.3, float(b))))) for a, b in uv]
    print('fan', kind, ['%.3e' % x for x in d], [tuple(map(float, q)) for q in uv])
    return orig(kind, ev, uv, n)
M._fan_uv = spy
print('ref ', M.refined_volume(m['tris'], m['meta'], fid=m.get('fid')))
print('rust', RC.refined_volume([[list(map(float, p)) for p in tr] for tr in m['tris']], m['meta'], fid=m.get('fid')))
print('meta kinds', sorted({str(x[0]) if isinstance(x, (list, tuple)) and x else str(x)[:30] for x in m['meta']}))
