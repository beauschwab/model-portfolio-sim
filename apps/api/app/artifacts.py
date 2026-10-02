"""Immutable, checksummed objects and versioned typed manifests. No pickle.

Parquet is the table persistence boundary. An Iceberg publisher can consume the
table references after job publication without changing API or pricing code.
"""
from __future__ import annotations

import datetime as dt
import hashlib
import io
import json
import math
import os
from pathlib import Path
import tempfile
from dataclasses import dataclass
from urllib.parse import urlparse

import numpy as np
import polars as pl
from pydantic import BaseModel


@dataclass(frozen=True)
class PartitionedTable:
    schema: dict
    parts: list

    def descriptor(self):
        return {"format": "partitioned-parquet", "schema": self.schema,
                "rows": sum(p['rows'] for p in self.parts), "parts": self.parts}


class Objects:
    def __init__(self, url, tenant, workspace, *, s3_client=None):
        self.prefix = "workbench/" + hashlib.sha256(f"{tenant}\0{workspace}".encode()).hexdigest()
        parsed = urlparse(url)
        self.bucket = parsed.netloc if parsed.scheme == "s3" else None
        if self.bucket:
            import boto3
            self.prefix = "/".join(filter(None, [parsed.path.strip("/"), self.prefix]))
            self.client = s3_client or boto3.client("s3", endpoint_url=os.getenv("S3_ENDPOINT_URL"))
        else:
            self.root = Path(url).resolve()
            self.root.mkdir(parents=True, exist_ok=True)

    def put(self, data: bytes, suffix: str):
        digest = hashlib.sha256(data).hexdigest()
        key = f"{self.prefix}/{digest[:2]}/{digest}.{suffix}"
        if self.bucket:
            self.client.put_object(Bucket=self.bucket, Key=key, Body=data,
                                   Metadata={"sha256": digest})
        else:
            target = self.root / key
            target.parent.mkdir(parents=True, exist_ok=True)
            if not target.exists():
                fd, temp = tempfile.mkstemp(dir=target.parent)
                try:
                    with os.fdopen(fd, "wb") as stream:
                        stream.write(data)
                        stream.flush()
                        os.fsync(stream.fileno())
                    os.replace(temp, target)
                finally:
                    if os.path.exists(temp):
                        os.unlink(temp)
        return {"key": key, "sha256": digest, "bytes": len(data), "format": suffix}

    def get(self, ref):
        digest = ref["sha256"]
        expected = f"{self.prefix}/{digest[:2]}/{digest}.{ref['format']}"
        if len(digest) != 64 or any(c not in "0123456789abcdef" for c in digest) or ref["key"] != expected:
            raise ValueError("artifact is outside this workspace or has an invalid identity")
        if ref["format"] not in {"json", "parquet", "bin"}:
            raise ValueError("unsupported artifact format")
        if self.bucket:
            response = self.client.get_object(Bucket=self.bucket, Key=expected)
            with response["Body"] as stream:
                data = stream.read()
        else:
            data = (self.root / expected).read_bytes()
        if len(data) != ref["bytes"] or hashlib.sha256(data).hexdigest() != digest:
            raise ValueError("artifact checksum mismatch")
        return data


class Codec:
    def __init__(self, objects):
        self.objects = objects

    def dump(self, value):
        manifest = {"schema_version": 1, "value": self.encode(value)}
        return self.objects.put(json.dumps(manifest, sort_keys=True, separators=(",", ":"),
                                           allow_nan=False).encode(), "json")

    def load(self, ref):
        manifest = json.loads(self.objects.get(ref))
        if manifest.get("schema_version") != 1:
            raise ValueError("unsupported artifact manifest version")
        return self.decode(manifest["value"])

    def encode(self, value):
        if isinstance(value, bytes):
            return {"type": "bytes", "ref": self.objects.put(value, "bin")}
        if isinstance(value, PartitionedTable):
            return {"type": "partitioned", "value": value.descriptor()}
        if isinstance(value, pl.DataFrame):
            obj = {}
            columns = []
            for name, dtype in value.schema.items():
                if dtype == pl.Object:
                    obj[name] = True
                    columns.append(pl.Series(name, [json.dumps(self.encode(v), allow_nan=False)
                                                    for v in value[name].to_list()], dtype=pl.String))
                else:
                    columns.append(value[name])
            stream = io.BytesIO()
            pl.DataFrame(columns).write_parquet(stream, compression="zstd")
            return {"type": "frame", "ref": self.objects.put(stream.getvalue(), "parquet"), "objects": obj}
        if isinstance(value, pl.Series):
            return {"type": "series", "value": self.encode(value.to_frame())}
        if isinstance(value, np.ndarray):
            if value.dtype.hasobject:
                raise TypeError("object arrays are not durable numerical data")
            return {"type": "array", "dtype": value.dtype.str, "shape": list(value.shape),
                    "value": self.encode(pl.DataFrame({"value": value.reshape(-1)}))}
        if isinstance(value, np.generic):
            return self.encode(value.item())
        if isinstance(value, BaseModel):
            return {"type": "model", "name": type(value).__name__, "value": self.encode(value.model_dump())}
        if isinstance(value, dict):
            return {"type": "dict", "items": [[self.encode(k), self.encode(v)] for k, v in value.items()]}
        if isinstance(value, (list, tuple)):
            return {"type": "tuple" if isinstance(value, tuple) else "list", "items": [self.encode(v) for v in value]}
        if isinstance(value, (dt.datetime, dt.date)):
            return {"type": "datetime" if isinstance(value, dt.datetime) else "date", "value": value.isoformat()}
        if isinstance(value, float) and not math.isfinite(value):
            return {"type": "float", "value": str(value)}
        if value is None or isinstance(value, (str, int, float, bool)):
            return value
        raise TypeError(f"unsupported durable value: {type(value).__name__}")

    def decode(self, value):
        if not isinstance(value, dict):
            return value
        kind = value["type"]
        if kind == "bytes":
            return self.objects.get(value['ref'])
        if kind == "partitioned":
            # Explicit descriptor: never hydrate a multi-million-row journal.
            return value['value']
        if kind == "frame":
            frame = pl.read_parquet(io.BytesIO(self.objects.get(value["ref"])))
            for name in value["objects"]:
                frame = frame.with_columns(pl.Series(name, [self.decode(json.loads(v)) for v in frame[name]], dtype=pl.Object))
            return frame
        if kind == "series":
            return self.decode(value["value"]).to_series()
        if kind == "array":
            return self.decode(value["value"])["value"].to_numpy().astype(value["dtype"]).reshape(value["shape"])
        if kind == "dict":
            return {self.decode(k): self.decode(v) for k, v in value["items"]}
        if kind in {"tuple", "list"}:
            result = [self.decode(v) for v in value["items"]]
            return tuple(result) if kind == "tuple" else result
        if kind in {"datetime", "date"}:
            return getattr(dt, kind).fromisoformat(value["value"])
        if kind == "float":
            return float(value["value"])
        if kind == "model":
            from . import schemas
            from .market_data import FetchRequest
            allowed = {name: cls for name, cls in vars(schemas).items()
                       if isinstance(cls, type) and issubclass(cls, BaseModel) and cls.__module__ == schemas.__name__}
            allowed["FetchRequest"] = FetchRequest
            return allowed[value["name"]](**self.decode(value["value"]))
        raise ValueError(f"unknown manifest node: {kind}")

    def tables(self, ref):
        """Return tabular result leaves by structural path, without loading them."""
        manifest = json.loads(self.objects.get(ref))
        found = {}
        def visit(node, path):
            if not isinstance(node, dict):
                return
            if node.get("type") == "frame":
                found[path or "$"] = node["ref"]
            elif node.get("type") == "partitioned":
                found[path or "$"] = node['value']
            elif node.get("type") == "dict":
                for key, child in node["items"]:
                    segment = str(key).replace('~', '~0').replace('/', '~1')
                    visit(child, f"{path}/{segment}")
            elif node.get("type") in {"tuple", "list"}:
                for i, child in enumerate(node["items"]):
                    visit(child, f"{path}/{i}")
        visit(manifest["value"], "")
        return found

    def import_balance_partitions(self, manifest_path, cancelled):
        """Copy verified engine partitions into the configured workspace store.

        Objects written before a failure are unreferenced; SQL publication remains
        the worker's fenced transaction. Memory is bounded to one partition.
        """
        path = Path(manifest_path).resolve()
        manifest = json.loads(path.read_text(encoding='utf-8'))
        if manifest.get('version') != 'balance-partitions-1' or not manifest['validation']['journal_replayed']:
            raise ValueError('unverified balance partition manifest')
        result = {}
        for name, table in manifest['tables'].items():
            parts = []
            for part in table['parts']:
                if cancelled():
                    raise InterruptedError('partition publication cancelled')
                file = (path.parent / part['path']).resolve()
                if file.parent != path.parent or file.suffix != '.parquet':
                    raise ValueError('invalid partition path')
                data = file.read_bytes()
                if len(data) != part['bytes'] or hashlib.sha256(data).hexdigest() != part['sha256']:
                    raise ValueError('partition checksum mismatch')
                frame = pl.read_parquet(io.BytesIO(data))
                if frame.height != part['rows'] or {k: str(v) for k,v in frame.schema.items()} != table['schema']:
                    raise ValueError('partition schema or row count mismatch')
                parts.append({'rows': part['rows'], 'ref': self.objects.put(data, 'parquet')})
            result[name] = PartitionedTable(table['schema'], parts)
        result['execution'] = {k: v for k,v in manifest.items() if k != 'tables'}
        return result
