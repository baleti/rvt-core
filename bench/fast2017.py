import os, sys, time
sys.path.insert(0, os.path.expanduser('~/src/rvt-core/python'))
os.environ['RVT_CORE_NOCACHE'] = '1'
import rvt_fast as RF
f = sys.argv[1]
print('on', RF.on(), 'nofast', os.environ.get('RVT_NOFAST'))
for i in range(2):
    t = time.time()
    try: x = RF.extract(f, want_all=True); print('extract', round(time.time() - t, 2), len(x['recs']), flush=True)
    except Exception as ex: import traceback; traceback.print_exc(); break
import rvt_scan, rvt_entities
print('W', x.get('W'))
t = time.time(); m = RF.Model(f); q = m.quick(); print('quick', round(time.time() - t, 2), flush=True)
