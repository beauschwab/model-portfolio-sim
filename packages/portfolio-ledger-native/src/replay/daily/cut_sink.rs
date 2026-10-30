//! Retained Sink containers after row frames became borrowed serializers.
//!
//! This refines the existing logical-Value component convention; it is not an
//! allocator/RSS or complete simulator proof. Layout and growth controls target
//! the repository's pinned 64-bit Rust toolchain.
use super::cut_inventory::TableInventory;
use super::cut_shape::{charge, Shape};
use serde::Serialize;
use serde_json::Value;
use std::collections::BTreeMap;
use std::mem::size_of;

pub(super) const BATCH_ROWS: usize = 16_384;

#[derive(Clone, Debug, Serialize)]
pub struct CutSinkMemory {
    pub schema: &'static str,
    pub row_tree_bytes_upper: usize,
    pub row_vector_bytes_upper: usize,
    pub journal_vector_bytes_upper: usize,
    pub dictionary_payload_bytes_upper: usize,
    pub dictionary_container_bytes_upper: usize,
    pub table_container_bytes_upper: usize,
    pub writer_frame_bytes_upper: usize,
}
fn add(a: usize, b: usize) -> Result<usize, String> {
    a.checked_add(b).ok_or("cut Sink capacity overflow".into())
}
fn mul(a: usize, b: usize) -> Result<usize, String> {
    a.checked_mul(b).ok_or("cut Sink capacity overflow".into())
}

/// Push/clear retains the high-water buffer. On the pinned RawVec growth path,
/// old + new buffers during doubling require at most 3N slots, with allowance
/// for the minimum initial capacity. Count that overlap even on reallocators
/// which can grow in place. Clearing never lowers N's historical bound.
fn vector_slots(n: usize) -> Result<usize, String> {
    if n == 0 {
        Ok(0)
    } else {
        add(mul(n, 3)?, 8)
    }
}

pub(super) fn inventory(
    prefix: &BTreeMap<String, TableInventory>,
    visible: &BTreeMap<String, TableInventory>,
    row_encoded_bytes: usize,
    dictionary: Shape,
) -> Result<(CutSinkMemory, usize), String> {
    // A toolchain/target change cannot silently reuse another container layout.
    if size_of::<usize>() != 8 || size_of::<Value>() != 32 || size_of::<String>() != 24 {
        return Err("cut Sink census requires validated 64-bit container layouts".into());
    }
    let mut row_slots = 0;
    let mut journal_rows = 0;
    let mut table_names = 0;
    let mut scan_work = 0;
    for (name, table) in prefix.iter().chain(visible) {
        let work = add(name.len(), 1)?;
        charge(work)?;
        scan_work = add(scan_work, work)?;
        if name == "journal" {
            journal_rows = add(journal_rows, table.rows_upper)?;
        } else {
            // Duplicate table names across the two phases deliberately reserve
            // two high-water buffers. The live Sink has only one per name.
            row_slots = add(row_slots, vector_slots(table.rows_upper.min(BATCH_ROWS))?)?;
            table_names = add(table_names, name.len())?;
        }
    }
    let tables = add(prefix.len(), visible.len())?;
    let labels = dictionary.cells_upper;
    // The supplied shape includes repeated labels and dictionary resends. It
    // dominates unique labels without money/zero-weight deduplication.
    let dictionary_vectors = mul(vector_slots(labels)?, size_of::<String>())?;
    // Pinned hashbrown's large-table 87.5% load policy and small-table allowance
    // need fewer than 8D+32 buckets across old/new-table growth. 64 bytes covers
    // (String, usize), control bytes and alignment, without counting their
    // separately owned String payloads twice during a move.
    let dictionary_map = mul(add(mul(labels, 8)?, 32)?, 64)?;
    Ok((
        CutSinkMemory {
            schema: "commercial-borrowed-sink-memory-1",
            row_tree_bytes_upper: mul(row_encoded_bytes, 20)?,
            row_vector_bytes_upper: mul(row_slots, size_of::<Value>())?,
            // Seven usize columns and one f64 column. Charging simultaneous growth
            // of all eight is conservative; the actual pushes grow sequentially.
            journal_vector_bytes_upper: mul(
                vector_slots(journal_rows.min(BATCH_ROWS))?,
                size_of::<[usize; 7]>() + size_of::<f64>(),
            )?,
            dictionary_payload_bytes_upper: mul(dictionary.string_bytes_upper, 2)?,
            dictionary_container_bytes_upper: add(dictionary_vectors, dictionary_map)?,
            // A 4KiB node per table dominates pinned BTreeMap node storage, headers
            // and table-name headers. Include retained plus temporary name payloads.
            table_container_bytes_upper: add(mul(add(tables, 1)?, 4096)?, mul(table_names, 2)?)?,
            // Includes the pinned 8KiB BufWriter and conservative frame/formatter
            // workspace. Financial report/Sim maps remain a separate component.
            writer_frame_bytes_upper: 64 << 10,
        },
        scan_work,
    ))
}

impl CutSinkMemory {
    pub(super) fn total(&self) -> Result<usize, String> {
        [
            self.row_tree_bytes_upper,
            self.row_vector_bytes_upper,
            self.journal_vector_bytes_upper,
            self.dictionary_payload_bytes_upper,
            self.dictionary_container_bytes_upper,
            self.table_container_bytes_upper,
            self.writer_frame_bytes_upper,
        ]
        .into_iter()
        .try_fold(0, add)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pinned_layout_and_push_clear_growth_cover_retained_and_moved_buffers() {
        assert_eq!(size_of::<Value>(), 32);
        assert_eq!(size_of::<String>(), 24);
        let mut rows = Vec::<Value>::new();
        let mut labels = Vec::<String>::new();
        let mut high_water = 0;
        for _ in 0..3 {
            for n in 1..=BATCH_ROWS {
                high_water = high_water.max(n);
                let old_rows = rows.capacity();
                let old_labels = labels.capacity();
                rows.push(Value::Null);
                labels.push(String::new());
                // Only a growth operation needs old and new allocation owners.
                let slots = |old, new| if old == new { new } else { old + new };
                assert!(slots(old_rows, rows.capacity()) <= vector_slots(high_water).unwrap());
                assert!(slots(old_labels, labels.capacity()) <= vector_slots(high_water).unwrap());
            }
            rows.clear();
            labels.clear();
            assert!(rows.capacity() > 0 && labels.capacity() > 0);
        }
    }

    #[test]
    fn pinned_dictionary_growth_includes_both_hash_tables_and_label_vectors() {
        let mut labels = Vec::<String>::new();
        let mut ids = std::collections::HashMap::<String, usize>::new();
        for n in 1..=BATCH_ROWS {
            let label = format!("label {n} \"\\\n雪😀");
            let old_vec = labels.capacity();
            let old_buckets = ids.capacity().checked_next_power_of_two().unwrap();
            labels.push(label.clone());
            ids.insert(label, n);
            let new_buckets = ids.capacity().checked_next_power_of_two().unwrap();
            assert!(old_buckets + new_buckets <= 8 * n + 32);
            let slots = if old_vec == labels.capacity() {
                labels.capacity()
            } else {
                old_vec + labels.capacity()
            };
            assert!(slots <= vector_slots(n).unwrap());
        }
    }

    #[test]
    fn empty_rows_retain_fixed_containers_and_overflow_refuses() {
        let tables = BTreeMap::new();
        let (got, work) = inventory(&tables, &tables, 4, Shape::default()).unwrap();
        assert_eq!(work, 0);
        assert_eq!(got.row_tree_bytes_upper, 80);
        assert_eq!(got.row_vector_bytes_upper, 0);
        assert_eq!(got.journal_vector_bytes_upper, 0);
        assert!(got.total().unwrap() > 64 << 10);
        assert!(inventory(&tables, &tables, usize::MAX, Shape::default()).is_err());
        assert!(inventory(
            &tables,
            &tables,
            0,
            Shape {
                cells_upper: usize::MAX,
                ..Shape::default()
            }
        )
        .is_err());
        assert!(vector_slots(usize::MAX).is_err());
        // Every component fits, but the native checked total must still refuse.
        let (large, _) = inventory(&tables, &tables, usize::MAX / 20, Shape::default()).unwrap();
        assert!(large.total().is_err());
    }

    #[test]
    fn table_scan_work_is_paid_once_and_returned_for_the_unticketed_fallback() {
        let prefix = BTreeMap::from([
            ("journal".into(), TableInventory::default()),
            ("gross 雪".into(), TableInventory::default()),
        ]);
        let visible = BTreeMap::from([("path".into(), TableInventory::default())]);
        let expected = "journal".len() + "gross 雪".len() + "path".len() + 3;
        let (_, work) = inventory(&prefix, &visible, 4, Shape::default()).unwrap();
        assert_eq!(work, expected);
        super::super::cut_shape::with_budget(expected, || {
            let (_, work) = inventory(&prefix, &visible, 4, Shape::default())?;
            assert_eq!(work, expected);
            assert_eq!(super::super::cut_shape::spent(), Some(expected));
            Ok(())
        })
        .unwrap();
        assert!(super::super::cut_shape::with_budget(expected - 1, || {
            inventory(&prefix, &visible, 4, Shape::default())
        })
        .is_err());
    }
}
