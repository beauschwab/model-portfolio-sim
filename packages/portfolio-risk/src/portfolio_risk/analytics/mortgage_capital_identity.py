"""Nonfinancial normalized typed identity encoding; no producer-shape typing."""
import hashlib
import math
import struct

POLICY='typed-json-binary64-identity-1'

def digest(value):
    out=hashlib.sha256()
    def size(n):out.update(n.to_bytes(8,'big'))
    def encode(v):
        if v is None:out.update(b'n')
        elif type(v) is bool:out.update(b't' if v else b'f')
        elif type(v) is int:
            data=str(v).encode('ascii');out.update(b'i');size(len(data));out.update(data)
        elif type(v) is float:
            if not math.isfinite(v):raise ValueError('finite typed identity float required')
            out.update(b'd');out.update(struct.pack('>d',v))
        elif type(v) is str:
            data=v.encode('utf-8');out.update(b's');size(len(data));out.update(data)
        elif type(v) is list:
            out.update(b'a');size(len(v))
            for item in v:encode(item)
        elif type(v) is dict and all(type(k) is str for k in v):
            out.update(b'o');size(len(v))
            for key in sorted(v,key=lambda k:k.encode('utf-8')):encode(key);encode(v[key])
        else:raise ValueError('typed JSON identity domain required')
    encode(value)
    return out.hexdigest()
