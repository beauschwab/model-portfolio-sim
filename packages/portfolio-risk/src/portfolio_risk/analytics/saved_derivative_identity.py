"""Exact original JSON/type/binary64 identity; no financial calculation."""
import json,math,struct
MAX_BYTES=32*1024*1024
def exact_identity(a,b):
    if type(a)is not type(b):return False
    if type(b)is dict:return set(a)==set(b)and all(type(k)is str and exact_identity(a[k],v)for k,v in b.items())
    if type(b)is list:return len(a)==len(b)and all(exact_identity(x,y)for x,y in zip(a,b))
    if type(b)is float:return math.isfinite(a)and math.isfinite(b)and struct.pack('>d',a)==struct.pack('>d',b)
    return type(b)in(str,int,bool,type(None))and a==b

def encoded(v):return json.dumps(v,ensure_ascii=False,allow_nan=False,sort_keys=True,separators=(',',':')).encode()
def loads(data,limit=MAX_BYTES):
    if type(data)not in(bytes,str)or len(data.encode()if type(data)is str else data)>limit:raise ValueError('derivative JSON byte cap')
    def pairs(items):
        out={}
        for key,value in items:
            if key in out:raise ValueError('duplicate derivative JSON key')
            out[key]=value
        return out
    def invalid(v):raise ValueError('nonfinite derivative JSON')
    def finite_float(value):
        out=float(value)
        if not math.isfinite(out):invalid(value)
        return out
    return json.loads(data,object_pairs_hook=pairs,parse_constant=invalid,parse_float=finite_float)
def probe(check):
    if check():raise InterruptedError('derivative source/publication cancelled')
