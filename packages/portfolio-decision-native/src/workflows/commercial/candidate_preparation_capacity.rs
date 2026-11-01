//! Construction census for a borrowed finite Compiled capability. This does
//! not substitute for Compiled's unchanged daily-runtime admission.
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
}
