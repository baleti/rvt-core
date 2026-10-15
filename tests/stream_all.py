"""robustness sweep of the fast pipeline the viewer uses: Model.quick / full / mesh_ids / every element mesh (rvt_viewer_fast.em_geometries, incl. tris_raw)
+ refined_volume of the curved ones, no reference. One line per file: counts, timings, exceptions (any exception or Rust panic is a FAIL). usage: stream_all.py FILE"""
import os, sys, time, traceback
HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, os.environ.get('RVT_FAST_DIR') or os.path.join(os.path.dirname(HERE), 'python'))
import rvt_viewer_fast as VF
f = sys.argv[1]; T = {}; err = []; n = nm = nr = ntri = 0
try:
    t = time.time(); m = VF.open(f, progress=False); q = m.quick(); T['quick'] = time.time() - t
    t = time.time(); d = VF.load(f, lambda *a: None, model=m); T['full'] = time.time() - t
    t = time.time(); ids = list(m.mesh_ids())
    for e, a in VF.em_geometries(m, ids):
        n += 1
        if a.get('confidence') != 'mesh_approx': continue
        nm += 1; ntri += len(a['solids'][0]['triangles'])
        if a.get('tris_raw') is not None:
            try: VF.refined_volume(a['tris_raw'], a['meta_raw'], fid=a.get('fid')); nr += 1
            except BaseException as ex: err.append('refvol %d: %r' % (e, ex))
    T['meshes'] = time.time() - t
except BaseException as ex:
    err.append(traceback.format_exc().strip().splitlines()[-1][:200])
print('%-48s el %6d meshed %6d tris %8d refvol %5d  %s  %s' % (os.path.basename(f)[:48], n, nm, ntri, nr, ' '.join('%s %.1fs' % kv for kv in T.items()),
      'OK' if not err else 'FAIL %d: %s' % (len(err), ' | '.join(err[:3]))), flush=True)
sys.exit(1 if err else 0)
