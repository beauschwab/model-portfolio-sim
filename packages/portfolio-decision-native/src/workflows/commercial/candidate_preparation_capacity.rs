//! Construction census for a borrowed finite Compiled capability. This does
//! not substitute for Compiled's unchanged daily-runtime admission.
#[derive(Clone, Copy)]
pub(super) struct Input {
    pub groups: usize,
    pub bindings: usize,
    pub positions: usize,
    pub accounts: usize,
    pub scenario_label_bytes: usize,
    pub coefficient_cells: usize,
    pub graph_bytes: usize,
}
pub(super) struct Capacity {
    pub work: usize,
    pub bytes: usize,
}
fn add(a: usize, b: usize) -> Result<usize, String> {
    a.checked_add(b)
        .ok_or("borrowed preparation census overflow".into())
}
fn mul(a: usize, b: usize) -> Result<usize, String> {
    a.checked_mul(b)
        .ok_or("borrowed preparation census overflow".into())
}
pub(super) fn census(i: Input) -> Result<Capacity, String> {
    // Historical conservative reservation for opening, indexing and closing
    // scans. Compiled now caches a borrowed, numeric metadata order and merges
    // daily slices without sorting or String keys. Keep the former allowance
    // until the entire raw-derived source/Compiled/Sim/codec bound is proved;
    // the new metadata sort inventory alone grants no admission discount.
    let comparison = mul(usize::BITS as usize, add(i.scenario_label_bytes, 32)?)?;
    let scans = mul(i.groups, add(mul(comparison, 4)?, 128)?)?;
    let coverage = mul(i.bindings, add(add(i.positions, i.accounts)?, 128)?)?;
    let work = add(scans, add(coverage, mul(i.coefficient_cells, 128)?)?)?;
    // Retain the historical BTree/string/vector capacity allowance as well.
    // Bindings, sets and ownership maps are also retained.
    // Tracking books, posting buffers and Sim state belong to the separate
    // complete cut-component reservation, not to this preparation capability.
    let indices = mul(i.groups, add(mul(i.scenario_label_bytes, 2)?, 1024)?)?;
    let bindings = mul(add(i.bindings, i.positions)?, 4096)?;
    let coefficients = mul(i.coefficient_cells, 512)?;
    Ok(Capacity {
        work,
        bytes: add(i.graph_bytes, add(indices, add(bindings, coefficients)?)?)?,
    })
}

/// Finite metadata construction under the immutable event-index algorithm.
/// Financial source work, coefficient cells and retained byte allowances are
/// unchanged. The caller derives these dimensions from its complete raw input;
/// no request field can supply a work allowance or omit a zero-weight source.
pub(super) struct IndexedInput {
    pub legacy: Input,
    pub units: usize,
    pub scenarios: usize,
    pub maximum_identity_utf8_bytes: usize,
    pub specification_bytes: usize,
    pub fee_records: usize,
}

pub(super) fn indexed_census(i: IndexedInput) -> Result<Capacity, String> {
    use portfolio_ledger_native::commercial_intent_owner::commercial_event_sort_upper;
    let d = &i.legacy;
    let sort = commercial_event_sort_upper(d.groups)?;
    // Day/year/Loan-Fee/binding/group metadata comparisons and the explicit
    // merge loop/copy counters. Bounds include initial scratch copying.
    let sorting = add(
        mul(sort.comparison_upper, 8)?,
        mul(sort.metadata_copy_upper, 16)?,
    )?;
    // Opening/group/closing searches visit the complete source, including
    // groups belonging to other retained paths. No monetary pruning applies.
    let groups = mul(d.groups, 128)?;
    let labels = add(i.maximum_identity_utf8_bytes, 128)?;
    let identities = add(add(i.units, d.bindings)?, i.scenarios)?;
    // Every searched tree path visits no more than its total retained keys.
    // Four complete quadratic allowances cover raw owner/run/position maps,
    // Compiled unique/weight sets and node redistribution. Coverage additionally
    // searches the weight tree for every retained position/run pair.
    let trees = mul(
        add(
            mul(mul(identities, identities)?, 4)?,
            mul(mul(d.bindings, d.bindings)?, i.scenarios)?,
        )?,
        labels,
    )?;
    let coverage = mul(
        mul(
            add(d.bindings, i.units)?,
            add(add(d.positions, d.accounts)?, add(i.scenarios, 1)?)?,
        )?,
        labels,
    )?;
    let fields = mul(
        add(
            add(add(d.positions, d.accounts)?, i.units)?,
            add(d.bindings, i.scenarios)?,
        )?,
        128,
    )?;
    let fees = mul(i.fee_records, labels)?;
    let coefficients = mul(d.coefficient_cells, 128)?;
    let specification = mul(i.specification_bytes, 8)?;
    let work = [
        sorting,
        groups,
        trees,
        coverage,
        fields,
        fees,
        coefficients,
        specification,
        128,
    ]
    .into_iter()
    .try_fold(0, add)?;
    // No source/allocator discount: keep the complete former byte reservation.
    let legacy = census(i.legacy)?;
    Ok(Capacity {
        work,
        bytes: legacy.bytes,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    fn input() -> Input {
        Input {
            groups: 4,
            bindings: 2,
            positions: 1,
            accounts: 1,
            scenario_label_bytes: 8,
            coefficient_cells: 22,
            graph_bytes: 1000,
        }
    }
    #[test]
    fn census_has_independently_counted_scan_index_and_coefficient_terms() {
        let c = census(input()).unwrap();
        assert_eq!(c.work, 4 * (64 * 40 * 4 + 128) + 2 * 130 + 22 * 128);
        assert_eq!(c.bytes, 1000 + 4 * 1040 + 3 * 4096 + 22 * 512);
    }
    #[test]
    fn graph_retention_and_label_length_remain_additive() {
        let a = census(input()).unwrap();
        let mut i = input();
        i.graph_bytes += 700;
        assert_eq!(census(i).unwrap().bytes, a.bytes + 700);
        let mut i = input();
        i.scenario_label_bytes += 1;
        let b = census(i).unwrap();
        assert_eq!(b.work - a.work, 4 * 64 * 4);
        assert_eq!(b.bytes - a.bytes, 8);
    }
    #[test]
    fn impossible_dimensions_refuse_without_wrapping() {
        let mut i = input();
        i.groups = usize::MAX;
        assert!(census(i).is_err());
        let mut i = input();
        i.graph_bytes = usize::MAX;
        assert!(census(i).is_err());
    }

    fn indexed() -> IndexedInput {
        IndexedInput {
            legacy: input(),
            units: 1,
            scenarios: 2,
            maximum_identity_utf8_bytes: 19,
            specification_bytes: 301,
            fee_records: 3,
        }
    }
    #[test]
    fn indexed_terms_cover_numeric_sort_full_group_scans_trees_fields_and_specification() {
        let a = indexed();
        let legacy_bytes = census(a.legacy).unwrap().bytes;
        let got = indexed_census(a).unwrap();
        // Four entries need two merge passes, <=8 comparisons and12 copies.
        let sort = 8 * 8 + 12 * 16;
        let groups = 4 * 128;
        let trees = (4 * 5 * 5 + 2 * 2 * 2) * 147;
        let coverage = 3 * (1 + 1 + 2 + 1) * 147;
        let fields = (1 + 1 + 1 + 2 + 2) * 128;
        assert_eq!(
            got.work,
            sort + groups + trees + coverage + fields + 3 * 147 + 22 * 128 + 301 * 8 + 128
        );
        assert_eq!(got.bytes, legacy_bytes);
    }
    #[test]
    fn indexed_does_not_discount_empty_sort_metadata_or_financial_retention() {
        let mut a = indexed();
        a.legacy.groups = 0;
        let zero = indexed_census(a).unwrap();
        let mut a = indexed();
        a.legacy.groups = 1;
        let one = indexed_census(a).unwrap();
        assert_eq!(one.work - zero.work, 128);
        assert_eq!(one.bytes - zero.bytes, 1040);
        let mut a = indexed();
        a.legacy.graph_bytes += 701;
        assert_eq!(
            indexed_census(a).unwrap().bytes,
            census(input()).unwrap().bytes + 701
        );
    }
    #[test]
    fn indexed_multibyte_identity_specification_and_fee_dimensions_increase_work() {
        let base = indexed_census(indexed()).unwrap();
        for dimension in 0..6 {
            let mut a = indexed();
            match dimension {
                0 => a.maximum_identity_utf8_bytes += 4,
                1 => a.specification_bytes += 1,
                2 => a.fee_records += 1,
                3 => a.units += 1,
                4 => a.scenarios += 1,
                5 => a.legacy.bindings += 1,
                _ => unreachable!(),
            }
            assert!(
                indexed_census(a).unwrap().work > base.work,
                "dimension={dimension}"
            );
        }
    }
    #[test]
    fn indexed_every_unbounded_dimension_refuses_checked_overflow() {
        for dimension in 0..10 {
            let mut a = indexed();
            match dimension {
                0 => a.legacy.groups = usize::MAX,
                1 => a.legacy.bindings = usize::MAX,
                2 => a.legacy.positions = usize::MAX,
                3 => a.legacy.accounts = usize::MAX,
                4 => a.legacy.coefficient_cells = usize::MAX,
                5 => a.units = usize::MAX,
                6 => a.scenarios = usize::MAX,
                7 => a.maximum_identity_utf8_bytes = usize::MAX,
                8 => a.specification_bytes = usize::MAX,
                9 => a.fee_records = usize::MAX,
                _ => unreachable!(),
            }
            assert!(indexed_census(a).is_err(), "dimension={dimension}");
        }
    }
}
