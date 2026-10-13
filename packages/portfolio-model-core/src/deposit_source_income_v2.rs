//! Counted performing tier. Source equations, original EIR and signed residues are shared.
use crate::{
    deposit_continuation as c, deposit_revised_income as reviewed, deposit_source_income as source,
    deposit_state as state,
};
use serde::Serialize;
pub use source::Request;
use std::mem::size_of;
pub const SCHEMA: &str = "deposit-source-income-2";
pub const CONTINUATION_SCHEMA: &str = "deposit-performing-continuation-2";
pub const OWNERSHIP: &str = "source_retained_mass_counted_fifo-2";
pub const MAX_DAYS: u32 = 365;
pub const MAX_WORK: usize = 8_000_000;
pub const MAX_BYTES: usize = 32 * 1024 * 1024;
#[derive(Debug, Clone, Serialize)]
pub struct TypedPlan {
    pub horizon_days: usize,
    pub point_slots: usize,
    pub interest_slots: usize,
    pub cash_slots: usize,
    pub atom_slots: usize,
    pub income_slots: usize,
    pub allocation_slots: usize,
    pub group_slots: usize,
    pub raw_bytes: usize,
    pub retained_reserved: usize,
    pub output_reserved: usize,
    pub reserved_bytes: usize,
    pub work_units: usize,
}
#[derive(Debug, Serialize)]
pub struct ResultData {
    pub schema: String,
    pub native: source::ResultData,
    pub typed_plan: TypedPlan,
    pub retained_used: usize,
    pub output_bytes: usize,
    pub charged_work_used: usize,
    pub installed_adoption: bool,
    pub empirical_validation: bool,
    pub capital_ftp_eligible: bool,
}
pub fn add(a: usize, b: usize) -> Result<usize, String> {
    a.checked_add(b)
        .ok_or_else(|| "counted tier addition overflow".into())
}
pub fn mul(a: usize, b: usize) -> Result<usize, String> {
    a.checked_mul(b)
        .ok_or_else(|| "counted tier multiplication overflow".into())
}
fn vector<T>(v: &Vec<T>) -> Result<usize, String> {
    mul(v.capacity(), size_of::<T>())
}
fn identity(i: &crate::dated_income::Identity) -> Result<usize, String> {
    add(i.id.capacity(), i.revision.capacity())
}
fn driver(d: &state::DepositDriver) -> Result<usize, String> {
    add(
        add(
            vector(&d.competitor_rates)?,
            vector(&d.offered_spread_stress)?,
        )?,
        add(vector(&d.confidence_stress)?, vector(&d.external_inflows)?)?,
    )
}
fn state_identity(i: &state::DepositIdentity) -> Result<usize, String> {
    [
        &i.entity_id,
        &i.cohort_id,
        &i.currency,
        &i.source_revision,
        &i.model_version,
        &i.assumption_id,
        &i.assumption_rationale,
        &i.run_id,
    ]
    .iter()
    .try_fold(0, |n, s| add(n, s.capacity()))
}
fn raw_state(r: &state::DepositStateRequest) -> Result<usize, String> {
    let mut n = add(
        size_of::<state::DepositStateRequest>(),
        add(r.schema.capacity(), state_identity(&r.identity)?)?,
    )?;
    for d in &r.drivers {
        n = add(n, driver(d)?)?;
    }
    n = add(
        n,
        add(
            vector(&r.drivers)?,
            add(vector(&r.segments)?, vector(&r.migrations)?)?,
        )?,
    )?;
    for s in &r.segments {
        n = add(
            n,
            add(
                s.segment_id.capacity(),
                vector(&s.interest_payment_ordinals)?,
            )?,
        )?;
        if let Some(o) = &s.repricing.supported_observation_range {
            n = add(n, o.history_revision.capacity())?;
        }
    }
    Ok(n)
}
fn raw_capacity(r: &Request) -> Result<usize, String> {
    let x = &r.continuation;
    let mut n = add(
        size_of::<Request>(),
        add(raw_state(&x.original)?, raw_state(&x.current)?)?,
    )?;
    for i in [
        &r.review,
        &x.contract,
        &x.anchor.purchase_quote,
        &x.anchor.original_yield_anchor,
        &x.policy.review,
        &x.prefix.identity,
    ] {
        n = add(n, identity(i)?)?;
    }
    for s in [
        &r.schema,
        &r.ownership,
        &x.schema,
        &x.policy.expectation_policy,
        &x.policy.pnl_policy,
        &x.policy.phase_policy,
        &x.policy.tail_policy,
    ] {
        n = add(n, s.capacity())?;
    }
    n = add(
        n,
        add(
            vector(&x.prefix.drivers)?,
            vector(&x.policy.changed_driver_dimensions)?,
        )?,
    )?;
    for d in &x.prefix.drivers {
        n = add(n, driver(d)?)?;
    }
    for s in &x.policy.changed_driver_dimensions {
        n = add(n, s.capacity())?;
    }
    Ok(n)
}
pub fn parse(b: &[u8]) -> Result<Request, String> {
    source::parse(b)
}
pub fn audit(
    r: &Request,
    check: &mut impl FnMut() -> Result<(), String>,
) -> Result<TypedPlan, String> {
    check()?;
    if r.schema != SCHEMA
        || r.ownership != OWNERSHIP
        || r.continuation.schema != CONTINUATION_SCHEMA
    {
        return Err("explicit counted source/continuation2 tier required".into());
    }
    let x = &r.continuation;
    let h = x.original.horizon_days as usize;
    for s in [&x.original, &x.current] {
        if s.horizon_days == 0
            || s.horizon_days > MAX_DAYS
            || s.segments.len() != 1
            || !s.migrations.is_empty()
            || s.drivers.len() > MAX_DAYS as usize + 65
            || s.segments[0].repricing.observation_lag_days > 64
            || s.segments[0].interest_payment_ordinals.len() > h + 1
        {
            return Err(
                "counted complete tier days<=365/one segment/lag<=64 shape required".into(),
            );
        }
        for d in &s.drivers {
            check()?;
            if [
                d.competitor_rates.len(),
                d.offered_spread_stress.len(),
                d.confidence_stress.len(),
                d.external_inflows.len(),
            ] != [1; 4]
            {
                return Err("one declared channel required".into());
            }
        }
    }
    if x.prefix.drivers.len() > h + 65 || x.policy.changed_driver_dimensions.len() > 4 {
        return Err("bounded complete prefix/dimensions required".into());
    }
    c::bounded_identity(&r.review)?;
    c::bounded_identity(&x.prefix.identity)?;
    if x.policy.expectation_policy != "later_date_original_yield_catchup-1"
        || x.policy.phase_policy != "all_date_recognition_settlement_then_revision-1"
    {
        return Err("explicit later-date original-yield policy required".into());
    }
    let end = x
        .original
        .as_of_ordinal
        .checked_add(i32::try_from(h).map_err(|_| "days overflow")?)
        .ok_or("date overflow")?;
    if x.revision_ordinal <= x.original.as_of_ordinal
        || x.revision_ordinal >= end
        || x.prefix.through_ordinal != x.revision_ordinal
        || x.original.segments[0]
            .maturity_ordinal
            .is_none_or(|d| d <= x.revision_ordinal)
    {
        return Err("revision before complete live maturity required".into());
    }
    let mut p = x.policy.clone();
    p.expectation_policy = "purchase_asof_original_yield_catchup-1".into();
    p.phase_policy = "original_opening_then_revision_then_daily_recognition_settlement-1".into();
    reviewed::validate_contracts(
        &reviewed::Request {
            schema: reviewed::SCHEMA.into(),
            contract: x.contract.clone(),
            anchor: x.anchor.clone(),
            policy: p,
            original: x.original.clone(),
            runs: vec![reviewed::RunSource {
                run_id: x.current.identity.run_id.clone(),
                scenario_id: "counted-current".into(),
                severity: 1.,
                revised: x.current.clone(),
            }],
        },
        MAX_DAYS,
    )?;
    let prefix: Vec<_> = x
        .original
        .drivers
        .iter()
        .filter(|d| d.ordinal <= x.revision_ordinal)
        .collect();
    if prefix.len() != x.prefix.drivers.len() {
        return Err("complete historical prefix absent".into());
    }
    for (old, p) in prefix.iter().zip(&x.prefix.drivers) {
        check()?;
        if !c::exact(*old, p)? {
            return Err("historical prefix differs".into());
        }
    }
    for (old, new) in x.original.drivers.iter().zip(&x.current.drivers) {
        check()?;
        if old.ordinal <= x.revision_ordinal && !c::exact(old, new)? {
            return Err("current changes history through revision".into());
        }
    }
    let raw = serialized_size(r, 1_048_576, check)?;
    let cash = add(mul(h, 3)?, 2)?;
    let atoms = add(h, 2)?;
    let allocations = add(h, cash)?;
    let groups = add(mul(h, 3)?, 3)?;
    let static_per_source = add(
        mul(add(h, 1)?, size_of::<state::DepositPoint>())?,
        add(
            mul(h, size_of::<state::DepositInterest>())?,
            add(
                mul(cash, size_of::<state::DepositCashEvent>())?,
                add(
                    mul(
                        atoms,
                        size_of::<crate::deposit_principal::PrincipalAtom>()
                            + size_of::<source::Transition>(),
                    )?,
                    add(
                        mul(
                            h,
                            size_of::<crate::dated_income::IncomeEvent>()
                                + size_of::<crate::dated_income::IncomeRow>()
                                + size_of::<c::Origin>(),
                        )?,
                        add(
                            mul(cash, size_of::<c::Settlement>())?,
                            mul(allocations, size_of::<c::Allocation>())?,
                        )?,
                    )?,
                )?,
            )?,
        )?,
    )?;
    let ids = add(
        add(x.contract.id.len(), x.contract.revision.len())?,
        add(x.policy.review.id.len(), x.policy.review.revision.len())?,
    )?;
    // Explicit typed record arenas doubled for Vec growth; metadata/dynamic fields and
    // transient indexed/expanded rights have their own conservative allowances.
    let groups_bytes = mul(
        groups,
        mul(
            2,
            add(size_of::<c::Group>(), mul(3, size_of::<c::Line>())?)?,
        )?,
    )?;
    let dynamic_bytes = mul(h, add(4096, mul(ids, 24)?)?)?;
    let retained = add(
        65_536,
        add(
            mul(raw, 12)?,
            add(
                mul(static_per_source, 4)?,
                add(groups_bytes, dynamic_bytes)?,
            )?,
        )?,
    )?;
    let output = add(
        65_536,
        add(mul(raw, 4)?, mul(h, add(12_000, mul(ids, 24)?)?)?)?,
    )?;
    let work = add(
        output,
        add(
            mul(raw, 8)?,
            mul(
                h,
                add(2048, mul(x.original.segments[0].segment_id.len() + 48, 48)?)?,
            )?,
        )?,
    )?;
    let bytes = add(retained, output)?;
    if work > MAX_WORK || bytes > MAX_BYTES {
        return Err("counted source joined8M/32MiB exceeded before projection".into());
    }
    if raw_capacity(r)? > retained {
        return Err("raw typed retained capacity exceeds reservation".into());
    }
    check()?;
    Ok(TypedPlan {
        horizon_days: h,
        point_slots: mul(add(h, 1)?, 2)?,
        interest_slots: mul(h, 2)?,
        cash_slots: mul(cash, 2)?,
        atom_slots: mul(atoms, 2)?,
        income_slots: mul(h, 2)?,
        allocation_slots: mul(allocations, 3)?,
        group_slots: groups,
        raw_bytes: raw,
        retained_reserved: retained,
        output_reserved: output,
        reserved_bytes: bytes,
        work_units: work,
    })
}
struct Counter<'a, F> {
    n: usize,
    cap: usize,
    check: &'a mut F,
}
impl<F: FnMut() -> Result<(), String>> std::io::Write for Counter<'_, F> {
    fn write(&mut self, b: &[u8]) -> std::io::Result<usize> {
        (self.check)().map_err(std::io::Error::other)?;
        self.n = self
            .n
            .checked_add(b.len())
            .ok_or_else(|| std::io::Error::other("output overflow"))?;
        if self.n > self.cap {
            return Err(std::io::Error::other("counted output exceeded reservation"));
        }
        Ok(b.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        (self.check)().map_err(std::io::Error::other)
    }
}
pub(crate) fn serialized_size<T: Serialize>(
    v: &T,
    cap: usize,
    check: &mut impl FnMut() -> Result<(), String>,
) -> Result<usize, String> {
    let mut w = Counter { n: 0, cap, check };
    serde_json::to_writer(&mut w, v).map_err(|e| e.to_string())?;
    Ok(w.n)
}
fn rights_capacity(r: &c::Rights) -> Result<usize, String> {
    let mut n = add(
        size_of::<c::Rights>(),
        add(vector(&r.origins)?, vector(&r.settlements)?)?,
    )?;
    for s in &r.settlements {
        n = add(
            n,
            add(s.native_event_id.capacity(), vector(&s.allocations)?)?,
        )?;
    }
    Ok(n)
}
fn owned_capacity(s: &source::SourceOwned) -> Result<usize, String> {
    let a = &s.source;
    let mut n = add(
        size_of::<source::SourceOwned>(),
        add(
            a.schema.capacity(),
            add(a.parameter_status.capacity(), state_identity(&a.identity)?)?,
        )?,
    )?;
    n = add(
        n,
        add(
            vector(&a.points)?,
            add(
                vector(&a.cashflows)?,
                add(vector(&a.interest)?, vector(&a.segment_ids)?)?,
            )?,
        )?,
    )?;
    for p in &a.points {
        n = add(
            n,
            add(
                add(vector(&p.balances)?, vector(&p.accrued_interest)?)?,
                add(
                    vector(&p.paid_rates)?,
                    add(
                        vector(&p.outside_supported_rate_regime)?,
                        vector(&p.rate_regime_assessed)?,
                    )?,
                )?,
            )?,
        )?;
    }
    for i in &a.interest {
        n = add(n, i.segment_id.capacity())?;
    }
    for c in &a.cashflows {
        n = add(
            n,
            add(
                c.event_id.capacity(),
                add(c.segment_id.capacity(), c.kind.capacity())?,
            )?,
        )?;
    }
    for id in &a.segment_ids {
        n = add(n, id.capacity())?;
    }
    n = add(
        n,
        add(
            vector(&s.atoms)?,
            add(
                vector(&s.transitions)?,
                add(vector(&s.income_request.events)?, vector(&s.income.rows)?)?,
            )?,
        )?,
    )?;
    for a in &s.atoms {
        n = add(
            n,
            add(
                a.segment_id.capacity(),
                add(a.kind.capacity(), vector(&a.cash_event_ids)?)?,
            )?,
        )?;
        for id in &a.cash_event_ids {
            n = add(n, id.capacity())?;
        }
    }
    for e in &s.income_request.events {
        n = add(n, add(e.event_id.capacity(), identity(&e.source)?)?)?;
        if let Some(crate::dated_income::YieldChange::CatchupOriginal {
            review,
            complete_future_cash,
        }) = &e.yield_change
        {
            n = add(n, add(identity(review)?, vector(complete_future_cash)?)?)?;
        }
    }
    for row in &s.income.rows {
        n = add(n, add(row.event_id.capacity(), identity(&row.source)?)?)?;
        if let Some(i) = &row.yield_review {
            n = add(n, identity(i)?)?;
        }
    }
    for i in [
        &s.income_request.contract,
        &s.income_request.policy,
        &s.income_request.original_yield_anchor,
        &s.income.contract,
        &s.income.policy,
        &s.income.original_yield_anchor,
    ] {
        n = add(n, identity(i)?)?;
    }
    for s in [
        &s.income_request.schema,
        &s.income_request.currency,
        &s.income_request.monetary_unit,
        &s.income.schema,
        &s.income.currency,
        &s.income.monetary_unit,
    ] {
        n = add(n, s.capacity())?;
    }
    add(n, rights_capacity(&s.rights)?)
}
pub fn evaluate(
    r: &Request,
    mut check: impl FnMut() -> Result<(), String>,
) -> Result<ResultData, String> {
    let plan = audit(r, &mut check)?;
    let a = source::Admission {
        work_units: plan.work_units,
        reserved_bytes: plan.output_reserved,
        raw_bytes: plan.raw_bytes,
        source_atom_slots: plan.atom_slots,
    };
    let native = source::evaluate_counted(r, a, &mut check)?;
    let mut retained = add(
        size_of::<ResultData>(),
        add(
            raw_capacity(r)?,
            add(
                owned_capacity(&native.original)?,
                owned_capacity(&native.current)?,
            )?,
        )?,
    )?;
    retained = add(
        retained,
        add(
            rights_capacity(&native.revision_rights)?,
            vector(&native.journal_deltas)?,
        )?,
    )?;
    for g in &native.journal_deltas {
        check()?;
        retained = add(
            retained,
            add(
                g.kind.capacity(),
                add(identity(&g.review)?, vector(&g.lines)?)?,
            )?,
        )?;
        for l in &g.lines {
            retained = add(retained, l.account.capacity())?;
        }
    }
    // BTreeMap nodes/keys: charge a conservative256bytes per retained entry plus strings.
    retained = add(retained, mul(native.journal_closing.len(), 256)?)?;
    for key in native.journal_closing.keys() {
        retained = add(retained, key.capacity())?;
    }
    retained = add(
        retained,
        add(native.schema.capacity(), raw_capacity(&native.raw_request)?)?,
    )?;
    let bytes = serialized_size(&native, plan.output_reserved, &mut check)?;
    let work = add(
        bytes,
        add(
            mul(plan.raw_bytes, 8)?,
            mul(
                plan.horizon_days,
                add(
                    2048,
                    mul(
                        r.continuation.original.segments[0].segment_id.len() + 48,
                        48,
                    )?,
                )?,
            )?,
        )?,
    )?;
    if retained > plan.retained_reserved || work > plan.work_units {
        return Err("actual typed source capacity/work exceeds reservation".into());
    }
    check()?;
    Ok(ResultData {
        schema: SCHEMA.into(),
        native,
        typed_plan: plan,
        retained_used: retained,
        output_bytes: bytes,
        charged_work_used: work,
        installed_adoption: false,
        empirical_validation: false,
        capital_ftp_eligible: false,
    })
}
