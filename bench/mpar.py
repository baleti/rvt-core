import sys, time, os
sys.path.insert(0, os.environ.get('RVT_FAST_DIR') or os.path.join(os.path.dirname(os.path.abspath(__file__)), '..', 'python')); import rvt_fast as RF
f = sys.argv[1]
t = time.time(); d = RF.Doc(f); cen = d._census(); md = RF.mesh_doc(d); print('doc+census+ctx %.2fs' % (time.time() - t))
ids = sorted(e for e, i in d.res.items() if i.get('bbox') and cen['cls'].get(e) and not cen['cls'][e].endswith('Symbol'))
for compact in (True, False):
    md = d._md = None; md = RF.mesh_doc(d)
    t = time.time(); r = md.element_meshes(ids, compact=compact); dt = time.time() - t
    print('%d elements compact=%s: %.2fs, %d tris, %d errors' % (len(ids), compact, dt, sum(len(x['tris']) for x in r if x), sum(1 for x in r if x is None)))
