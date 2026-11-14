"""regression (item 101): 2018 Autodesk rac_advanced_sample_project, ProjectPhase 86961 must read its real ElementHeader
(category -2000112 as unsigned 32-bit), not the misaligned candidate with category == family == ownerView == 65536."""
import glob, os, sys, pytest
C = os.path.expanduser('~/rvt-research/corpus/2018/09*.rvt')
F = (glob.glob(C) or [None])[0]
@pytest.mark.skipif(F is None, reason='corpus sample not present')
def test_projectphase_header():
    sys.path.insert(0, os.environ.get('RVT_FAST_DIR', os.path.join(os.path.dirname(__file__), '..', 'dev')))
    sys.path.insert(0, os.path.expanduser('~/rvt-research'))
    import rvt_fast
    x = rvt_fast.extract(F, want_all=True)
    h = x['eh'][86961]
    assert int(h['m_categroryId']) != 65536
    assert not (int(h['m_categroryId']) == int(h['m_familyId']) == int(h['m_ownerViewId']))

G = (glob.glob(os.path.expanduser('~/rvt-research/corpus/2023/36_rst_basic*.rvt')) or [None])[0]
@pytest.mark.skipif(G is None, reason='corpus sample not present')
def test_paramelemexternal_header():
    """2023 rst_basic 1372251 and no header with a family id in (2^31, 2^32-1)"""
    sys.path.insert(0, os.environ.get('RVT_FAST_DIR', os.path.join(os.path.dirname(__file__), '..', 'dev')))
    sys.path.insert(0, os.path.expanduser('~/rvt-research'))
    import rvt_fast
    x = rvt_fast.extract(G, want_all=True)
    assert int(x['eh'][1372251]['m_designOptionId']) != 37027840
    assert not [e for e, f in x['eh'].items() if (1 << 31) < int(f['m_familyId']) < (1 << 32) - 1]
