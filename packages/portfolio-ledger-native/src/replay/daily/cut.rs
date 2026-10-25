//! A native prefix moves its complete live Sim state into a resumed Sim.
//! No caller monetary checkpoint, time reset, second purchase or cloned ledger.
use super::*;
use crate::owners::commercial::intents::{Compiled, Scheduled};
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};

struct WindowWriter {
    enabled: Arc<AtomicBool>,
    output: Box<dyn Write + Send>,
    structural_bytes: Option<(usize, usize)>,
}
impl Write for WindowWriter {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        portfolio_compute_control::checkpoint().map_err(io::Error::other)?;
        if let Some((seen, limit)) = &mut self.structural_bytes {
            *seen = seen
                .checked_add(bytes.len())
                .ok_or_else(|| io::Error::other("selected cut all-frame byte overflow"))?;
            if *seen > *limit {
                return Err(io::Error::other(
                    "selected cut prefix/visible complete frames exceed pre-Sim inventory",
                ));
            }
        }
        if self.enabled.load(Ordering::Relaxed) {
            self.output.write(bytes)
        } else {
            Ok(bytes.len())
        }
    }
    fn flush(&mut self) -> io::Result<()> {
        self.output.flush()
    }
}

/// Private, nonserializable capability. Moving the actual Sim retains every
/// runtime claim, policy counter, book, gross right, GL balance and source clock.
struct CutCheckpoint<'a> {
    sim: Sim<'a>,
    day: usize,
    previously_sent_dictionary: usize,
}

fn equal<T: serde::Serialize>(a: &T, b: &T) -> R<()> {
    portfolio_compute_control::checkpoint()?;
    let comparison_bytes = portfolio_model_core::strict_json::serialized_size(a, 32 << 20)?
        .checked_add(portfolio_model_core::strict_json::serialized_size(
            b,
            32 << 20,
        )?)
        .ok_or("native cut comparison capacity overflow")?;
    if comparison_bytes > 32 << 20 {
        return Err("native cut comparison exceeds component capacity".into());
    }
    if portfolio_model_core::strict_json::encode(a, 32 << 20)?
        != portfolio_model_core::strict_json::encode(b, 32 << 20)?
    {
        return Err("native cut immutable specification or observed group differs".into());
    }
    Ok(())
}

/// The immutable specification with the original position inventory substituted.
/// All nested inputs remain borrowed; their own serializers retain every field.
/// Exhaustive destructuring makes a new Spec field a compile-time review gate.
#[derive(serde::Serialize)]
struct RevisionSpec<'a> {
    horizon_days: usize,
    accounts: &'a [Account],
    positions: &'a [Position],
    netting_sets: &'a [NettingSet],
    policies: &'a [Policy],
    scenarios: &'a [Scenario],
    reverse_severities: &'a [f64],
    cashflows: &'a [Cashflow],
    ruleset: &'a str,
}
fn revision_spec<'a>(spec: &'a Spec, retained: &'a [Position]) -> RevisionSpec<'a> {
    let Spec {
        horizon_days,
        accounts,
        positions: _,
        netting_sets,
        policies,
        scenarios,
        reverse_severities,
        cashflows,
        ruleset,
    } = spec;
    RevisionSpec {
        horizon_days: *horizon_days,
        accounts,
        positions: retained,
        netting_sets,
        policies,
        scenarios,
        reverse_severities,
        cashflows,
        ruleset,
    }
}

fn equal_revision_specifications(old: &Spec, new: &Spec) -> R<()> {
    let retained = new
        .positions
        .get(..old.positions.len())
        .ok_or("native cut requires complete surviving ownership and a future calendar")?;
    equal(
        &revision_spec(old, &old.positions),
        &revision_spec(new, &old.positions),
    )?;
    equal(&old.positions.as_slice(), &retained)
}

fn validate_revision(
    old: &Spec,
    new: &Spec,
    a: &Compiled<'_>,
    b: &Compiled<'_>,
    cut: usize,
) -> R<()> {
    if cut >= old.horizon_days
        || new.positions.len() < old.positions.len()
        || a.position_owners.len() != old.positions.len()
        || b.position_owners.len() != new.positions.len()
        || a.bindings.len() > b.bindings.len()
        || a.fees.len() > b.fees.len()
    {
        return Err(
            "native cut requires complete surviving ownership and a future calendar".into(),
        );
    }
    equal_revision_specifications(old, new)?;
    if a.position_owners != b.position_owners[..old.positions.len()] {
        return Err("native cut existing position ownership changed".into());
    }
    for (i, p) in new.positions.iter().enumerate().skip(old.positions.len()) {
        if !b.position_owners[i]
            || p.classification != "ac"
            || [
                p.balance,
                p.book_adjustment,
                p.opening_accrued,
                p.allowance,
                p.commitment,
            ]
            .iter()
            .any(|x| *x != 0.)
            || !p.collateral_position.is_empty()
        {
            return Err("native cut new candidate requires exclusive future AC ownership and zero opening money".into());
        }
    }
    for (left, right) in a.bindings.iter().zip(&b.bindings) {
        equal(
            &(
                left.scenario,
                left.position_id,
                left.path_key,
                left.origin_day,
                left.source_class.identity(),
                left.source_weight,
                left.allocated_face,
                left.native_unit_face,
            ),
            &(
                right.scenario,
                right.position_id,
                right.path_key,
                right.origin_day,
                right.source_class.identity(),
                right.source_weight,
                right.allocated_face,
                right.native_unit_face,
            ),
        )?;
    }
    if b.bindings[a.bindings.len()..]
        .iter()
        .any(|x| x.origin_day <= cut)
    {
        return Err("native cut new source predates the verified cut".into());
    }
    for (left, right) in a.fees.iter().zip(&b.fees) {
        equal(
            &(left.loan_binding, left.native_position_id),
            &(right.loan_binding, right.native_position_id),
        )?;
    }
    for scenario in std::iter::once("baseline").chain(old.scenarios.iter().map(|s| s.name.as_str()))
    {
        for day in 0..=cut {
            portfolio_compute_control::checkpoint()?;
            let left = a.ordered_events(scenario, day);
            let right = b.ordered_events(scenario, day);
            if left.len() != right.len() {
                return Err("native cut observed event coverage changed".into());
            }
            for (left, right) in left.iter().zip(&right) {
                match (*left, *right) {
                    (Scheduled::Loan(x, i), Scheduled::Loan(y, j)) if x == y && i == j => equal(
                        &*a.bindings[x].accounting.group(i),
                        &*b.bindings[y].accounting.group(j),
                    )?,
                    (Scheduled::Fee(x, i), Scheduled::Fee(y, j)) if x == y && i == j => {
                        equal(&a.fees[x].native_years[i], &b.fees[y].native_years[j])?;
                        equal(&*a.fees[x].source.group(i), &*b.fees[y].source.group(j))?;
                    }
                    _ => return Err("native cut ordered source ownership changed".into()),
                }
            }
        }
    }
    Ok(())
}

impl<'a> CutCheckpoint<'a> {
    fn capture(mut sim: Sim<'a>, day: usize) -> R<Self> {
        let previously_sent_dictionary = sim.sink.sent;
        sim.opening()?;
        sim.run_range(1, day)?;
        // Close-day reconciliation has completed. Flush while output is muted.
        sim.sink.flush()?;
        Ok(Self {
            sim,
            day,
            previously_sent_dictionary,
        })
    }
    fn resume(mut self, spec: &'a Spec, source: &'a Compiled<'a>) -> R<Sim<'a>> {
        let old_count = self.sim.positions.len();
        for p in spec.positions.iter().skip(old_count) {
            let index = self.sim.positions.len();
            self.sim.pid.insert(p.id.to_string(), index);
            self.sim.positions.push(p.clone());
            self.sim.market.push(p.opening_market_price);
            self.sim.marks.push(0.);
            self.sim.accrued.push(0.);
            self.sim.commitments.push(0.);
            self.sim.schedule_balance.push(0.);
            self.sim.htm_sold.push(0.);
            self.sim.claims_by_position.push(vec![]);
            self.sim.opening_links.push(vec![]);
        }
        self.sim
            .commercial_state
            .as_mut()
            .ok_or("native cut owner state absent")?
            .extend_cut(source)?;
        // Build a distinct Sim by moving every live field, never reconstructing
        // money from JSON or replaying acquisition. Original calendar persists.
        let mut resumed = Sim {
            spec,
            commercials: Some(source),
            ..self.sim
        };
        // Muted prefix flushes did not publish their dictionary additions.
        resumed.sink.sent = self.previously_sent_dictionary;
        resumed
            .sink
            .reports
            .begin_cut_window(&resumed.scenario.name);
        let mut keys = resumed.gl.keys().copied().collect::<Vec<_>>();
        keys.sort_by(|x, y| {
            (
                &resumed.accounts[x.0].id,
                &resumed.sink.dictionary[x.1],
                &resumed.sink.dictionary[x.2],
            )
                .cmp(&(
                    &resumed.accounts[y.0].id,
                    &resumed.sink.dictionary[y.1],
                    &resumed.sink.dictionary[y.2],
                ))
        });
        for (a, g, i) in keys {
            let gl = resumed.sink.dictionary[g].clone();
            let instrument = resumed.sink.dictionary[i].clone();
            resumed.sink.journal(
                0,
                &resumed.scenario.name,
                self.day,
                &resumed.accounts[a].id,
                "native_verified_cut_opening",
                &gl,
                &instrument,
                resumed.gl[&(a, g, i)],
            )?;
        }
        resumed.close_day(self.day)?;
        Ok(resumed)
    }
}

/// Explicit component route. The coordinator must additionally admit both raw
/// source graphs, retain original anchors and verify persisted publication.
/// Protocol8 is distinct from historical full-origin commercial protocols6/7.
#[derive(Clone, Copy, Debug, serde::Serialize)]
pub struct CommercialCutAdmission {
    pub work_upper: usize,
    pub retained_bytes_upper: usize,
    pub specification_bytes: usize,
    pub original_compiled_work_upper: usize,
    pub future_compiled_work_upper: usize,
    pub original_compiled_bytes_upper: usize,
    pub future_compiled_bytes_upper: usize,
    pub comparison_work_upper: usize,
}

/// The same unchanged guard is used by both pre-Sim coordinator admission and
/// actual execution. The native owners are genuine borrowed capabilities.
pub fn commercial_cut_census(
    completed: &Spec,
    revised: &Spec,
    original: &Compiled<'_>,
    future: &Compiled<'_>,
) -> R<CommercialCutAdmission> {
    crate::validation::specification::validate(completed, 60000, 90_000_000)?;
    crate::validation::specification::validate(revised, 60000, 90_000_000)?;
    if !completed.reverse_severities.is_empty() || !revised.reverse_severities.is_empty() {
        return Err("commercial reverse scenarios require explicit native path sources".into());
    }
    // Count both typed specifications before any Value clone or byte encoding.
    let raw_bytes = portfolio_model_core::strict_json::serialized_size(completed, 2 << 20)?
        .checked_add(portfolio_model_core::strict_json::serialized_size(
            revised,
            2 << 20,
        )?)
        .ok_or("native cut raw capacity overflow")?;
    // Canonical prefix comparisons scan strings and signed monetary endpoints,
    // separately from numeric posting work. Retained source bounds include the
    // source groups; charge size scans, encoding and equality separately.
    let comparison_work = original
        .retained_bytes_upper
        .checked_add(future.retained_bytes_upper)
        .and_then(|n| n.checked_mul(8))
        .ok_or("native cut comparison work overflow")?;
    let work = original
        .work_upper
        .checked_add(future.work_upper)
        .and_then(|n| n.checked_add(comparison_work))
        .and_then(|n| n.checked_add(raw_bytes.checked_mul(8)?))
        .ok_or("native cut work overflow")?;
    // Retained owners coexist with canonical comparison buffers. Reserve a
    // second complete source bound rather than relying on allocator reuse.
    let bytes = original
        .retained_bytes_upper
        .checked_add(future.retained_bytes_upper)
        .and_then(|n| n.checked_mul(2))
        .and_then(|n| n.checked_add(revised.positions.len().checked_mul(4096)?))
        .and_then(|n| n.checked_add(raw_bytes.checked_mul(20)?))
        .ok_or("native cut capacity overflow")?;
    Ok(CommercialCutAdmission {
        work_upper: work,
        retained_bytes_upper: bytes,
        specification_bytes: raw_bytes,
        original_compiled_work_upper: original.work_upper,
        future_compiled_work_upper: future.work_upper,
        original_compiled_bytes_upper: original.retained_bytes_upper,
        future_compiled_bytes_upper: future.retained_bytes_upper,
        comparison_work_upper: comparison_work,
    })
}

/// The structural census alone grants no execution authority. This unchanged
/// component guard is also called independently by every actual cut runner.
pub fn admit_commercial_cut(
    completed: &Spec,
    revised: &Spec,
    original: &Compiled<'_>,
    future: &Compiled<'_>,
) -> R<CommercialCutAdmission> {
    let census = commercial_cut_census(completed, revised, original, future)?;
    if census.work_upper > 8_000_000 || census.retained_bytes_upper > 32 << 20 {
        return Err("native cut ledger component exceeds8M/32MiB".into());
    }
    Ok(census)
}

pub fn run_commercial_cut(
    completed: &Spec,
    revised: &Spec,
    original: &Compiled<'_>,
    future: &Compiled<'_>,
    cut_day: usize,
    output: Box<dyn Write + Send>,
) -> R<()> {
    run_commercial_cut_inner(completed, revised, original, future, cut_day, output, None)
}

/// An additional complete-frame bound counts muted prefix serialization too.
/// It cannot grant authority or bypass the unchanged independent cut guard.
pub fn run_commercial_cut_with_inventory(
    completed: &Spec,
    revised: &Spec,
    original: &Compiled<'_>,
    future: &Compiled<'_>,
    cut_day: usize,
    output: Box<dyn Write + Send>,
    all_frame_bytes_upper: usize,
) -> R<()> {
    run_commercial_cut_inner(
        completed,
        revised,
        original,
        future,
        cut_day,
        output,
        Some(all_frame_bytes_upper),
    )
}

fn run_commercial_cut_inner(
    completed: &Spec,
    revised: &Spec,
    original: &Compiled<'_>,
    future: &Compiled<'_>,
    cut_day: usize,
    output: Box<dyn Write + Send>,
    all_frame_bytes_upper: Option<usize>,
) -> R<()> {
    admit_commercial_cut(completed, revised, original, future)?;
    validate_revision(completed, revised, original, future, cut_day)?;
    let enabled = Arc::new(AtomicBool::new(false));
    let mut sink = Sink::new(Box::new(WindowWriter {
        enabled: enabled.clone(),
        output,
        structural_bytes: all_frame_bytes_upper.map(|limit| (0, limit)),
    }));
    let baseline = Scenario {
        name: "baseline".into(),
        start_day: 1,
        rate_shift: 0.,
        spread_shift: 0.,
        deposit_flight: 0.,
        pd_multiplier: 1.,
        migration_multiplier: 1.,
        lgd_add: 0.,
        draw_multiplier: 0.,
        rollover_loss: 0.,
        haircut_add: 0.,
        market_shock: 0.,
        outage_days: 0,
    };
    for scenario in std::iter::once(&baseline).chain(completed.scenarios.iter()) {
        enabled.store(false, Ordering::Relaxed);
        let sim = Sim::new(
            completed,
            scenario.clone(),
            1.,
            true,
            &mut sink,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            Some(original),
        )?;
        let checkpoint = CutCheckpoint::capture(sim, cut_day)?;
        enabled.store(true, Ordering::Relaxed);
        let mut resumed = checkpoint.resume(revised, future)?;
        resumed.run_range(cut_day + 1, revised.horizon_days)?;
        resumed.finish()?;
        resumed.sink.flush()?;
    }
    for row in sink.reports.attribution(&revised.scenarios) {
        sink.emit("attribution", row)?;
    }
    sink.flush()?;
    serde_json::to_writer(
        &mut sink.out,
        &json!({"complete":true,"protocol":8,
        "cut_day":cut_day,"native_cut_state_moved":true,"original_calendar_retained":true,
        "new_sim_started_at_cut":true,"prior_publication_verified":false}),
    )
    .map_err(|e| e.to_string())?;
    sink.out.write_all(b"\n").map_err(|e| e.to_string())?;
    sink.out.flush().map_err(|e| e.to_string())?;
    portfolio_compute_control::checkpoint()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::owners::commercial::intents::{AccountingSource, Binding, SourceClass};
    use portfolio_model_core::commercial_coupon_accounting::{AtomicGroup, OwnedPosting, Snapshot};
    use portfolio_model_core::credit_accounting::{AccountCode as A, Identity, PostingIntent};
    use std::{borrow::Cow, sync::Mutex};
    struct HandSource(Vec<AtomicGroup>);
    impl crate::owners::commercial::intents::Sealed for HandSource {}
    impl AccountingSource for HandSource {
        fn group_count(&self) -> usize {
            self.0.len()
        }
        fn group(&self, i: usize) -> Cow<'_, AtomicGroup> {
            Cow::Borrowed(&self.0[i])
        }
        fn has_closing(&self, key: u64) -> bool {
            self.0.last().is_some_and(|g| g.path_key == key)
        }
    }
    fn source(key: u64) -> HandSource {
        let snapshot = |year, principal, basis| Snapshot {
            path_key: key,
            year,
            principal,
            recognized_interest: 0.,
            legal_interest: 0.,
            native_legal_reduction_difference: 0.,
            book_adjustment: basis,
            gross_carry: principal + basis,
            in_default: false,
            reviewed_credit_state: None,
            lots: vec![],
            pending_coupon_obligation_indices: vec![],
            canonical_legal_coupons: vec![],
        };
        let group = |year, phase, p, b, cash, interest, entries: Vec<(A, f64)>| AtomicGroup {
            path_key: key,
            year,
            phase,
            policy: Identity {
                id: "hand-component-control".into(),
                revision: "1".into(),
            },
            source_event_id: None,
            effective_interest: interest,
            basis_realization: 0.,
            cash,
            postings: entries
                .into_iter()
                .map(|(account, amount)| OwnedPosting {
                    coupon_id: None,
                    earning_origin_index: None,
                    posting: PostingIntent { account, amount },
                })
                .collect(),
            next: snapshot(year, p, b),
        };
        HandSource(vec![
            group(
                0.,
                "reviewed_opening_snapshot",
                100.,
                -1.,
                -99.,
                0.,
                vec![
                    (A::LivePrincipal, 100.),
                    (A::LiveBookAdjustment, -1.),
                    (A::Cash, -99.),
                ],
            ),
            group(
                10. / 365.,
                "recognition",
                100.,
                0.,
                0.,
                1.,
                vec![(A::LiveBookAdjustment, 1.), (A::InterestIncome, -1.)],
            ),
            group(
                20. / 365.,
                "repayment",
                0.,
                0.,
                100.,
                0.,
                vec![(A::LivePrincipal, -100.), (A::Cash, 100.)],
            ),
        ])
    }
    fn spec() -> Spec {
        let raw: Value = serde_json::from_str(include_str!(
            "../../../../../examples/models/commercial-candidate-journal-1.json"
        ))
        .unwrap();
        let raw = raw["specification"].clone();

        crate::spec::normalize(raw, 512, 8_000_000).unwrap()
    }

    // Keep the pre-change Value/clone comparison as an independent test oracle.
    fn previous_revision_comparison(old: &Spec, new: &Spec) -> R<()> {
        let old_json = serde_json::to_value(old).unwrap();
        let mut new_json = serde_json::to_value(new).unwrap();
        new_json["positions"] = old_json["positions"].clone();
        equal(&old_json, &new_json)?;
        equal(
            &old.positions,
            &new.positions[..old.positions.len()].to_vec(),
        )
    }

    fn complete_spec() -> Spec {
        let mut raw = serde_json::to_value(spec()).unwrap();
        raw["netting_sets"] = json!([{
            "id": "netting", "account": "bank", "counterparty": "cp"
        }]);
        raw["policies"] = json!([{
            "id": "policy", "account": "bank", "kind": "funding",
            "trigger_cash": 1., "limit": 2.
        }]);
        raw["cashflows"] = json!([{
            "position": "loan", "day": 1, "principal": 3.,
            "cash_interest": -0., "accrual_interest": 4., "book_amortization": -5.
        }]);
        raw["reverse_severities"] = json!([0., 0.5, 1.]);
        raw["scenarios"] = json!([{"name": "stress", "rate_shift": -0.01}]);
        serde_json::from_value(raw).unwrap()
    }

    fn scalar_paths(value: &Value, path: &str, paths: &mut Vec<String>) {
        match value {
            Value::Object(fields) => {
                for (name, child) in fields {
                    scalar_paths(child, &format!("{path}/{name}"), paths);
                }
            }
            Value::Array(values) => {
                for (index, child) in values.iter().enumerate() {
                    scalar_paths(child, &format!("{path}/{index}"), paths);
                }
            }
            _ => paths.push(path.to_owned()),
        }
    }

    #[test]
    fn borrowed_revision_matches_previous_comparison_for_every_serialized_scalar() {
        let original = complete_spec();
        let raw = serde_json::to_value(&original).unwrap();
        let mut paths = Vec::new();
        scalar_paths(&raw, "", &mut paths);
        assert!(paths.len() >= 100, "exercise all populated Spec families");
        eprintln!(
            "borrowed revision: {} scalar mutation controls",
            paths.len()
        );
        for path in paths {
            let mut changed = raw.clone();
            let cell = changed.pointer_mut(&path).unwrap();
            *cell = match &*cell {
                Value::String(text) => json!(format!("{text}\"\\\n\u{0}雪")),
                Value::Bool(flag) => json!(!flag),
                Value::Number(n) if n.is_u64() => json!(n.as_u64().unwrap() + 1),
                Value::Number(n) => json!(n.as_f64().unwrap() + 0.125),
                other => panic!("unexpected scalar: {other}"),
            };
            let changed: Spec = serde_json::from_value(changed).unwrap();
            let previous = previous_revision_comparison(&original, &changed);
            let borrowed = equal_revision_specifications(&original, &changed);
            assert!(previous.is_err(), "previous comparison must reject {path}");
            assert_eq!(borrowed, previous, "mutation {path}");
        }
    }

    #[test]
    fn borrowed_revision_preserves_full_wire_content_and_borrows_all_containers() {
        let original = complete_spec();
        let mut revised = original.clone();
        revised.positions.push(original.positions[0].clone());
        let view = revision_spec(&revised, &original.positions);
        let mut previous = serde_json::to_value(&revised).unwrap();
        previous["positions"] = serde_json::to_value(&original.positions).unwrap();
        assert_eq!(serde_json::to_value(&view).unwrap(), previous);
        assert!(std::ptr::eq(view.accounts, revised.accounts.as_slice()));
        assert!(std::ptr::eq(view.positions, original.positions.as_slice()));
        assert!(std::ptr::eq(
            view.netting_sets,
            revised.netting_sets.as_slice()
        ));
        assert!(std::ptr::eq(view.policies, revised.policies.as_slice()));
        assert!(std::ptr::eq(view.scenarios, revised.scenarios.as_slice()));
        assert!(std::ptr::eq(
            view.reverse_severities,
            revised.reverse_severities.as_slice()
        ));
        assert!(std::ptr::eq(view.cashflows, revised.cashflows.as_slice()));
        assert!(std::ptr::eq(view.ruleset, revised.ruleset.as_str()));
        assert_eq!(equal_revision_specifications(&original, &revised), Ok(()));
        assert_eq!(previous_revision_comparison(&original, &revised), Ok(()));
        revised.positions.clear();
        assert!(equal_revision_specifications(&original, &revised).is_err());
    }

    #[test]
    fn borrowed_revision_retains_signed_zero_extreme_numbers_capacity_and_cancellation() {
        let mut original = complete_spec();
        for value in [-0., f64::MAX, f64::MIN_POSITIVE, f64::from_bits(1), -1e-200] {
            original.accounts[0].cash = value;
            assert_eq!(equal_revision_specifications(&original, &original), Ok(()));
            assert_eq!(previous_revision_comparison(&original, &original), Ok(()));
            let view = revision_spec(&original, &original.positions);
            assert_eq!(
                serde_json::to_value(view).unwrap(),
                serde_json::to_value(&original).unwrap()
            );
        }
        original.accounts[0].cash = 0.;
        let mut changed = original.clone();
        changed.accounts[0].cash = -0.;
        assert_eq!(
            equal_revision_specifications(&original, &changed),
            previous_revision_comparison(&original, &changed)
        );
        assert!(equal_revision_specifications(&original, &changed).is_err());
        original.ruleset = "r".repeat((16 << 20) + 1);
        let previous = previous_revision_comparison(&original, &original);
        assert!(previous
            .as_ref()
            .unwrap_err()
            .contains("component capacity"));
        assert_eq!(
            equal_revision_specifications(&original, &original),
            previous
        );
        let control = portfolio_compute_control::Control::new(10_000).unwrap();
        let cancelled = portfolio_compute_control::scope(Some(control.clone()), || {
            control.cancel();
            equal_revision_specifications(&original, &original)
        });
        assert_eq!(cancelled.unwrap_err(), "COMPUTE_CANCELLED");
    }

    #[test]
    fn borrowed_revision_preserves_empty_inputs_array_order_and_container_changes() {
        let empty: Spec = serde_json::from_value(json!({})).unwrap();
        assert_eq!(equal_revision_specifications(&empty, &empty), Ok(()));
        assert_eq!(previous_revision_comparison(&empty, &empty), Ok(()));
        let original = complete_spec();
        let raw = serde_json::to_value(&original).unwrap();
        for (field, values) in raw.as_object().unwrap() {
            let Some(values) = values.as_array() else {
                continue;
            };
            assert!(!values.is_empty(), "populate {field} before mutating it");
            let mut changed = raw.clone();
            changed[field]
                .as_array_mut()
                .unwrap()
                .push(values[0].clone());
            let revised: Spec = serde_json::from_value(changed).unwrap();
            let previous = previous_revision_comparison(&original, &revised);
            assert_eq!(previous.is_ok(), field == "positions", "{field}");
            assert_eq!(equal_revision_specifications(&original, &revised), previous);
        }
        let mut revised = original.clone();
        revised.reverse_severities.reverse();
        assert!(equal_revision_specifications(&original, &revised).is_err());
        assert_eq!(
            equal_revision_specifications(&original, &revised),
            previous_revision_comparison(&original, &revised)
        );
    }

    fn binding<'a>(source: &'a HandSource, id: &'a str, key: u64, origin: usize) -> Binding<'a> {
        Binding {
            source_class: SourceClass::CommercialLoan,
            scenario: "baseline",
            position_id: id,
            path_key: key,
            origin_day: origin,
            source_weight: 1.,
            allocated_face: 100.,
            native_unit_face: 100.,
            accounting: source,
        }
    }
    fn bindings<'a>(
        source: &'a HandSource,
        id: &'a str,
        key: u64,
        origin: usize,
    ) -> Vec<Binding<'a>> {
        let first = binding(source, id, key, origin);
        let mut second = binding(source, id, key, origin);
        second.scenario = "stress";
        vec![first, second]
    }
    #[derive(Clone)]
    struct Buffer(Arc<Mutex<Vec<u8>>>);
    impl Write for Buffer {
        fn write(&mut self, v: &[u8]) -> io::Result<usize> {
            self.0.lock().unwrap().extend(v);
            Ok(v.len())
        }
        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }
    fn tables(buffer: &Buffer) -> BTreeMap<String, Vec<Value>> {
        let data = buffer.0.lock().unwrap();
        let mut dictionary = Vec::<String>::new();
        let mut tables = BTreeMap::<String, Vec<Value>>::new();
        for line in data.split(|b| *b == b'\n').filter(|l| !l.is_empty()) {
            let row: Value = serde_json::from_slice(line).unwrap();
            let Some(name) = row["table"].as_str() else {
                continue;
            };
            let out = tables.entry(name.into()).or_default();
            if name == "journal" {
                dictionary.extend(
                    serde_json::from_value::<Vec<String>>(row["dictionary"].clone()).unwrap(),
                );
                let columns = row["columns"].as_object().unwrap();
                for i in 0..columns["value"].as_array().unwrap().len() {
                    let mut result = serde_json::Map::new();
                    for (key, values) in columns {
                        let value = if matches!(
                            key.as_str(),
                            "scenario" | "account" | "event" | "gl_account" | "instrument_id"
                        ) {
                            json!(dictionary[values[i].as_u64().unwrap() as usize])
                        } else {
                            values[i].clone()
                        };
                        result.insert(key.clone(), value);
                    }
                    out.push(Value::Object(result));
                }
            } else {
                out.extend(row["rows"].as_array().unwrap().iter().cloned());
            }
        }
        tables
    }
    fn buffer() -> Buffer {
        Buffer(Arc::default())
    }
    #[test]
    fn real_cut_state_preserves_future_journal_and_full_closing_at_multiple_cuts() {
        let s = spec();
        let owner = source(17);
        let id = s.positions[0].id.as_str();
        let c = Compiled::new(&s, bindings(&owner, id, 17, 0)).unwrap();
        let full = buffer();
        super::super::run_with_commercial(&s, None, &c, Box::new(full.clone())).unwrap();
        let full = tables(&full);
        for cut in [0, 5, 10, 20] {
            let window = buffer();
            run_commercial_cut(&s, &s, &c, &c, cut, Box::new(window.clone())).unwrap();
            let got = tables(&window);
            assert_eq!(got["closing_statements"], full["closing_statements"]);
            assert_eq!(got["trial_balance"], full["trial_balance"]);
            let future = |rows: &Vec<Value>| {
                rows.iter()
                    .filter(|r| r["day"].as_u64().unwrap() > cut as u64)
                    .cloned()
                    .collect::<Vec<_>>()
            };
            assert_eq!(future(&got["journal"]), future(&full["journal"]));
            assert!(got["journal"]
                .iter()
                .any(|r| r["event"] == "native_verified_cut_opening"));
            assert!(got["path"]
                .iter()
                .all(|r| r["day"].as_u64().unwrap() >= cut as u64));
        }
    }
    #[test]
    fn new_future_candidate_uses_carried_cash_without_reacquiring_backbook() {
        let old = spec();
        let mut new = old.clone();
        let mut position = new.positions[0].clone();
        position.id = "new-candidate".into();
        position.start_day = 8;
        new.positions.push(position);
        let held = source(17);
        let candidate = source(18);
        let a = Compiled::new(&old, bindings(&held, old.positions[0].id.as_str(), 17, 0)).unwrap();
        let b = Compiled::new(
            &new,
            bindings(&held, new.positions[0].id.as_str(), 17, 0)
                .into_iter()
                .chain(bindings(&candidate, "new-candidate", 18, 8))
                .collect(),
        )
        .unwrap();
        let full = buffer();
        super::super::run_with_commercial(&new, None, &b, Box::new(full.clone())).unwrap();
        let full = tables(&full);
        let window = buffer();
        run_commercial_cut(&old, &new, &a, &b, 5, Box::new(window.clone())).unwrap();
        let got = tables(&window);
        let inventory = super::super::commercial_cut_inventory(&old, &new, &a, &b, 5).unwrap();
        inventory.verify_tables(&got).unwrap();
        let bounded = buffer();
        run_commercial_cut_with_inventory(
            &old,
            &new,
            &a,
            &b,
            5,
            Box::new(bounded.clone()),
            inventory.all_stream.encoded_bytes_upper,
        )
        .unwrap();
        assert_eq!(tables(&bounded), got);
        assert!(bounded.0.lock().unwrap().len() <= inventory.visible_stream.encoded_bytes_upper);
        let failed = buffer();
        assert!(run_commercial_cut_with_inventory(
            &old,
            &new,
            &a,
            &b,
            5,
            Box::new(failed.clone()),
            1
        )
        .unwrap_err()
        .contains("prefix/visible complete frames"));
        assert!(failed.0.lock().unwrap().is_empty());
        assert_eq!(got["closing_statements"], full["closing_statements"]);
        assert_eq!(got["trial_balance"], full["trial_balance"]);
        assert_eq!(got["closing_statements"][0]["cash"], 1002.);
        let acquisitions = got["journal"]
            .iter()
            .filter(|r| r["event"] == "reviewed_opening_snapshot")
            .map(|r| r["transaction_number"].as_u64().unwrap())
            .collect::<std::collections::BTreeSet<_>>();
        assert_eq!(acquisitions.len(), 1);
    }
    #[test]
    fn changed_observed_cash_refuses_before_prefix() {
        let s = spec();
        let source = source(17);
        let mut changed = HandSource(source.0.clone());
        changed.0[0].cash -= 1.;
        let a = Compiled::new(&s, bindings(&source, s.positions[0].id.as_str(), 17, 0)).unwrap();
        let b = Compiled::new(&s, bindings(&changed, s.positions[0].id.as_str(), 17, 0)).unwrap();
        assert!(run_commercial_cut(&s, &s, &a, &b, 5, Box::new(io::sink()))
            .unwrap_err()
            .contains("observed group"));
    }

    fn assert_future_equal(
        full: &BTreeMap<String, Vec<Value>>,
        got: &BTreeMap<String, Vec<Value>>,
        cut: u64,
    ) {
        assert_eq!(got["closing_statements"], full["closing_statements"]);
        assert_eq!(got["trial_balance"], full["trial_balance"]);
        let after = |rows: &Vec<Value>| {
            rows.iter()
                .filter(|r| r["day"].as_u64().unwrap() > cut)
                .cloned()
                .collect::<Vec<_>>()
        };
        assert_eq!(after(&got["journal"]), after(&full["journal"]));
    }
    #[test]
    fn zero_half_and_full_allocations_keep_actual_native_cut_state() {
        let s = spec();
        let owner = source(17);
        for face in [0., 50., 100.] {
            let mut rows = bindings(&owner, s.positions[0].id.as_str(), 17, 0);
            for row in &mut rows {
                row.allocated_face = face;
            }
            let c = Compiled::new(&s, rows).unwrap();
            let full = buffer();
            super::super::run_with_commercial(&s, None, &c, Box::new(full.clone())).unwrap();
            let full = tables(&full);
            for cut in [0, 5, 10, 20] {
                let window = buffer();
                run_commercial_cut(&s, &s, &c, &c, cut, Box::new(window.clone())).unwrap();
                let got = tables(&window);
                assert_future_equal(&full, &got, cut as u64);
                assert_eq!(got["closing_statements"][0]["cash"], 1000. + face / 100.);
            }
        }
    }
    fn prepaid(sign: f64, id: &str) -> portfolio_model_core::nonintegral_fee::ResultData {
        use portfolio_model_core::nonintegral_fee::{evaluate, Request, SCHEMA};
        let identity = json!({"id":"review", "revision":"1"});
        let event = |day, sequence, action, reason| {
            json!({"event_id":format!("fee-{sequence}"),
            "fee_id":"fee", "source":identity, "review":identity, "ordinal":730000+day,
            "sequence":sequence,"action":action,"reason":reason,"amount":sign*5.})
        };
        let r: Request = serde_json::from_value(json!({"schema":SCHEMA,"measure":"physical",
            "currency":"USD","monetary_unit":"USD","policy":identity,
            "asof_ordinal":730000,"reporting_end_ordinal":730005,"complete_end_ordinal":730030,
            "fees":[{"fee_id":"fee","source":identity,"classification_review":identity,
              "service":identity,"position_id":id,"tranche_id":"base","entitlement":sign*5.,
              "prepaid_refunds_allowed":true}],
            "events":[event(0,0,"cash","settlement"),event(20,1,"earn","service")]}))
        .unwrap();
        evaluate(&r).unwrap()
    }
    #[test]
    fn signed_live_prepaid_fee_rights_survive_cut_without_recognition_or_cash_duplication() {
        use crate::owners::commercial::intents::FeeBinding;
        let s = spec();
        let owner = source(17);
        let id = s.positions[0].id.as_str();
        for sign in [1., -1.] {
            let fee = prepaid(sign, id);
            let years = [0., 20. / 365.];
            for face in [0., 50., 100.] {
                let mut rows = bindings(&owner, id, 17, 0);
                for row in &mut rows {
                    row.allocated_face = face;
                }
                let mut c = Compiled::new(&s, rows).unwrap();
                c.attach_fees(
                    &s,
                    vec![
                        FeeBinding {
                            loan_binding: 0,
                            native_position_id: id,
                            source: &fee,
                            native_years: &years,
                        },
                        FeeBinding {
                            loan_binding: 1,
                            native_position_id: id,
                            source: &fee,
                            native_years: &years,
                        },
                    ],
                )
                .unwrap();
                let full = buffer();
                super::super::run_with_commercial(&s, None, &c, Box::new(full.clone())).unwrap();
                let full = tables(&full);
                for cut in [0, 5, 10, 20] {
                    let window = buffer();
                    run_commercial_cut(&s, &s, &c, &c, cut, Box::new(window.clone())).unwrap();
                    let got = tables(&window);
                    if cut > 0 {
                        let inventory =
                            super::super::commercial_cut_inventory(&s, &s, &c, &c, cut).unwrap();
                        inventory.verify_tables(&got).unwrap();
                        let mut altered = got.clone();
                        altered.get_mut("commercial_daily").unwrap()[0]["foreign_snapshot"] =
                            json!(true);
                        assert!(inventory
                            .verify_tables(&altered)
                            .unwrap_err()
                            .contains("field identity"));
                    }
                    assert_future_equal(&full, &got, cut as u64);
                    assert_eq!(
                        got["closing_statements"][0]["cash"],
                        1000. + face / 100. * (1. + sign * 5.)
                    );
                    if cut < 20 && face > 0. {
                        let account = if sign > 0. {
                            "commercial_deferred_fee_income"
                        } else {
                            "commercial_prepaid_fee_expense"
                        };
                        assert!(got["journal"]
                            .iter()
                            .any(|r| r["event"] == "native_verified_cut_opening"
                                && r["gl_account"] == account));
                    }
                }
            }
        }
    }
    #[test]
    fn changed_allocation_calendar_or_past_candidate_refuses_before_output() {
        let s = spec();
        let owner = source(17);
        let id = s.positions[0].id.as_str();
        let c = Compiled::new(&s, bindings(&owner, id, 17, 0)).unwrap();
        let mut rows = bindings(&owner, id, 17, 0);
        for row in &mut rows {
            row.allocated_face = 50.;
        }
        let changed = Compiled::new(&s, rows).unwrap();
        let out = buffer();
        assert!(
            run_commercial_cut(&s, &s, &c, &changed, 5, Box::new(out.clone()))
                .unwrap_err()
                .contains("immutable")
        );
        assert!(out.0.lock().unwrap().is_empty());
        let mut revised = s.clone();
        revised.horizon_days += 1;
        let future = Compiled::new(&revised, bindings(&owner, id, 17, 0)).unwrap();
        assert!(
            run_commercial_cut(&s, &revised, &c, &future, 5, Box::new(out.clone()))
                .unwrap_err()
                .contains("immutable")
        );
        let mut revised = s.clone();
        let mut position = revised.positions[0].clone();
        position.id = "too-early".into();
        revised.positions.push(position);
        let second = source(18);
        let future = Compiled::new(
            &revised,
            bindings(&owner, id, 17, 0)
                .into_iter()
                .chain(bindings(&second, "too-early", 18, 5))
                .collect(),
        )
        .unwrap();
        assert!(
            run_commercial_cut(&s, &revised, &c, &future, 5, Box::new(out.clone()))
                .unwrap_err()
                .contains("predates")
        );
        assert!(out.0.lock().unwrap().is_empty());
    }
    #[test]
    fn comparison_budget_overflow_refuses_before_writing_any_prefix() {
        let s = spec();
        let owner = source(17);
        let id = s.positions[0].id.as_str();
        let c = Compiled::new(&s, bindings(&owner, id, 17, 0)).unwrap();
        let mut future = Compiled::new(&s, bindings(&owner, id, 17, 0)).unwrap();
        future.retained_bytes_upper = usize::MAX;
        let out = buffer();
        assert!(
            run_commercial_cut(&s, &s, &c, &future, 5, Box::new(out.clone()))
                .unwrap_err()
                .contains("overflow")
        );
        assert!(out.0.lock().unwrap().is_empty());
        future.retained_bytes_upper = 1_000_000;
        assert!(
            run_commercial_cut(&s, &s, &c, &future, 5, Box::new(out.clone()))
                .unwrap_err()
                .contains("exceeds8M")
        );
        assert!(out.0.lock().unwrap().is_empty());
    }

    #[test]
    fn cancellation_during_visible_cut_stream_never_emits_completion() {
        struct CancelOnWrite {
            cancelled: Arc<AtomicBool>,
            buffer: Buffer,
        }
        impl Write for CancelOnWrite {
            fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
                self.cancelled.store(true, Ordering::Relaxed);
                self.buffer.write(bytes)
            }
            fn flush(&mut self) -> io::Result<()> {
                Ok(())
            }
        }
        let s = spec();
        let owner = source(17);
        let c = Compiled::new(&s, bindings(&owner, s.positions[0].id.as_str(), 17, 0)).unwrap();
        let cancelled = Arc::new(AtomicBool::new(false));
        let control =
            portfolio_compute_control::Control::with_cancel_flag(10000, cancelled.clone()).unwrap();
        let out = buffer();
        let result = portfolio_compute_control::scope(Some(control), || {
            run_commercial_cut(
                &s,
                &s,
                &c,
                &c,
                5,
                Box::new(CancelOnWrite {
                    cancelled,
                    buffer: out.clone(),
                }),
            )
        });
        assert!(result.unwrap_err().contains("COMPUTE_CANCELLED"));
        let bytes = out.0.lock().unwrap();
        assert!(!bytes.is_empty());
        assert!(!String::from_utf8_lossy(&bytes).contains("\"complete\":true"));
    }
}

#[cfg(test)]
mod inventory_tests {
    use super::*;
    #[test]
    fn all_frame_capacity_counts_muted_prefix_and_resumed_bytes() {
        let enabled = Arc::new(AtomicBool::new(false));
        let mut writer = WindowWriter {
            enabled: enabled.clone(),
            output: Box::new(io::sink()),
            structural_bytes: Some((0, 6)),
        };
        assert_eq!(writer.write(b"abc").unwrap(), 3);
        enabled.store(true, Ordering::Relaxed);
        assert_eq!(writer.write(b"de").unwrap(), 2);
        assert!(writer
            .write(b"fg")
            .unwrap_err()
            .to_string()
            .contains("prefix/visible complete frames"));
    }
    #[test]
    fn omitted_writer_preserves_unbounded_component_behavior() {
        let mut writer = WindowWriter {
            enabled: Arc::new(AtomicBool::new(false)),
            output: Box::new(io::sink()),
            structural_bytes: None,
        };
        assert_eq!(writer.write(b"ordinary prefix").unwrap(), 15);
        assert!(writer.structural_bytes.is_none());
    }
}
