//! One ordered journal: the undesignated derivative owner's postings re-owned under
//! reviewed formal relationships, plus hedged-item basis and OCI mechanics.
use super::{inputs, types::*};
use crate::products::derivatives::accounting::{
    self as derivative_accounting, AccountCode as DerivativeAccount, JournalLedger,
};
use crate::products::derivatives::contract::{
    amount,
    csa::Trade,
    near,
    swaps::{self, DiscountCurve, PaymentTiming, SwapRequest},
    Identity,
};
use crate::products::term::contracts::indexed::{Coupon, RateSource};
use portfolio_model_core::cashflow::{actual_year_fraction, CashflowMeasure};
use std::collections::BTreeMap;

struct FrozenBasis {
    amount: f64,
    start: i32,
    amortized: f64,
}
struct FrozenOci {
    amount: f64,
    released: f64,
    weights: Vec<(i32, f64)>,
}
struct State {
    status: Status,
    scale: f64,
    proportion: f64,
    unit_clean: Option<f64>,
    hyp_prev: Option<f64>,
    inst_prev: Option<f64>,
    cum_d: f64,
    cum_e: f64,
    basis: f64,
    frozen_basis: Vec<FrozenBasis>,
    oci: f64,
    frozen_oci: Vec<FrozenOci>,
    basis_adjustments: f64,
    basis_amortized: f64,
    basis_derecognized: f64,
    oci_recognized: f64,
    oci_reclassified: f64,
    last_clean: Option<f64>,
    last_hyp: Option<f64>,
}
struct Ledger<'a> {
    request: &'a HedgeAccountingRequest,
    scenario_id: String,
    financial: BTreeMap<Account, f64>,
    memo: BTreeMap<Account, f64>,
    intents: Vec<Intent>,
}
fn posting(account: Account, value: f64, owner: Option<&str>, bucket: Option<&str>, relationship: Option<&str>) -> Posting {
    Posting {
        account,
        position_id: owner.map(str::to_string),
        bucket: bucket.map(str::to_string),
        relationship_id: relationship.map(str::to_string),
        debit_amount: value,
    }
}
impl<'a> Ledger<'a> {
    fn new(request: &'a HedgeAccountingRequest, scenario_id: &str) -> Result<Self, String> {
        let o = &request.derivative.opening;
        let mut financial = BTreeMap::new();
        for (a, v) in [
            (Account::Cash, o.cash),
            (Account::PostedVmAsset, o.posted_vm_asset),
            (Account::ReceivedVmLiability, -o.received_vm_liability),
            (Account::PostedImAsset, o.posted_im_asset),
            (Account::OpeningEquity, -o.equity_credit_balance),
        ] {
            financial.insert(a, amount(v)?);
        }
        for t in &o.trades {
            *financial.entry(Account::DerivativeAsset).or_default() += t.clean_value.max(0.);
            *financial.entry(Account::DerivativeLiability).or_default() -= (-t.clean_value).max(0.);
        }
        let mut memo = BTreeMap::new();
        memo.insert(Account::MemoCustodianAsset, o.received_im_custody);
        memo.insert(Account::MemoCustodianObligation, -o.received_im_custody);
        Ok(Self { request, scenario_id: scenario_id.into(), financial, memo, intents: Vec::new() })
    }
    #[allow(clippy::too_many_arguments)]
    fn emit(
        &mut self,
        kind: &str,
        date: i32,
        ledger: JournalLedger,
        mapped: Option<&str>,
        owner: Option<&str>,
        bucket: Option<&str>,
        relationship: Option<&str>,
        policy: &Identity,
        sources: Vec<Identity>,
        mut postings: Vec<Posting>,
    ) -> Result<(), String> {
        postings.retain(|p| p.debit_amount != 0.);
        if postings.is_empty() {
            return Ok(());
        }
        let sum = postings.iter().try_fold(0., |a, p| amount(a + amount(p.debit_amount)?))?;
        if !near(sum, 0.) {
            return Err(format!("unbalanced hedge intent {kind}: {sum}"));
        }
        if self.intents.len() >= 250000 {
            return Err("hedge journal output exceeds 250000 intents".into());
        }
        let balances = if ledger == JournalLedger::Financial { &mut self.financial } else { &mut self.memo };
        for p in &postings {
            let value = balances.entry(p.account).or_default();
            *value = amount(*value + p.debit_amount)?;
        }
        let r = self.request;
        self.intents.push(Intent {
            event_id: format!("{kind}:{}", self.intents.len()),
            ordinal: date,
            ledger,
            run: r.run.clone(),
            scenario_id: self.scenario_id.clone(),
            entity: r.entity.clone(),
            currency: r.currency.clone(),
            policy: policy.clone(),
            sources,
            mapped_position_id: mapped.map(str::to_string),
            contract_position_id: owner.map(str::to_string),
            bucket: bucket.map(str::to_string),
            relationship_id: relationship.map(str::to_string),
            measure: CashflowMeasure::Physical,
            basis: r.derivative.basis,
            postings,
        });
        Ok(())
    }
    fn balances(map: &BTreeMap<Account, f64>) -> Vec<Balance> {
        map.iter().map(|(a, v)| Balance { account: *a, debit_balance: *v }).collect()
    }
}

fn node_curve<'a>(node: &'a crate::products::derivatives::contract::csa::Node, position: &str) -> Result<&'a DiscountCurve, String> {
    node.trades
        .iter()
        .find_map(|t| match t {
            Trade::Swap { request } if request.position_id == position => Some(&request.discount_curve),
            _ => None,
        })
        .ok_or_else(|| "hedging instrument absent from derivative node".into())
}
/// Benchmark-component clean value per unit of hedged notional.
fn unit_clean(item: &HedgedItem, rate: f64, curve: &DiscountCurve, t: i32, timing: PaymentTiming) -> Result<f64, String> {
    let active = |pay: i32| if timing == PaymentTiming::BeforeScheduledPayments { pay >= t } else { pay > t };
    let mut dirty = 0.;
    let mut accrued = 0.;
    for p in &item.periods {
        let tau = actual_year_fraction(p.start_ordinal, p.end_ordinal, p.day_count)?;
        if active(p.payment_ordinal) {
            dirty += rate * tau * curve.at(p.payment_ordinal)?;
            if p.end_ordinal <= t {
                accrued += rate * tau;
            }
        }
        if p.start_ordinal <= t && t < p.end_ordinal {
            accrued += rate * actual_year_fraction(p.start_ordinal, t, p.day_count)?;
        }
    }
    if active(item.principal_ordinal) {
        dirty += curve.at(item.principal_ordinal)?;
    }
    amount(dirty - accrued)
}
fn hypothetical_value(req: &SwapRequest) -> Result<f64, String> {
    swaps::value_swap(req).map(|v| v.clean_value)
}
fn hypothetical_settlement(req: &SwapRequest, t: i32) -> Result<f64, String> {
    for p in &req.floating_periods {
        if p.payment_ordinal == t {
            if let Coupon::Indexed { observations, .. } = &p.coupon {
                for o in observations {
                    match &o.source {
                        RateSource::Observed { .. } => {}
                        RateSource::ConditionalProjection { measure: CashflowMeasure::Physical, .. } => {}
                        _ => return Err("matured hypothetical period requires observed or Physical fixings".into()),
                    }
                }
            }
        }
    }
    let mut before = req.clone();
    before.payment_timing = PaymentTiming::BeforeScheduledPayments;
    let priced = swaps::value_swap(&before)?;
    priced
        .schedule
        .events
        .iter()
        .filter(|e| e.payment_ordinal == t)
        .try_fold(0., |a, e| amount(a + e.components.cash_total()?))
}
fn ratio(d: f64, e: f64) -> Option<f64> {
    if e == 0. {
        None
    } else {
        Some(d / e)
    }
}

pub(super) fn evaluate(r: &HedgeAccountingRequest) -> Result<HedgeAccountingResult, String> {
    let compiled = inputs::admit(r)?;
    portfolio_compute_control::checkpoint()?;
    let derivative = derivative_accounting::evaluate(&r.derivative)?;
    portfolio_compute_control::checkpoint()?;
    let c = inputs::csa(r)?;
    let path = &c.paths[0];
    if derivative.daily.len() != path.nodes.len() {
        return Err("derivative owner daily closings do not match path nodes".into());
    }
    let mut ledger = Ledger::new(r, &path.path_id)?;
    let band = &r.policy.assessment;
    let mut states = r
        .relationships
        .iter()
        .map(|rel| State {
            status: Status::Pending,
            scale: 1.,
            proportion: rel.designated_proportion,
            unit_clean: None,
            hyp_prev: None,
            inst_prev: None,
            cum_d: 0.,
            cum_e: 0.,
            basis: 0.,
            frozen_basis: Vec::new(),
            oci: 0.,
            frozen_oci: Vec::new(),
            basis_adjustments: 0.,
            basis_amortized: 0.,
            basis_derecognized: 0.,
            oci_recognized: 0.,
            oci_reclassified: 0.,
            last_clean: None,
            last_hyp: None,
        })
        .collect::<Vec<_>>();
    let mut lifecycle = Vec::new();
    let mut assessments = Vec::new();
    let mut daily = Vec::new();
    let mut cursor = 0usize;
    let mut event_cursor = 0usize;
    let rel_index: BTreeMap<&str, usize> = r.relationships.iter().enumerate().map(|(i, x)| (x.relationship_id.as_str(), i)).collect();
    for (k, node) in path.nodes.iter().enumerate() {
        portfolio_compute_control::checkpoint()?;
        let t = node.ordinal;
        // Status at the start of the node governs CFH mark ownership for this node.
        let cfh_shares: Vec<(usize, f64)> = r
            .relationships
            .iter()
            .enumerate()
            .filter(|(i, x)| x.designation == Designation::CashFlowHedge && states[*i].status == Status::Qualified)
            .map(|(i, _)| (i, states[i].proportion))
            .collect();
        if cursor < derivative.intents.len() && derivative.intents[cursor].ordinal < t {
            return Err("derivative owner intents are not in node order".into());
        }
        while cursor < derivative.intents.len() && derivative.intents[cursor].ordinal == t {
            let di = &derivative.intents[cursor];
            cursor += 1;
            let mark = di.event_id.starts_with("clean_mark:");
            let mut postings = Vec::with_capacity(di.postings.len() + 2);
            let mut relationship: Option<String> = None;
            for p in &di.postings {
                if mark && p.account == DerivativeAccount::MarkGainLoss {
                    let m = amount(p.debit_amount)?;
                    let mapped = di.mapped_position_id.as_deref().ok_or("mark intent lacks mapped position")?;
                    let mut retained = 1.;
                    for &(i, share) in cfh_shares.iter().filter(|(i, _)| r.relationships[*i].hedging_instrument_position_id == mapped) {
                        let st = &mut states[i];
                        let value = amount(m * share)?;
                        postings.push(posting(Account::CashFlowHedgeOci, value, p.position_id.as_deref(), p.bucket.as_deref(), Some(&r.relationships[i].relationship_id)));
                        st.oci = amount(st.oci - value)?;
                        st.oci_recognized = amount(st.oci_recognized - value)?;
                        retained -= share;
                        relationship = Some(r.relationships[i].relationship_id.clone());
                    }
                    postings.push(posting(Account::MarkGainLoss, amount(m * retained)?, p.position_id.as_deref(), p.bucket.as_deref(), None));
                } else {
                    postings.push(posting(p.account.into(), p.debit_amount, p.position_id.as_deref(), p.bucket.as_deref(), None));
                }
            }
            let kind = di.event_id.split(':').next().unwrap_or("derivative");
            ledger.emit(kind, t, di.ledger, di.mapped_position_id.as_deref(), di.contract_position_id.as_deref(), di.bucket.as_deref(), relationship.as_deref(), &di.policy, di.sources.clone(), postings)?;
        }
        let closing_k = &derivative.daily[k];
        let instrument_value = |position: &str| -> Result<f64, String> {
            closing_k
                .trades
                .iter()
                .find(|x| x.mapped_position_id == position)
                .map(|x| x.clean_carrying)
                .ok_or_else(|| "hedging instrument missing from derivative daily closing".into())
        };
        // Hedge postings under the status at the start of the node.
        for (i, rel) in r.relationships.iter().enumerate() {
            let cr = &compiled.relationships[i];
            let st = &mut states[i];
            let instrument = rel.hedging_instrument_position_id.as_str();
            let sources_base = |extra: Vec<Identity>| {
                let mut s = vec![r.policy.identity.clone(), rel.documentation.clone()];
                s.extend(extra);
                s
            };
            if t >= rel.inception_ordinal {
                // Clean cumulative instrument change: after-payment carrying differences exclude
                // settled net coupons, matching the clean hedged-item measure at accrual-free dates.
                let value = instrument_value(instrument)?;
                if st.status == Status::Qualified {
                    let prev = st.inst_prev.ok_or("relationship lacks its inception instrument value")?;
                    st.cum_d = amount(st.cum_d + st.proportion * (value - prev))?;
                }
                st.inst_prev = Some(value);
            }
            match rel.designation {
                Designation::FairValueHedge => {
                    let item = &r.hedged_items[cr.item.ok_or("compiled item missing")?];
                    let hedged = rel.hedged_notional.ok_or("hedged notional missing")?;
                    let rate = rel.benchmark_rate.ok_or("benchmark rate missing")?;
                    if t >= rel.inception_ordinal {
                        let curve = node_curve(node, instrument)?;
                        // Settled item cash is excluded exactly as the derivative owner excludes coupons:
                        // the change measures before-payment value against the prior after-payment value.
                        let before = unit_clean(item, rate, curve, t, PaymentTiming::BeforeScheduledPayments)?;
                        let unit = unit_clean(item, rate, curve, t, PaymentTiming::AfterScheduledPayments)?;
                        if st.status == Status::Qualified {
                            let prev = st.unit_clean.ok_or("fair-value relationship lacks its inception clean value")?;
                            let d_c = amount(hedged * st.scale * (before - prev))?;
                            if d_c != 0. {
                                let dir = cr.direction_sign;
                                ledger.emit(
                                    "hedged_item_basis",
                                    t,
                                    JournalLedger::Financial,
                                    Some(instrument),
                                    Some(&item.item_id),
                                    None,
                                    Some(&rel.relationship_id),
                                    &r.policy.identity,
                                    sources_base(vec![item.source.clone(), item.carrying_source.clone()]),
                                    vec![
                                        posting(Account::HedgedItemBasisAdjustment, dir * d_c, Some(&item.item_id), None, Some(&rel.relationship_id)),
                                        posting(Account::HedgedItemBasisGainLoss, -dir * d_c, Some(&item.item_id), None, Some(&rel.relationship_id)),
                                    ],
                                )?;
                                st.basis = amount(st.basis + d_c)?;
                                st.basis_adjustments = amount(st.basis_adjustments + d_c)?;
                                st.cum_e = amount(st.cum_e - dir * d_c)?;
                            }
                        }
                        st.unit_clean = Some(unit);
                        st.last_clean = Some(amount(hedged * st.scale * unit)?);
                    }
                    for tranche in &mut st.frozen_basis {
                        let target = if item.principal_ordinal <= tranche.start {
                            tranche.amount
                        } else {
                            let fraction = (f64::from(t - tranche.start) / f64::from(item.principal_ordinal - tranche.start)).min(1.);
                            amount(tranche.amount * fraction)?
                        };
                        let delta = amount(target - tranche.amortized)?;
                        if delta != 0. {
                            let dir = cr.direction_sign;
                            ledger.emit(
                                "hedged_item_basis_amortization",
                                t,
                                JournalLedger::Financial,
                                Some(instrument),
                                Some(&item.item_id),
                                None,
                                Some(&rel.relationship_id),
                                &r.policy.identity,
                                sources_base(vec![item.source.clone(), item.carrying_source.clone()]),
                                vec![
                                    posting(Account::HedgedItemBasisAdjustment, -dir * delta, Some(&item.item_id), None, Some(&rel.relationship_id)),
                                    posting(Account::HedgedItemBasisAmortization, dir * delta, Some(&item.item_id), None, Some(&rel.relationship_id)),
                                ],
                            )?;
                            tranche.amortized = target;
                            st.basis_amortized = amount(st.basis_amortized + delta)?;
                        }
                    }
                }
                Designation::CashFlowHedge => {
                    let exposure = &r.forecast_exposures[cr.exposure.ok_or("compiled exposure missing")?];
                    if t >= rel.inception_ordinal {
                        let (h, settlement, evidence) = if let Some(hyp) = &rel.hypothetical {
                            let req = &hyp.nodes[*cr.hypothetical.get(&t).ok_or("hypothetical node missing")?].request;
                            (hypothetical_value(req)?, hypothetical_settlement(req, t)?, hyp.evidence.clone())
                        } else {
                            let physical = derivative
                                .cash_rights
                                .iter()
                                .filter(|x| x.position_id == instrument && x.payment_ordinal == t)
                                .try_fold(0., |a, x| amount(a + x.physical_amount))?;
                            (instrument_value(instrument)?, amount(rel.designated_proportion * physical)?, rel.matched_critical_terms_evidence.clone().ok_or("matched evidence missing")?)
                        };
                        if t == rel.inception_ordinal && rel.hypothetical.is_some() && !near(h, 0.) {
                            return Err("hypothetical derivative must have zero clean value at inception".into());
                        }
                        if st.status == Status::Qualified {
                            let prev = st.hyp_prev.ok_or("cash-flow relationship lacks its inception hypothetical value")?;
                            st.cum_e = amount(st.cum_e + st.scale * (h - prev))?;
                            let due = cr.hedged_periods.iter().any(|&q| exposure.periods[q].payment_ordinal == t);
                            if due {
                                let reclass = amount(st.scale * settlement)?;
                                if reclass != 0. {
                                    ledger.emit(
                                        "cash_flow_hedge_reclassification",
                                        t,
                                        JournalLedger::Financial,
                                        Some(instrument),
                                        Some(&exposure.exposure_id),
                                        None,
                                        Some(&rel.relationship_id),
                                        &r.policy.identity,
                                        sources_base(vec![exposure.source.clone(), exposure.probability_evidence.clone(), evidence.clone()]),
                                        vec![
                                            posting(Account::CashFlowHedgeOci, reclass, Some(&exposure.exposure_id), None, Some(&rel.relationship_id)),
                                            posting(Account::HedgedForecastReclassification, -reclass, Some(&exposure.exposure_id), None, Some(&rel.relationship_id)),
                                        ],
                                    )?;
                                    st.oci = amount(st.oci - reclass)?;
                                    st.oci_reclassified = amount(st.oci_reclassified + reclass)?;
                                }
                            }
                        }
                        st.hyp_prev = Some(h);
                        st.last_hyp = Some(amount(st.scale * h)?);
                    }
                    for tranche in &mut st.frozen_oci {
                        let last = tranche.weights.iter().map(|w| w.0).max();
                        let mut release = 0.;
                        for (pay, w) in &tranche.weights {
                            if *pay == t {
                                release = if last == Some(t) { amount(tranche.amount - tranche.released)? } else { amount(tranche.amount * w)? };
                            }
                        }
                        if release != 0. {
                            ledger.emit(
                                "frozen_oci_release",
                                t,
                                JournalLedger::Financial,
                                Some(instrument),
                                Some(&exposure.exposure_id),
                                None,
                                Some(&rel.relationship_id),
                                &r.policy.identity,
                                sources_base(vec![exposure.source.clone(), exposure.probability_evidence.clone()]),
                                vec![
                                    posting(Account::CashFlowHedgeOci, release, Some(&exposure.exposure_id), None, Some(&rel.relationship_id)),
                                    posting(Account::HedgedForecastReclassification, -release, Some(&exposure.exposure_id), None, Some(&rel.relationship_id)),
                                ],
                            )?;
                            tranche.released = amount(tranche.released + release)?;
                            st.oci_reclassified = amount(st.oci_reclassified + release)?;
                        }
                    }
                }
            }
        }
        // Activation at the end of the inception node.
        for (i, rel) in r.relationships.iter().enumerate() {
            if rel.inception_ordinal != t {
                continue;
            }
            if let Some(prior) = compiled.relationships[i].predecessor {
                if states[prior].status != Status::Discontinued || states[prior].proportion != 0. {
                    return Err("redesignation requires the predecessor relationship to be fully discontinued before inception".into());
                }
            }
            if rel.designation == Designation::CashFlowHedge
                && rel.hypothetical.is_none()
                && !near(instrument_value(&rel.hedging_instrument_position_id)?, 0.)
            {
                return Err("matched critical terms require zero instrument fair value at inception".into());
            }
            let st = &mut states[i];
            if st.status != Status::Pending {
                return Err("relationship activated twice".into());
            }
            st.status = Status::Qualified;
            lifecycle.push(LifecycleRecord {
                ordinal: t,
                sequence: None,
                relationship_id: rel.relationship_id.clone(),
                kind: "inception".into(),
                event_id: None,
                review: Some(rel.documentation.clone()),
                reason: None,
                status_before: Status::Pending,
                status_after: Status::Qualified,
                proportion_before: 0.,
                proportion_after: st.proportion,
                frozen_basis: 0.,
                frozen_oci: 0.,
                released_oci: 0.,
                derecognized_basis: 0.,
            });
        }
        // Reviewed events at this node, in sequence order.
        while event_cursor < r.events.len() && r.events[event_cursor].ordinal == t {
            let ev = &r.events[event_cursor];
            event_cursor += 1;
            let i = rel_index[ev.relationship_id.as_str()];
            let rel = &r.relationships[i];
            let cr = &compiled.relationships[i];
            let instrument = rel.hedging_instrument_position_id.as_str();
            let before = (states[i].status, states[i].proportion);
            let mut record = LifecycleRecord {
                ordinal: t,
                sequence: Some(ev.sequence),
                relationship_id: rel.relationship_id.clone(),
                kind: String::new(),
                event_id: Some(ev.event_id.clone()),
                review: Some(ev.review.clone()),
                reason: None,
                status_before: before.0,
                status_after: before.0,
                proportion_before: before.1,
                proportion_after: before.1,
                frozen_basis: 0.,
                frozen_oci: 0.,
                released_oci: 0.,
                derecognized_basis: 0.,
            };
            let discontinue = |states: &mut Vec<State>, ledger: &mut Ledger<'_>, q: f64, reason: &str, record: &mut LifecycleRecord| -> Result<(), String> {
                let st = &mut states[i];
                if st.status != Status::Qualified {
                    return Err("discontinuation requires a qualified relationship".into());
                }
                let sources = vec![r.policy.identity.clone(), rel.documentation.clone(), ev.review.clone()];
                match rel.designation {
                    Designation::FairValueHedge => {
                        let item = &r.hedged_items[cr.item.ok_or("compiled item missing")?];
                        let frozen = amount(q * st.basis)?;
                        st.basis = amount(st.basis - frozen)?;
                        if reason == "hedged_item_sold" {
                            let dir = cr.direction_sign;
                            ledger.emit(
                                "hedged_item_basis_derecognition",
                                t,
                                JournalLedger::Financial,
                                Some(instrument),
                                Some(&item.item_id),
                                None,
                                Some(&rel.relationship_id),
                                &r.policy.identity,
                                sources,
                                vec![
                                    posting(Account::HedgedItemBasisAdjustment, -dir * frozen, Some(&item.item_id), None, Some(&rel.relationship_id)),
                                    posting(Account::HedgedItemBasisDerecognition, dir * frozen, Some(&item.item_id), None, Some(&rel.relationship_id)),
                                ],
                            )?;
                            st.basis_derecognized = amount(st.basis_derecognized + frozen)?;
                            record.derecognized_basis = frozen;
                        } else if frozen != 0. {
                            st.frozen_basis.push(FrozenBasis { amount: frozen, start: t, amortized: 0. });
                            record.frozen_basis = frozen;
                        }
                    }
                    Designation::CashFlowHedge => {
                        let exposure = &r.forecast_exposures[cr.exposure.ok_or("compiled exposure missing")?];
                        let frozen = amount(q * st.oci)?;
                        st.oci = amount(st.oci - frozen)?;
                        let remaining: Vec<(i32, f64)> = cr
                            .hedged_periods
                            .iter()
                            .map(|&k| &exposure.periods[k])
                            .filter(|p| p.payment_ordinal > t)
                            .map(|p| actual_year_fraction(p.start_ordinal, p.end_ordinal, p.day_count).map(|tau| (p.payment_ordinal, tau)))
                            .collect::<Result<_, _>>()?;
                        let total: f64 = remaining.iter().map(|x| x.1).sum();
                        if remaining.is_empty() || total <= 0. {
                            if frozen != 0. {
                                ledger.emit(
                                    "frozen_oci_release",
                                    t,
                                    JournalLedger::Financial,
                                    Some(instrument),
                                    Some(&exposure.exposure_id),
                                    None,
                                    Some(&rel.relationship_id),
                                    &r.policy.identity,
                                    sources,
                                    vec![
                                        posting(Account::CashFlowHedgeOci, frozen, Some(&exposure.exposure_id), None, Some(&rel.relationship_id)),
                                        posting(Account::HedgedForecastReclassification, -frozen, Some(&exposure.exposure_id), None, Some(&rel.relationship_id)),
                                    ],
                                )?;
                                st.oci_reclassified = amount(st.oci_reclassified + frozen)?;
                                record.released_oci = frozen;
                            }
                        } else if frozen != 0. {
                            st.frozen_oci.push(FrozenOci {
                                amount: frozen,
                                released: 0.,
                                weights: remaining.into_iter().map(|(pay, tau)| (pay, tau / total)).collect(),
                            });
                            record.frozen_oci = frozen;
                        }
                    }
                }
                st.scale *= 1. - q;
                st.proportion = amount(rel.designated_proportion * st.scale)?;
                if q >= 1. {
                    st.scale = 0.;
                    st.proportion = 0.;
                    st.status = Status::Discontinued;
                }
                record.reason = Some(reason.into());
                record.status_after = st.status;
                record.proportion_after = st.proportion;
                Ok(())
            };
            match ev.kind {
                EventKind::Assess => {
                    record.kind = "assessment".into();
                    let st = &states[i];
                    if st.status != Status::Qualified {
                        return Err("assessment on a relationship that is not qualified".into());
                    }
                    let (d, e) = (st.cum_d, st.cum_e);
                    if d == 0. && e == 0. {
                        return Err("assessment requires nonzero cumulative change to measure".into());
                    }
                    let ratio = ratio(d, e);
                    let qualified = ratio.is_some_and(|x| x >= band.lower_bound && x <= band.upper_bound);
                    assessments.push(Assessment {
                        event_id: ev.event_id.clone(),
                        relationship_id: rel.relationship_id.clone(),
                        ordinal: t,
                        method: band.method,
                        lower_bound: band.lower_bound,
                        upper_bound: band.upper_bound,
                        cumulative_instrument_change: d,
                        cumulative_hedged_change: e,
                        ratio,
                        qualified,
                    });
                    if !qualified {
                        discontinue(&mut states, &mut ledger, 1., "failed_assessment", &mut record)?;
                    } else {
                        record.reason = Some("qualified".into());
                    }
                }
                EventKind::Discontinue => {
                    record.kind = "discontinuation".into();
                    let reason = match ev.reason.ok_or("discontinuation reason missing")? {
                        DiscontinueReason::Dedesignation => "dedesignation",
                        DiscontinueReason::HedgedItemSold => "hedged_item_sold",
                        DiscontinueReason::InstrumentTerminated => "instrument_terminated",
                    };
                    discontinue(&mut states, &mut ledger, ev.proportion.unwrap_or(1.), reason, &mut record)?;
                }
                EventKind::ForecastStatus => {
                    record.kind = "forecast_status".into();
                    match ev.status.ok_or("forecast status missing")? {
                        ForecastStatus::NoLongerProbable => {
                            if states[i].status == Status::Qualified {
                                discontinue(&mut states, &mut ledger, 1., "forecast_no_longer_probable", &mut record)?;
                            } else if states[i].status == Status::Pending {
                                return Err("forecast status before inception".into());
                            } else {
                                record.reason = Some("forecast_no_longer_probable".into());
                            }
                        }
                        ForecastStatus::ProbableNotToOccur => {
                            let st = &mut states[i];
                            if st.status == Status::Pending {
                                return Err("forecast status before inception".into());
                            }
                            let exposure = &r.forecast_exposures[cr.exposure.ok_or("compiled exposure missing")?];
                            let mut release = st.oci;
                            for tranche in &mut st.frozen_oci {
                                let left = amount(tranche.amount - tranche.released)?;
                                tranche.released = tranche.amount;
                                release = amount(release + left)?;
                            }
                            st.oci = 0.;
                            if release != 0. {
                                ledger.emit(
                                    "forecast_not_occurring_release",
                                    t,
                                    JournalLedger::Financial,
                                    Some(instrument),
                                    Some(&exposure.exposure_id),
                                    None,
                                    Some(&rel.relationship_id),
                                    &r.policy.identity,
                                    vec![r.policy.identity.clone(), rel.documentation.clone(), ev.review.clone()],
                                    vec![
                                        posting(Account::CashFlowHedgeOci, release, Some(&exposure.exposure_id), None, Some(&rel.relationship_id)),
                                        posting(Account::HedgedForecastReclassification, -release, Some(&exposure.exposure_id), None, Some(&rel.relationship_id)),
                                    ],
                                )?;
                                st.oci_reclassified = amount(st.oci_reclassified + release)?;
                            }
                            st.scale = 0.;
                            st.proportion = 0.;
                            st.status = Status::Discontinued;
                            record.reason = Some("forecast_probable_not_to_occur".into());
                            record.released_oci = release;
                            record.status_after = Status::Discontinued;
                            record.proportion_after = 0.;
                        }
                    }
                }
            }
            lifecycle.push(record);
        }
        // Daily closing reconciles the derivative owner's balances except re-owned marks.
        for b in &closing_k.financial_trial_balance {
            let mine = ledger.financial.get(&b.account.into()).copied().unwrap_or(0.);
            let value = if b.account == DerivativeAccount::MarkGainLoss {
                // Re-owned marks: OCI balance net of reclassifications equals the moved mark share.
                mine + ledger.financial.get(&Account::CashFlowHedgeOci).copied().unwrap_or(0.)
                    - states.iter().map(|s| s.oci_reclassified).sum::<f64>()
            } else {
                mine
            };
            if !near(value, b.debit_balance) {
                return Err(format!("hedge journal does not reconcile the derivative owner's {:?} balance", b.account));
            }
        }
        for b in &closing_k.custody_trial_balance {
            if !near(ledger.memo.get(&b.account.into()).copied().unwrap_or(0.), b.debit_balance) {
                return Err("hedge journal custody memo does not reconcile the derivative owner".into());
            }
        }
        if !near(ledger.financial.values().sum(), 0.) || !near(ledger.memo.values().sum(), 0.) {
            return Err("hedge trial balance failed".into());
        }
        let mut rels = Vec::with_capacity(states.len());
        for (i, rel) in r.relationships.iter().enumerate() {
            let st = &states[i];
            let value = if t >= rel.inception_ordinal { amount(st.proportion * instrument_value(&rel.hedging_instrument_position_id)?)? } else { 0. };
            rels.push(DailyRelationship {
                relationship_id: rel.relationship_id.clone(),
                status: st.status,
                designated_proportion: st.proportion,
                instrument_designated_value: value,
                hedged_item_clean_value: st.last_clean,
                hypothetical_value: st.last_hyp,
                cumulative_instrument_change: st.cum_d,
                cumulative_hedged_change: st.cum_e,
                basis_adjustment_balance: st.basis,
                frozen_basis_balance: st.frozen_basis.iter().map(|x| x.amount - x.amortized).sum(),
                oci_balance: st.oci,
                frozen_oci_balance: st.frozen_oci.iter().map(|x| x.amount - x.released).sum(),
            });
        }
        daily.push(Daily {
            ordinal: t,
            relationships: rels,
            financial_trial_balance: Ledger::balances(&ledger.financial),
            custody_trial_balance: Ledger::balances(&ledger.memo),
        });
    }
    if cursor != derivative.intents.len() {
        return Err("derivative owner intents beyond the path tail".into());
    }
    if event_cursor != r.events.len() {
        return Err("hedge events beyond the path tail".into());
    }
    let last = path.nodes[path.nodes.len() - 1].ordinal;
    let mut relationships = Vec::new();
    let mut closing = Vec::new();
    for (i, rel) in r.relationships.iter().enumerate() {
        let st = &states[i];
        let cr = &compiled.relationships[i];
        let pending = cr
            .exposure
            .map(|x| {
                cr.hedged_periods
                    .iter()
                    .map(|&k| &r.forecast_exposures[x].periods[k])
                    .filter(|p| p.payment_ordinal > last)
                    .map(|p| p.period_id.clone())
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        relationships.push(RelationshipClosing {
            relationship_id: rel.relationship_id.clone(),
            predecessor_relationship_id: rel.predecessor_relationship_id.clone(),
            designation: rel.designation,
            hedging_instrument_position_id: rel.hedging_instrument_position_id.clone(),
            hedged_item_id: rel.hedged_item_id.clone(),
            forecast_exposure_id: rel.forecast_exposure_id.clone(),
            inception_ordinal: rel.inception_ordinal,
            status: st.status,
            designated_proportion: st.proportion,
            basis_adjustment_balance: st.basis,
            frozen_basis_balance: st.frozen_basis.iter().map(|x| x.amount - x.amortized).sum(),
            oci_balance: st.oci,
            frozen_oci_balance: st.frozen_oci.iter().map(|x| x.amount - x.released).sum(),
            pending_reclassification_periods: pending,
        });
        let frozen_basis: f64 = st.frozen_basis.iter().map(|x| x.amount - x.amortized).sum();
        let frozen_oci: f64 = st.frozen_oci.iter().map(|x| x.amount - x.released).sum();
        closing.push(RollForward {
            relationship_id: rel.relationship_id.clone(),
            basis_adjustments: st.basis_adjustments,
            basis_amortized: st.basis_amortized,
            basis_derecognized: st.basis_derecognized,
            basis_closing: amount(st.basis + frozen_basis)?,
            oci_recognized: st.oci_recognized,
            oci_reclassified: st.oci_reclassified,
            oci_closing: amount(st.oci + frozen_oci)?,
        });
        if !near(st.basis_adjustments - st.basis_amortized - st.basis_derecognized, st.basis + frozen_basis)
            || !near(st.oci_recognized - st.oci_reclassified, st.oci + frozen_oci)
        {
            return Err("hedge roll-forward does not reconcile retained balances".into());
        }
    }
    Ok(HedgeAccountingResult {
        schema: SCHEMA.into(),
        model_version: MODEL_VERSION.into(),
        rule_version: RULE_VERSION.into(),
        run: r.run.clone(),
        entity: r.entity.clone(),
        currency: r.currency.clone(),
        monetary_unit: r.monetary_unit.clone(),
        policy: r.policy.clone(),
        derivative,
        relationships,
        lifecycle,
        assessments,
        daily,
        intents: ledger.intents,
        closing,
        accounting_standard_approval: false,
        empirical_validation: false,
        persisted_journal_verified: false,
        saved_book_adoption: false,
        portfolio_layer_supported: false,
        net_investment_supported: false,
        disclosure: "Reviewed modeled formal interest-rate hedge relationships on one Physical derivative source. Instrument marks, coupons, collateral and closeouts are owned once by the undesignated derivative owner and re-owned here under the selected designation; hedged-item benchmark basis, OCI, reclassification, amortization and derecognition follow the asc815-asu2017-12 rule matrix in docs/models/hedge-accounting.md with explicit reviewed assessment bounds. Not accounting-standard approval, observed effectiveness, portfolio-layer, net-investment, option or nonfinancial coverage, saved-book or shared daily journal adoption, treasury/capital feed or complete-scale acceptance.".into(),
    })
}
