# viewer conversion with private caches: bench/conv_profile.py FILE BASEDIR [p] (p = cProfile to BASEDIR/p.out); SHARED=1 patches get_x with rvt_fast.extract_shared (dev build)
import sys, os, time, json, cProfile, pstats, tempfile
f=sys.argv[1]; base=sys.argv[2]; prof=len(sys.argv)>3
sys.path.insert(0, os.path.expanduser('~/src/rvt-viewer')); sys.path.insert(0, os.path.expanduser('~/rvt-research'))
os.environ['RVT_VIEWER_CACHE']=base+'/vc'; os.makedirs(base+'/vc',exist_ok=True); os.makedirs(base+'/xc',exist_ok=True)
if os.environ.get('SHARED'): sys.path.insert(0, os.path.expanduser('~/src/rvt-core/dev'))
import rvt_xcache; rvt_xcache.D=base+'/xc'
if os.environ.get('SHARED'):
    import rvt_fast as _F; assert 'dev' in _F.__file__
    rvt_xcache.get_x = _F.extract_shared
from rvtviewer import build
out=tempfile.mkdtemp(dir=base)
if prof:
    pr=cProfile.Profile(); pr.enable()
t=time.time(); m=build.convert(f,out,log=lambda *a:None); print(json.dumps(m['timings']))
if prof:
    pr.disable(); pr.dump_stats(base+'/p.out'); st=pstats.Stats(pr); st.sort_stats('cumulative').print_stats(45); st.sort_stats('tottime').print_stats(30)
