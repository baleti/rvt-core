"""canonical text of a decoded Python value: types (EID vs int, list vs tuple), dict key order, floats bit-exact (NaN == NaN)"""
def canon(x, out):
    t = type(x).__name__
    if isinstance(x, dict):
        out.append('{')
        for k, v in x.items(): out.append(repr(k)); out.append(':'); canon(v, out); out.append(',')
        out.append('}')
    elif isinstance(x, (list, tuple)):
        out.append('[' if isinstance(x, list) else '(')
        for v in x: canon(v, out); out.append(',')
        out.append(']' if isinstance(x, list) else ')')
    elif isinstance(x, float): out.append('f' + (x.hex() if x == x else 'nan'))
    else: out.append(t + ':' + repr(x))
def digest(x):
    import hashlib
    o = []; canon(x, o); return hashlib.sha1(''.join(o).encode()).hexdigest()[:16]
