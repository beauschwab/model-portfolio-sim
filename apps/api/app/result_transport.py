"""ARW1 result transport. No workspace state or execution dependencies."""
import io
import json
import struct

import numpy as np
import polars as pl

# ---- Arrow envelope serialization -------------------------------------------
# Computed polars frames cross the wire as Apache Arrow IPC, not JSON. A result
# tree is split into a JSON "skeleton" (every pl.DataFrame leaf replaced by a
# {"__arrow__": i} marker) plus N concatenated Arrow IPC blobs. Scalars, lists,
# and numpy-derived arrays (e.g. runoff_vectors) stay inline in the skeleton.
ARROW_ENVELOPE_MIME = "application/vnd.arrow-envelope"
_IPC_COMPRESSION = None      # Arrow-JS IPC compression is version-fragile; these
                             # frames are small -- uncompressed is the safe wire.
# polars defaults string columns to the Arrow Utf8View layout (type 24), which
# apache-arrow JS cannot read. oldest() emits plain Utf8 the JS reader accepts.
_IPC_COMPAT = pl.CompatLevel.oldest()


def _arrow_safe(df: pl.DataFrame) -> pl.DataFrame:
    """Cast columns Arrow IPC cannot encode (pl.Object, e.g. the demo
    `call_schedule` list column) to JSON-encoded Utf8. Anything else
    unencodable surfaces loudly from write_ipc -- intentionally."""
    obj_cols = [c for c, dt in zip(df.columns, df.dtypes) if dt == pl.Object]
    if not obj_cols:
        return df
    return df.with_columns([
        pl.col(c).map_elements(
            lambda v: None if v is None else json.dumps(v, default=str),
            return_dtype=pl.Utf8).alias(c)
        for c in obj_cols
    ])


def _frames_to_arrow(obj):
    """Walk a result tree, returning (skeleton, blobs). Each pl.DataFrame leaf
    becomes a {"__arrow__": idx} marker and an Arrow IPC blob at that index."""
    blobs: list[bytes] = []

    def walk(o):
        import datetime
        if isinstance(o, (datetime.date, datetime.datetime)):
            return o.isoformat()
        if isinstance(o, bytes):
            import base64
            return {'__binary_base64__': base64.b64encode(o).decode('ascii')}
        if isinstance(o, pl.DataFrame):
            buf = io.BytesIO()
            _arrow_safe(o).write_ipc(buf, compression=_IPC_COMPRESSION,
                                     compat_level=_IPC_COMPAT)
            blobs.append(buf.getvalue())
            return {"__arrow__": len(blobs) - 1}
        if isinstance(o, dict):
            return {k: walk(v) for k, v in o.items()}
        if isinstance(o, (list, tuple)):
            return [walk(v) for v in o]
        if isinstance(o, np.ndarray):
            return walk(o.tolist())
        if isinstance(o, np.generic):
            return walk(o.item())
        if isinstance(o, float) and not np.isfinite(o):
            return None
        return o

    return walk(obj), blobs


def _pack_envelope(skeleton, blobs: list[bytes]) -> bytes:
    """Frame skeleton + blobs into the ARW1 binary envelope (little-endian)."""
    sj = json.dumps(skeleton, allow_nan=False).encode("utf-8")
    parts = [b"ARW1", struct.pack("<I", len(sj)), sj,
             struct.pack("<I", len(blobs))]
    parts += [struct.pack("<I", len(b)) for b in blobs]
    parts += blobs
    return b"".join(parts)


def to_arrow_envelope(obj) -> bytes:
    """Serialize a result object (frames + scalars) to an ARW1 envelope."""
    return _pack_envelope(*_frames_to_arrow(obj))
