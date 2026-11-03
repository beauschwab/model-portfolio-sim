use super::*;
use std::sync::{Arc, Mutex};

fn rows() -> Vec<Value> {
    vec![
        json!({"null":null,"boolean":true,"integer":u64::MAX,"signed":i64::MIN}),
        json!({"zero":-0.0,"tiny":f64::from_bits(1),"large":f64::MAX,"array":[1,1.0,false,null]}),
        json!({"rights":{"receivable":1.25,"payable":-2.5},"label":"\"\\\n\u{0}雪😀".repeat(4096)}),
    ]
}

#[test]
fn borrowed_frame_preserves_frozen_value_bytes_for_complete_json_categories() {
    for table in ["path", "gross \"rights\"\n雪", ""] {
        for rows in [Vec::new(), rows()] {
            let old = serde_json::to_vec(&json!({"table":table,"rows":rows})).unwrap();
            let borrowed = serde_json::to_vec(&RowMessage { rows: &rows, table }).unwrap();
            assert_eq!(borrowed, old);
            assert_eq!(
                portfolio_model_core::strict_json::serialized_size(
                    &RowMessage { rows: &rows, table },
                    1 << 20
                )
                .unwrap(),
                old.len()
            );
        }
    }
}

#[derive(Clone)]
struct Limited {
    output: Arc<Mutex<Vec<u8>>>,
    limit: usize,
}
impl Write for Limited {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        let mut output = self.output.lock().unwrap();
        let remaining = self.limit.saturating_sub(output.len());
        if remaining == 0 {
            return Err(io::Error::other("fixture write boundary"));
        }
        let count = remaining.min(bytes.len());
        output.extend_from_slice(&bytes[..count]);
        Ok(count)
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}
fn sink(limit: usize) -> (Sink, Arc<Mutex<Vec<u8>>>) {
    let output = Arc::new(Mutex::new(Vec::new()));
    let mut sink = Sink::new(Box::new(Limited {
        output: output.clone(),
        limit,
    }));
    for row in rows() {
        sink.emit("path", row).unwrap();
    }
    sink.emit(
        "closing",
        json!({"assets":1.,"liabilities":2.,"equity":-1.}),
    )
    .unwrap();
    for (index, value) in [0., -0., 1., -2., f64::from_bits(1)]
        .into_iter()
        .enumerate()
    {
        sink.journal(
            index, "stress", index, "bank", "event", "cash", "loan", value,
        )
        .unwrap();
    }
    (sink, output)
}

fn independent_json_work(value: &Value) -> usize {
    match value {
        Value::Null | Value::Bool(_) => 1,
        Value::Number(_) => 33,
        Value::String(text) => 1 + text.len(),
        Value::Array(rows) => 1 + rows.iter().map(independent_json_work).sum::<usize>(),
        Value::Object(fields) => {
            1 + fields
                .iter()
                .map(|(key, v)| 1 + key.len() + independent_json_work(v))
                .sum::<usize>()
        }
    }
}

#[test]
fn prepaid_frames_preserve_all_wire_bytes_and_pay_every_cell_and_newline() {
    let (mut ordinary, wire) = sink(2 << 20);
    ordinary.flush().unwrap();
    let bytes = wire.lock().unwrap().clone();
    let work = std::str::from_utf8(&bytes)
        .unwrap()
        .lines()
        .map(|line| {
            independent_json_work(&serde_json::from_str::<Value>(line).unwrap())
                + 2 * (line.len() + 1)
        })
        .sum::<usize>();
    let (mut paid, actual) = sink(2 << 20);
    paid.prepaid_frames = true;
    cut_shape::with_budget(work, || {
        paid.flush()?;
        assert_eq!(cut_shape::spent(), Some(work));
        Ok(())
    })
    .unwrap();
    assert_eq!(*actual.lock().unwrap(), bytes);
    assert_eq!(paid.sent, ordinary.sent);
    assert!(paid.rows.values().all(Vec::is_empty));
}

#[test]
fn exhausted_frame_budget_retains_retry_state_and_cannot_emit_an_unpaid_frame() {
    let (mut paid, actual) = sink(2 << 20);
    paid.prepaid_frames = true;
    let dictionary = paid.dictionary.clone();
    let rows = paid.count;
    assert!(cut_shape::with_budget(0, || paid.flush()).is_err());
    assert!(actual.lock().unwrap().is_empty());
    assert_eq!(paid.dictionary, dictionary);
    assert_eq!(paid.sent, 0);
    assert_eq!(paid.count, rows);
    assert!(!paid.journal.value.is_empty());
    assert!(paid.rows.values().any(|v| !v.is_empty()));
    assert_eq!(cut_shape::spent(), None);
    cut_shape::with_budget(10_000_000, || paid.flush()).unwrap();
    assert!(!actual.lock().unwrap().is_empty());
}

// Frozen pre-change flush, including dictionary ownership and error ordering.
fn old_flush(sink: &mut Sink) -> R<()> {
    if !sink.journal.value.is_empty() {
        serde_json::to_writer(
            &mut sink.out,
            &JournalMessage {
                table: "journal",
                columns: &sink.journal,
                dictionary: &sink.dictionary[sink.sent..],
            },
        )
        .map_err(|e| e.to_string())?;
        sink.out.write_all(b"\n").map_err(|e| e.to_string())?;
        sink.sent = sink.dictionary.len();
        sink.journal = JournalColumns::default();
    }
    for (name, rows) in sink.rows.iter_mut() {
        if rows.is_empty() {
            continue;
        }
        serde_json::to_writer(&mut sink.out, &json!({"table":name,"rows":rows}))
            .map_err(|e| e.to_string())?;
        sink.out.write_all(b"\n").map_err(|e| e.to_string())?;
        rows.clear();
    }
    sink.count = 0;
    sink.out.flush().map_err(|e| e.to_string())
}
fn state(sink: &Sink) -> Value {
    json!({
        "count":sink.count,"sent":sink.sent,"dictionary":sink.dictionary,
        "journal":sink.journal,
        "rows":sink.rows.iter().map(|(name, rows)|(name,rows.len(),rows.capacity())).collect::<Vec<_>>()
    })
}

#[test]
fn borrowed_flush_retains_exact_bytes_dictionary_and_failure_state() {
    for limit in [0, 1, 127, 8191, 8192, 8193, 16384, 65536, usize::MAX] {
        let (mut old, old_output) = sink(limit);
        let (mut borrowed, borrowed_output) = sink(limit);
        let old_result = old_flush(&mut old);
        let result = borrowed.flush();
        assert_eq!(result, old_result, "write limit {limit}");
        assert_eq!(state(&borrowed), state(&old), "state at {limit}");
        assert_eq!(
            *borrowed_output.lock().unwrap(),
            *old_output.lock().unwrap(),
            "bytes at {limit}"
        );
    }
}

#[test]
fn successive_borrowed_flushes_keep_high_water_capacity_and_dictionary_suffixes() {
    let (mut old, old_output) = sink(usize::MAX);
    let (mut borrowed, borrowed_output) = sink(usize::MAX);
    for cycle in 0..3 {
        old_flush(&mut old).unwrap();
        borrowed.flush().unwrap();
        assert_eq!(state(&borrowed), state(&old));
        assert_eq!(
            *borrowed_output.lock().unwrap(),
            *old_output.lock().unwrap()
        );
        for target in [&mut old, &mut borrowed] {
            target
                .emit("path", json!({"cycle":cycle,"amount":-0.0}))
                .unwrap();
            target
                .journal(
                    cycle,
                    "stress",
                    cycle,
                    "bank",
                    "new event",
                    "cash",
                    "loan",
                    -1.,
                )
                .unwrap();
        }
    }
}

#[test]
fn mixed_rows_and_journal_auto_flush_at_the_original_shared_threshold() {
    let (mut old, old_output) = sink(usize::MAX);
    let (mut borrowed, borrowed_output) = sink(usize::MAX);
    let mut full_flushes = 0;
    for n in 0..(cut_sink::BATCH_ROWS * 2 + 23) {
        if n % 3 == 0 {
            let row = json!({"day":n,"signed_right":-0.0,"label":"雪😀"});
            // Frozen emit mechanics, with the pre-change flush callback.
            old.rows.entry("path".into()).or_default().push(row.clone());
            old.count += 1;
            if old.count >= cut_sink::BATCH_ROWS {
                old_flush(&mut old).unwrap();
            }
            borrowed.emit("path", row).unwrap();
        } else {
            let event = format!("event {} 雪", n % 31);
            let value = if n % 2 == 0 { 1.25 } else { -1.25 };
            // Frozen column append mechanics. intern/posting semantics did not
            // change; the frame construction is the independent oracle here.
            let codes = [
                old.intern("stress"),
                old.intern("bank"),
                old.intern(&event),
                old.intern("cash"),
                old.intern("loan"),
            ];
            old.reports.posting(codes[1], codes[3], codes[4], value);
            let j = &mut old.journal;
            j.transaction_number.push(n);
            j.scenario.push(codes[0]);
            j.day.push(n);
            j.account.push(codes[1]);
            j.event.push(codes[2]);
            j.gl_account.push(codes[3]);
            j.instrument_id.push(codes[4]);
            j.value.push(value);
            old.count += 1;
            if old.count >= cut_sink::BATCH_ROWS {
                old_flush(&mut old).unwrap();
            }
            borrowed
                .journal(n, "stress", n, "bank", &event, "cash", "loan", value)
                .unwrap();
        }
        assert_eq!(borrowed.count, old.count);
        if borrowed.count == 0 {
            full_flushes += 1;
            assert_eq!(state(&borrowed), state(&old));
            assert_eq!(
                *borrowed_output.lock().unwrap(),
                *old_output.lock().unwrap()
            );
        }
    }
    assert_eq!(full_flushes, 2);
    old_flush(&mut old).unwrap();
    borrowed.flush().unwrap();
    assert_eq!(state(&borrowed), state(&old));
    assert_eq!(
        *borrowed_output.lock().unwrap(),
        *old_output.lock().unwrap()
    );
    assert_eq!(borrowed.out.capacity(), 8192);
}
