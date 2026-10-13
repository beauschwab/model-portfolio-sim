"""Primitive report metadata transport; no financial computation."""
def source_summary(result):
    """Retain scalar source metadata without deriving financial amounts."""
    return {k:v for k,v in result.items() if v is None or isinstance(v,(str,int,float,bool)) or
            isinstance(v,dict) and all(x is None or isinstance(x,(str,int,float,bool)) for x in v.values())}

