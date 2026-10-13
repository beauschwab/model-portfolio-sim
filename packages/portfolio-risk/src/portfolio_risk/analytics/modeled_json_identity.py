def exact_json_identity(left, right):
    """Retained queued JSON identity, independent of numeric comparison gates."""
    import math
    import struct
    if type(left) is not type(right):return False
    if type(left) is float:
        return math.isfinite(left) and math.isfinite(right) and struct.pack('>d',left)==struct.pack('>d',right)
    if type(left) in (str,int,bool,type(None)):return left==right
    if type(left) is list:
        return len(left)==len(right) and all(exact_json_identity(a,b) for a,b in zip(left,right))
    if type(left) is dict:
        return all(type(k) is str for k in left) and all(type(k) is str for k in right) and left.keys()==right.keys() and all(exact_json_identity(left[k],right[k]) for k in left)
    return False


