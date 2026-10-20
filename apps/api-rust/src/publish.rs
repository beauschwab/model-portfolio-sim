//! Bounded, checksummed publication objects for one verified native stream, and their
//! bounded reopen. Objects are written before anything references them; the manifest is then
//! reread, every stream chunk and table page is reread and checked, and the reread stream is
//! verified independently again before the final SQL transaction may reference the manifest.
//!
//! Stream chunks keep the producer's bytes verbatim (`bin`), so persisted verification replays
//! exactly what was produced. Table pages (`json`) are bounded views for paging; they are
//! formatting only and never feed verification.
use crate::objects::{Descriptor, Objects};
use crate::repo::Claim;
use crate::Refusal;
use portfolio_verification_native::{verify, Receipt, MAX_REQUEST_BYTES, MAX_STREAM_BYTES};
use serde_json::{json, Map, Value};

pub const MANIFEST_SCHEMA: &str = "native-lifecycle-publication-1";
pub const CHUNK_BYTES: usize = 4 * 1024 * 1024;
pub const PAGE_ROWS: usize = 1000;
pub const MAX_MANIFEST_BYTES: u64 = 16 * 1024 * 1024;
pub const MAX_PAGE_BYTES: u64 = 32 * 1024 * 1024;
pub const MAX_PAGE_LIMIT: usize = 1000;
/// Dictionary-encoded columns of the native columnar journal batch.
pub const JOURNAL_DICTIONARY_COLUMNS: [&str; 5] = [
    "scenario",
    "account",
    "event",
    "gl_account",
    "instrument_id",
];

fn storage(e: impl std::fmt::Display) -> Refusal {
    Refusal::new("STORAGE_ERROR", e.to_string())
}

fn chunks(stream: &[u8]) -> Vec<&[u8]> {
    // Whole lines, at most CHUNK_BYTES each unless a single line is longer.
    let mut out = Vec::new();
    let mut start = 0;
    while start < stream.len() {
        let limit = (start + CHUNK_BYTES).min(stream.len());
        let end = if limit == stream.len() {
            limit
        } else {
            match stream[start..limit].iter().rposition(|b| *b == b'\n') {
                Some(p) => start + p + 1,
                None => stream[limit..]
                    .iter()
                    .position(|b| *b == b'\n')
                    .map_or(stream.len(), |p| limit + p + 1),
            }
        };
        out.push(&stream[start..end]);
        start = end;
    }
    out
}

fn put_json(objects: &Objects, value: &Value) -> Result<Descriptor, Refusal> {
    Ok(objects.put(&serde_json::to_vec(value).expect("JSON serializes"), "json")?)
}

fn segment(objects: &Objects, table: &str, offset: usize, line: &Value) -> Result<Value, Refusal> {
    if let Some(rows) = line.get("rows").and_then(Value::as_array) {
        let mut pages = Vec::new();
        for (k, page) in rows.chunks(PAGE_ROWS).enumerate() {
            let d = put_json(objects, &json!({"table": table, "rows": page}))?;
            pages.push(json!({"offset": offset + k * PAGE_ROWS, "rows": page.len(), "object": d.to_json()}));
        }
        return Ok(json!({"offset": offset, "rows": rows.len(), "layout": "rows", "pages": pages}));
    }
    let columns = line
        .get("columns")
        .and_then(Value::as_object)
        .ok_or_else(|| {
            Refusal::new(
                "STREAM_LAYOUT",
                format!("table `{table}` has neither rows nor columns"),
            )
        })?;
    let count = columns
        .values()
        .next()
        .and_then(Value::as_array)
        .map_or(0, Vec::len);
    if columns
        .values()
        .any(|c| c.as_array().map(Vec::len) != Some(count))
    {
        return Err(Refusal::new(
            "STREAM_LAYOUT",
            format!("table `{table}` columns differ in length"),
        ));
    }
    let dictionary = match line.get("dictionary") {
        Some(d) => Some(put_json(objects, &json!({"table": table, "dictionary": d}))?.to_json()),
        None => None,
    };
    let starts: Vec<usize> = if count == 0 {
        vec![0]
    } else {
        (0..count).step_by(PAGE_ROWS).collect()
    };
    let mut pages = Vec::new();
    for start in starts {
        let end = (start + PAGE_ROWS).min(count);
        let slice: Map<String, Value> = columns
            .iter()
            .map(|(k, v)| {
                (
                    k.clone(),
                    Value::Array(v.as_array().expect("column")[start..end].to_vec()),
                )
            })
            .collect();
        let d = put_json(objects, &json!({"table": table, "columns": slice}))?;
        pages.push(json!({"offset": offset + start, "rows": end - start, "object": d.to_json()}));
    }
    Ok(
        json!({"offset": offset, "rows": count, "layout": "columns", "dictionary": dictionary, "pages": pages}),
    )
}

/// Writes every object, then the manifest; returns the manifest descriptor and its value.
pub fn write(
    objects: &Objects,
    claim: &Claim,
    raw: &[u8],
    stream: &[u8],
    receipt: &Receipt,
) -> Result<(Descriptor, Value), Refusal> {
    let mut chunk_refs = Vec::new();
    for c in chunks(stream) {
        chunk_refs.push(objects.put(c, "bin")?.to_json());
    }
    let mut tables: Vec<(String, Vec<Value>, usize)> = Vec::new();
    let mut header: Option<Descriptor> = None;
    let mut lines = 0usize;
    for line in std::str::from_utf8(stream)
        .map_err(|_| Refusal::new("STREAM_LAYOUT", "stream is not UTF-8"))?
        .lines()
        .filter(|l| !l.trim().is_empty())
    {
        lines += 1;
        let value: Value =
            serde_json::from_str(line).map_err(|e| Refusal::new("STREAM_LAYOUT", e.to_string()))?;
        if let Some(w) = value.get("workflow") {
            // The verified producer receipt, for views that read it without the whole stream.
            header = Some(put_json(objects, &json!({"workflow": w}))?);
            continue;
        }
        let Some(table) = value.get("table").and_then(Value::as_str) else {
            continue;
        };
        let position = tables.iter().position(|(t, _, _)| t == table);
        let index = match position {
            Some(i) => i,
            None => {
                tables.push((table.to_string(), Vec::new(), 0));
                tables.len() - 1
            }
        };
        let offset = tables[index].2;
        let seg = segment(objects, table, offset, &value)?;
        tables[index].2 += seg["rows"].as_u64().unwrap_or(0) as usize;
        tables[index].1.push(seg);
    }
    let a = &claim.authority;
    let manifest = json!({
        "schema": MANIFEST_SCHEMA,
        "job_id": claim.job_id,
        "attempt": claim.attempt,
        "source": {
            "schema": a.schema, "raw": a.raw.to_json(), "admitted_revision": a.admitted_revision,
            "admitted_at": a.admitted_at, "deadline_at": a.deadline_at, "authority_sha256": claim.authority_sha256,
        },
        "producer": {"executable_name": a.executable_name, "executable_sha256": a.executable_sha256},
        "verification": {"verifier": a.verifier, "receipt": serde_json::to_value(receipt).expect("receipt")},
        "header": header.map(|d| d.to_json()),
        "stream": {"bytes": stream.len(), "sha256": crate::sha256_hex(stream), "lines": lines, "chunks": chunk_refs},
        "tables": tables.iter().map(|(t, segs, rows)| json!({"table": t, "rows": rows, "segments": segs})).collect::<Vec<_>>(),
        "raw_sha256": crate::sha256_hex(raw),
    });
    let d = put_json(objects, &manifest)?;
    Ok((d, manifest))
}

pub fn load_manifest(objects: &Objects, d: &Descriptor) -> Result<Value, Refusal> {
    let bytes = objects.get_bounded(d, MAX_MANIFEST_BYTES)?;
    let m: Value = serde_json::from_slice(&bytes).map_err(storage)?;
    if m["schema"] != MANIFEST_SCHEMA {
        return Err(Refusal::new(
            "STORAGE_ERROR",
            "unsupported publication manifest",
        ));
    }
    Ok(m)
}

/// Rereads everything the manifest references from storage, checks every checksum and the
/// stream identity, and verifies the reread raw input and stream independently again. The
/// reread receipt must equal the one the manifest records.
pub fn reread(objects: &Objects, d: &Descriptor, expected: &Value) -> Result<Receipt, Refusal> {
    let m = load_manifest(objects, d)?;
    if &m != expected {
        return Err(Refusal::new(
            "REREAD_MISMATCH",
            "reread manifest differs from the written manifest",
        ));
    }
    let raw_d = Descriptor::from_json(&m["source"]["raw"])?;
    let raw = objects.get_bounded(&raw_d, MAX_REQUEST_BYTES as u64)?;
    let mut stream = Vec::new();
    for c in m["stream"]["chunks"]
        .as_array()
        .ok_or_else(|| Refusal::new("STORAGE_ERROR", "chunks missing"))?
    {
        let cd = Descriptor::from_json(c)?;
        if stream.len() as u64 + cd.bytes > MAX_STREAM_BYTES as u64 {
            return Err(Refusal::new(
                "STORAGE_ERROR",
                "persisted stream exceeds its bound",
            ));
        }
        stream.extend(objects.get_bounded(&cd, MAX_STREAM_BYTES as u64)?);
    }
    if crate::sha256_hex(&stream) != m["stream"]["sha256"].as_str().unwrap_or("")
        || Some(stream.len() as u64) != m["stream"]["bytes"].as_u64()
    {
        return Err(Refusal::new(
            "REREAD_MISMATCH",
            "persisted stream identity differs",
        ));
    }
    if let Some(h) = m.get("header").filter(|h| !h.is_null()) {
        let bytes = objects.get_bounded(&Descriptor::from_json(h)?, MAX_PAGE_BYTES)?;
        let stored: Value = serde_json::from_slice(&bytes).map_err(storage)?;
        let first = std::str::from_utf8(&stream)
            .ok()
            .and_then(|t| t.lines().find(|l| !l.trim().is_empty()))
            .and_then(|l| serde_json::from_str::<Value>(l).ok());
        if first.as_ref() != Some(&stored) {
            return Err(Refusal::new(
                "REREAD_MISMATCH",
                "persisted producer receipt differs from the stream",
            ));
        }
    }
    for table in m["tables"].as_array().into_iter().flatten() {
        for seg in table["segments"].as_array().into_iter().flatten() {
            if let Some(dict) = seg.get("dictionary").filter(|v| !v.is_null()) {
                objects.get_bounded(&Descriptor::from_json(dict)?, MAX_PAGE_BYTES)?;
            }
            for page in seg["pages"].as_array().into_iter().flatten() {
                objects.get_bounded(&Descriptor::from_json(&page["object"])?, MAX_PAGE_BYTES)?;
            }
        }
    }
    let receipt =
        verify(&raw, &stream).map_err(|e| Refusal::new("PERSISTED_VERIFICATION_REFUSED", e))?;
    if serde_json::to_value(&receipt).expect("receipt") != m["verification"]["receipt"] {
        return Err(Refusal::new(
            "REREAD_MISMATCH",
            "persisted verification receipt differs",
        ));
    }
    Ok(receipt)
}

/// The verified producer receipt of a publication (its workflow header).
pub fn receipt(objects: &Objects, d: &Descriptor) -> Result<Value, Refusal> {
    let m = load_manifest(objects, d)?;
    let h = m
        .get("header")
        .filter(|h| !h.is_null())
        .ok_or_else(|| Refusal::new("NOT_FOUND", "this publication has no producer receipt"))?;
    let bytes = objects.get_bounded(&Descriptor::from_json(h)?, MAX_PAGE_BYTES)?;
    serde_json::from_slice(&bytes).map_err(storage)
}

/// One bounded page of a published table: only the pages overlapping the window are read.
pub fn page(
    objects: &Objects,
    d: &Descriptor,
    table: &str,
    offset: usize,
    limit: usize,
) -> Result<Value, Refusal> {
    if !(1..=MAX_PAGE_LIMIT).contains(&limit) {
        return Err(Refusal::new(
            "MALFORMED_INPUT",
            "limit must be within [1, 1000]",
        ));
    }
    let m = load_manifest(objects, d)?;
    let entry = m["tables"]
        .as_array()
        .into_iter()
        .flatten()
        .find(|t| t["table"] == table)
        .ok_or_else(|| Refusal::new("NOT_FOUND", format!("table `{table}` is not published")))?;
    let total = entry["rows"].as_u64().unwrap_or(0) as usize;
    let end = (offset + limit).min(total);
    let mut rows = Vec::new();
    for seg in entry["segments"].as_array().into_iter().flatten() {
        let dictionary = match seg.get("dictionary").filter(|v| !v.is_null()) {
            Some(dd) if seg["layout"] == "columns" => {
                let seg_start = seg["offset"].as_u64().unwrap_or(0) as usize;
                let seg_end = seg_start + seg["rows"].as_u64().unwrap_or(0) as usize;
                if seg_end <= offset || seg_start >= end {
                    None
                } else {
                    let bytes = objects.get_bounded(&Descriptor::from_json(dd)?, MAX_PAGE_BYTES)?;
                    let v: Value = serde_json::from_slice(&bytes).map_err(storage)?;
                    Some(v["dictionary"].clone())
                }
            }
            _ => None,
        };
        for p in seg["pages"].as_array().into_iter().flatten() {
            let p_start = p["offset"].as_u64().unwrap_or(0) as usize;
            let p_end = p_start + p["rows"].as_u64().unwrap_or(0) as usize;
            if p_end <= offset || p_start >= end {
                continue;
            }
            let bytes =
                objects.get_bounded(&Descriptor::from_json(&p["object"])?, MAX_PAGE_BYTES)?;
            let v: Value = serde_json::from_slice(&bytes).map_err(storage)?;
            let lo = offset.max(p_start) - p_start;
            let hi = end.min(p_end) - p_start;
            if let Some(r) = v["rows"].as_array() {
                rows.extend(r[lo..hi].iter().cloned());
            } else if let Some(cols) = v["columns"].as_object() {
                for k in lo..hi {
                    let mut row = Map::new();
                    for (name, col) in cols {
                        let cell = col[k].clone();
                        let decoded = match (
                            &dictionary,
                            table == "journal"
                                && JOURNAL_DICTIONARY_COLUMNS.contains(&name.as_str()),
                        ) {
                            (Some(dict), true) => cell
                                .as_u64()
                                .and_then(|i| dict.get(i as usize))
                                .cloned()
                                .ok_or_else(|| {
                                    Refusal::new(
                                        "STORAGE_ERROR",
                                        "journal dictionary index out of range",
                                    )
                                })?,
                            _ => cell,
                        };
                        row.insert(name.clone(), decoded);
                    }
                    rows.push(Value::Object(row));
                }
            }
        }
    }
    Ok(json!({"table": table, "offset": offset, "limit": limit, "total": total, "rows": rows}))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn chunks_keep_whole_lines_and_every_byte() {
        let line = "x".repeat(1024 * 1024) + "\n";
        let stream = line.repeat(9);
        let parts = chunks(stream.as_bytes());
        assert!(parts.len() >= 3);
        assert_eq!(parts.concat(), stream.as_bytes());
        assert!(parts.iter().all(|p| p.ends_with(b"\n")));
        assert!(parts.iter().all(|p| p.len() <= CHUNK_BYTES));
    }
}
