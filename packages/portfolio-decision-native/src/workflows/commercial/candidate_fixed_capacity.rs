//! Prospective candidate metadata and Value allocations. This complements the
//! complete source/graph/unit inventory; the cut Sim and transport are separate.
use super::*;
use std::mem::size_of;
#[derive(Serialize)]
pub(super) struct Capacity {
    pub units: units::FixedConstructionCapacity,
    pub metadata_bytes_upper: usize,
    pub total_bytes_upper: usize,
    pub portable_reservation_bytes: usize,
}
/// A platform-independent allowance calculated only from raw dimensions. The
/// exhaustive native inventory must fit this allowance before constructing a
/// graph; the independent verifier derives the same allowance from raw inputs.
/// A mismatch refuses, never discounts an owner or grants a larger limit.
pub(super) fn reservation(r: &Request) -> Result<usize, String> {
    let mut bytes = mul(strict_json::serialized_size(r, 2 << 20)?, 128)?;
    for u in &r.commercial.units {
        for p in &u.graph.positions {
            if p.exercise.is_some()
                || p.generated_policy.is_some()
                || !p.original_physical.is_empty()
            {
                return Err(
                    "fixed reservation requires separately inventoried advanced owners".into(),
                );
            }
            let originals = p.original_pricing.iter().map(|h| (h, false));
            let current = p.histories.iter().flat_map(|s| {
                s.physical.iter().map(|h| (h, true)).chain(
                    s.pricing
                        .iter()
                        .chain(&s.pricing_down)
                        .chain(&s.pricing_up)
                        .map(|h| (h, false)),
                )
            });
            for (h, physical) in originals.chain(current) {
                let prep = source::fixed_preparation_capacity(&h.source)?;
                let n = h.source.source.current.periods.len();
                let j = h.source.credit.max_jumps_per_path;
                let points = mul(
                    h.source.credit.paths.len(),
                    sum([prep.intervals_upper, mul(n, 2)?, mul(j, 2)?, 69])?,
                )?;
                let allowance = add(
                    mul(
                        mul(points, add(n, 1)?)?,
                        if physical { 16384 } else { 4096 },
                    )?,
                    65536,
                )?;
                if prep.construction_bytes_upper > allowance {
                    return Err(
                        "fixed preparation expansion exceeds its raw-dimensional allowance".into(),
                    );
                }
                bytes = add(bytes, allowance)?;
            }
            if let Some(fees) = &p.fees {
                for history in fees
                    .original_pricing
                    .iter()
                    .chain(fees.histories.iter().flat_map(|s| {
                        s.physical
                            .iter()
                            .chain(&s.pricing)
                            .chain(&s.pricing_down)
                            .chain(&s.pricing_up)
                    }))
                {
                    bytes = add(
                        bytes,
                        portfolio_risk_native::commercial_fee_source::fixed_capacity::reservation(
                            history,
                            fees.allocations.len(),
                        )?,
                    )?;
                }
            }
        }
    }
    if bytes > 128 << 20 {
        return Err("fixed raw-dimensional reservation exceeds128MiB".into());
    }
    Ok(bytes)
}
fn vector<T>(n: usize) -> Result<usize, String> {
    if n == 0 {
        Ok(0)
    } else {
        mul(add(mul(n, 3)?, 8)?, size_of::<T>())
    }
}
fn sum(v: impl IntoIterator<Item = usize>) -> Result<usize, String> {
    v.into_iter().try_fold(0, add)
}
/// One full pinned internal BTree node (744 bytes) per cell exceeds Value
/// object allocation, including temporary splits. Vec old/new growth needs
/// <=6 slots (192 bytes) per element including a one-element vector. Object
/// and sequence cells are disjoint. UTF-8 payload is counted independently.
fn json_storage(shape: daily::CutJsonShape) -> Result<usize, String> {
    add(mul(shape.cells_upper, 1024)?, shape.string_bytes_upper)
}
fn array_count(raw: &Value, name: &str) -> Result<usize, String> {
    raw.get(name).map_or(Ok(0), |v| {
        v.as_array()
            .map(Vec::len)
            .ok_or(format!("fixed specification {name} must be an array"))
    })
}
pub(super) fn derive(r: &Request) -> Result<Capacity, String> {
    let portable_reservation_bytes = reservation(r)?;
    let units = units::fixed_construction_capacity(&r.commercial)?;
    for field in [
        "netting_sets",
        "policies",
        "cashflows",
        "reverse_severities",
    ] {
        if array_count(&r.specification, field)? != 0 {
            return Err("fixed candidate requires exclusive commercial specification".into());
        }
    }
    let accounts = array_count(&r.specification, "accounts")?;
    let positions = array_count(&r.specification, "positions")?;
    let scenarios = array_count(&r.specification, "scenarios")?;
    if accounts > 50 || positions > 32 || scenarios > 8 {
        return Err("fixed candidate specification count exceeds existing component limits".into());
    }
    let rows = sum([accounts, positions, scenarios, 1])?;
    let n = r.commercial.units.len();
    let s = r.commercial.units[0].graph.scenarios.len();
    let h = r.commercial.reporting_ordinals.len() - 1;
    let coefficients = if r
        .commercial
        .units
        .iter()
        .any(|u| u.graph.positions.iter().any(|p| p.fees.is_some()))
    {
        11
    } else {
        10
    };
    let raw_shape = daily::cut_json_shape(r)?;
    let spec_shape = daily::cut_json_shape(&r.specification)?;
    // Normalization consumes a Value clone, then constructs typed defaulted
    // records. Each record has <=16 text fields, <=120 Unicode scalar values
    // after validation (<=480 UTF-8 bytes). Reserve String-to-Arc overlap too.
    // Defaulted numeric fields are within these complete typed row headers.
    let typed_spec = sum([
        size_of::<portfolio_ledger_native::types::Spec>(),
        vector::<portfolio_ledger_native::types::Account>(accounts)?,
        vector::<portfolio_ledger_native::types::Position>(positions)?,
        vector::<portfolio_ledger_native::types::Scenario>(scenarios)?,
        mul(rows, 16 * 2 * 512)?,
    ])?;
    // Resolved Spec: <=128 named scalar members per row. Keys and values
    // each contribute a cell; UTF-8 keys <=64 bytes, values <=512 bytes.
    let resolved = json_storage(daily::CutJsonShape {
        cells_upper: mul(rows, 257)?,
        string_bytes_upper: mul(rows, 128 * (64 + 512))?,
        encoded_bytes_upper: 0,
    })?;
    // Allocation replay retains one complete scalar cash object per funding
    // date/scenario (7 fields), 10*h coefficients per scenario, 8-field unit
    // identities, faces and fixed metadata. Include both numeric scratch
    // vectors and their Value cells; zero face still retains every source.
    let replay_cells = sum([
        mul(mul(s, units.funding_dates_upper)?, 15)?,
        mul(mul(s, h)?, coefficients)?,
        mul(n, 20)?,
        mul(s, 32)?,
        128,
    ])?;
    let replay = json_storage(daily::CutJsonShape {
        cells_upper: replay_cells,
        string_bytes_upper: sum([
            mul(mul(s, units.funding_dates_upper)?, 128)?,
            mul(n, 4 * 256)?,
            mul(s, 256)?,
            4096,
        ])?,
        encoded_bytes_upper: 0,
    })?;
    let metadata_bytes_upper = sum([
        // Raw candidate receipt plus the independently retained typed input's
        // nonfinancial fields/Value. Unit raw ownership is in the unit bound.
        json_storage(raw_shape)?,
        mul(json_storage(spec_shape)?, 2)?,
        typed_spec,
        resolved,
        replay,
        vector::<f64>(n)?,
        vector::<f64>(mul(h, 11)?)?,
        // Owner/run/reservation maps, input clone headers, validation sets and
        // preserved text. These have no nested source projections.
        mul(
            sum([
                r.owners.len(),
                r.allocation.len(),
                r.run_mapping.len(),
                rows,
            ])?,
            8192,
        )?,
    ])?;
    let total_bytes_upper = add(units.total_bytes_upper, metadata_bytes_upper)?;
    if total_bytes_upper > 128 << 20 {
        return Err("fixed candidate construction exceeds128MiB before generation".into());
    }
    if total_bytes_upper > portable_reservation_bytes {
        return Err(
            "complete fixed candidate inventory exceeds its raw-dimensional allowance".into(),
        );
    }
    Ok(Capacity {
        units,
        metadata_bytes_upper,
        total_bytes_upper,
        portable_reservation_bytes,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn original_fixed_candidate_inventory_keeps_complete_sources() {
        let raw: Value = serde_json::from_slice(include_bytes!(
            "../../../../../examples/models/commercial-cut-mixed-journal-1.json"
        ))
        .unwrap();
        let mut count = 0;
        for side in ["completed_candidate", "revised_candidate"] {
            let r: Request = serde_json::from_value(raw[side].clone()).unwrap();
            let b = derive(&r).unwrap();
            count += b.units.source_count;
            let (_, _, _, admission) = prepare_and_run(
                &r,
                false,
                None,
                Box::new(io::sink()),
                (0, 0),
                PreparationMode::AdmitFixedBorrowed,
                None,
            )
            .unwrap();
            let (prepared, _, _, actual) = prepare_and_run(
                &r,
                false,
                None,
                Box::new(io::sink()),
                (0, 0),
                PreparationMode::ExecuteFixedBorrowed,
                Some(&mut |_, _, _, _| Ok(())),
            )
            .unwrap();
            assert_eq!(admission.retained_bytes_upper, actual.retained_bytes_upper);
            assert_eq!(admission.work_upper, actual.work_upper);
            assert_eq!(
                prepared
                    .unwrap()
                    .native_preparation
                    .source()
                    .native_graphs
                    .len(),
                r.commercial.units.len()
            );
            eprintln!(
                "{side}: work={}, bytes={}, fixed={}",
                admission.work_upper, admission.retained_bytes_upper, b.total_bytes_upper
            );
        }
        assert_eq!(count, 27);
    }
}
