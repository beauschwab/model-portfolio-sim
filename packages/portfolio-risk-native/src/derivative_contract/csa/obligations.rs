//! Derived contractual MPOR obligations. Instructions name events; they never supply cash amounts.
use super::*;
use crate::term_enhancements::indexed::{
    build_path, ContractPathRequest, Coupon, PrincipalPlan, RateSource,
};
use portfolio_model_core::cashflow::CashflowMeasure;
use portfolio_model_core::floating_rate::Direction;
#[derive(Debug)]
struct Due {
    event_id: String,
    cash: f64,
    sources: Vec<RateSource>,
}
pub(super) struct Book<'a> {
    policy: &'a MporPolicy,
    default: &'a DefaultPolicy,
    instructions: HashMap<(String, String, i32), &'a ObligationInstruction>,
    used: HashSet<(String, String, i32)>,
    used_paths: HashSet<(String, String)>,
    nodes: HashSet<i32>,
    physical_paths: HashMap<(String, String), &'a PhysicalIndexPath>,
    pub ledger: Vec<MporObligation>,
    pub exercises: Vec<MporExercise>,
}
impl<'a> Book<'a> {
    pub fn new(
        policy: &'a MporPolicy,
        default: &'a DefaultPolicy,
        path: &Path,
        measure: CashflowMeasure,
        bytes: &mut usize,
        work: &mut usize,
    ) -> Result<Self, String> {
        identity(&policy.identity, bytes)?;
        if policy.claim_measure != measure
            || measure == CashflowMeasure::Contractual
            || policy.settlement_instructions.len() > 250000
            || policy.physical_index_paths.len() > 4096
        {
            return Err("MPOR reviewed claim measure/bounds mismatch".into());
        }
        let observation_work = policy.physical_index_paths.iter().try_fold(*work, |w, p| {
            w.checked_add(p.observations.len())
                .ok_or("MPOR work overflow")
        })?;
        if observation_work > 250000 {
            return Err("MPOR combined observation work exceeds 250000".into());
        }
        let node_availability: HashMap<_, _> = path
            .nodes
            .iter()
            .map(|n| (n.ordinal, n.settlement_available))
            .collect();
        let raw = serde_json::to_vec(policy).map_err(|e| e.to_string())?;
        *bytes = bytes
            .checked_add(raw.len())
            .ok_or("MPOR metadata overflow")?;
        if *bytes > 32 * 1024 * 1024 {
            return Err("combined MPOR metadata exceeds 32 MiB".into());
        }
        let mut instructions = HashMap::new();
        for instruction in &policy.settlement_instructions {
            text(&instruction.position_id, bytes)?;
            text(&instruction.event_id, bytes)?;
            if instruction.payment_ordinal < default.default_ordinal
                || instruction.payment_ordinal > default.closeout_ordinal
                || !node_availability.contains_key(&instruction.payment_ordinal)
            {
                return Err(
                    "every MPOR payment instruction requires an admitted contractual due-date node"
                        .into(),
                );
            }
            if let ObligationDisposition::SettleOnNode { ordinal, evidence } =
                &instruction.disposition
            {
                identity(evidence, bytes)?;
                if *ordinal < instruction.payment_ordinal
                    || *ordinal > default.closeout_ordinal
                    || node_availability.get(ordinal) != Some(&true)
                {
                    return Err("MPOR settlement instruction requires an available admitted due-or-later node by closeout".into());
                }
            }
            let key = (
                instruction.position_id.clone(),
                instruction.event_id.clone(),
                instruction.payment_ordinal,
            );
            if instructions.insert(key, instruction).is_some() {
                return Err("duplicate MPOR obligation instruction".into());
            }
        }
        let mut physical = HashSet::new();
        let mut physical_paths = HashMap::new();
        for p in &policy.physical_index_paths {
            text(&p.position_id, bytes)?;
            text(&p.period_id, bytes)?;
            identity(&p.path, bytes)?;
            physical_paths.insert((p.position_id.clone(), p.period_id.clone()), p);
            *work = work
                .checked_add(p.observations.len())
                .ok_or("MPOR work overflow")?;
            if *work > 250000
                || measure != CashflowMeasure::Physical
                || !physical.insert((&p.position_id, &p.period_id))
            {
                return Err(
                    "Physical MPOR index paths require unique bounded physical-measure inputs"
                        .into(),
                );
            }
        }
        Ok(Self {
            policy,
            default,
            instructions,
            used: HashSet::new(),
            used_paths: HashSet::new(),
            nodes: node_availability.into_keys().collect(),
            physical_paths,
            ledger: Vec::new(),
            exercises: Vec::new(),
        })
    }
    fn eligible(&self, date: i32, timing: swaps::PaymentTiming) -> bool {
        date >= self.default.default_ordinal
            && date <= self.default.closeout_ordinal
            && (date > self.default.default_ordinal
                || timing == swaps::PaymentTiming::BeforeScheduledPayments)
    }
    pub fn require_dates(&self, t: &Trade, _path: &Path) -> Result<(), String> {
        for d in dates(t) {
            if d >= self.default.default_ordinal
                && d <= self.default.closeout_ordinal
                && !self.nodes.contains(&d)
            {
                return Err("every contractual coupon/European exercise/settlement date within MPOR must be an admitted node".into());
            }
        }
        Ok(())
    }
    /// Returns the Pricing due-right already embedded in a Before-payments mark.
    pub fn capture(&mut self, t: &Trade, n: &Node, bucket: &str) -> Result<f64, String> {
        if !self.eligible(n.ordinal, n.payment_timing) {
            return Ok(0.);
        }
        let id = position(t);
        if let Trade::ExpiredEuropean {
            terminal, election, ..
        } = t
        {
            if terminal.expiry_ordinal == n.ordinal {
                let result = options::value_european(terminal)?;
                let state = result
                    .states
                    .first()
                    .ok_or("missing reviewed European expiry state")?;
                self.exercises.push(MporExercise {
                    position_id: id.into(),
                    expiry_ordinal: n.ordinal,
                    contract: terminal.contract.clone(),
                    election: election.clone(),
                    state_id: state.state_id.clone(),
                    exercised: state.exercised,
                    settlement: terminal.settlement,
                    delivered_position_id: if state.exercised
                        && terminal.settlement == options::Settlement::PhysicalDelivery
                    {
                        Some(state.delivered_position_id.clone())
                    } else {
                        None
                    },
                    netting_set_id: state.netting_set_id.clone(),
                    underlying_contract: terminal.states[0].underlying.contract.clone(),
                    pricing_intrinsic_value: state.intrinsic_value,
                    claim_measure: self.policy.claim_measure,
                    conditional_model: terminal.conditional_model.clone(),
                    collateral_discount_source: terminal.states[0]
                        .underlying
                        .discount_curve
                        .source
                        .clone(),
                });
            }
        }
        let mut pricing = Vec::new();
        let mut actual = Vec::new();
        match t {
            Trade::Swap { request } => {
                pricing = swap_due(request, n.ordinal)?;
                actual = self.actual_swap_due(id, request, n, &pricing)?;
            }
            Trade::EuropeanSwaption { .. } => {}
            Trade::ExpiredEuropean {
                terminal,
                delivered_underlying,
                ..
            } => {
                if terminal.settlement == options::Settlement::CollateralizedCashPrice
                    && terminal.settlement_ordinal == n.ordinal
                {
                    let result = options::value_european(terminal)?;
                    let state = result.states.first().ok_or("missing expiry state")?;
                    let sign = if terminal.side == options::OptionSide::Long {
                        1.
                    } else {
                        -1.
                    };
                    // The reviewed cash settlement contract is defined on clean value at expiry, not future realized coupons.
                    pricing.push(Due {
                        event_id: format!("option_cash:{}", state.state_id),
                        cash: amount(sign * state.settlement_cash)?,
                        sources: Vec::new(),
                    });
                    actual = pricing
                        .iter()
                        .map(|p| Due {
                            event_id: p.event_id.clone(),
                            cash: p.cash,
                            sources: Vec::new(),
                        })
                        .collect();
                } else if let Some(delivered) = delivered_underlying {
                    pricing = swap_due(delivered, n.ordinal)?;
                    actual = self.actual_swap_due(id, delivered, n, &pricing)?;
                }
            }
        }
        let embedded = if n.payment_timing == swaps::PaymentTiming::BeforeScheduledPayments {
            pricing.iter().try_fold(0., |a, p| amount(a + p.cash))?
        } else {
            0.
        };
        for due in actual {
            let key = (id.to_string(), due.event_id.clone(), n.ordinal);
            let instruction = self.instructions.get(&key).ok_or(
                "complete MPOR policy missing native-derived contractual event instruction",
            )?;
            if !self.used.insert(key) {
                return Err("MPOR contractual event captured more than once".into());
            }
            self.ledger.push(MporObligation {
                position_id: id.into(),
                event_id: due.event_id,
                bucket: bucket.into(),
                payment_ordinal: n.ordinal,
                captured_ordinal: n.ordinal,
                currency: match t {
                    Trade::Swap { request } => request.currency.clone(),
                    Trade::EuropeanSwaption { request } => request.currency.clone(),
                    Trade::ExpiredEuropean { terminal, .. } => terminal.currency.clone(),
                },
                amount: amount(due.cash)?,
                measure: self.policy.claim_measure,
                contract: contractual(t).clone(),
                selected_rate_sources: due.sources,
                origin_election: match t {
                    Trade::ExpiredEuropean { election, .. } => Some(election.clone()),
                    _ => None,
                },
                delivered_position_id: match t {
                    Trade::ExpiredEuropean {
                        delivered_underlying: Some(u),
                        ..
                    } => Some(u.position_id.clone()),
                    _ => None,
                },
                settlement_price_sources: match t {
                    Trade::ExpiredEuropean { terminal, .. }
                        if terminal.settlement == options::Settlement::CollateralizedCashPrice =>
                    {
                        vec![
                            terminal.exercise_policy.clone(),
                            terminal.conditional_model.clone(),
                            terminal.states[0].underlying.discount_curve.source.clone(),
                        ]
                    }
                    _ => Vec::new(),
                },
                disposition: instruction.disposition.clone(),
                settled_ordinal: None,
                included_in_closeout: false,
            });
        }
        Ok(embedded)
    }
    fn actual_swap_due(
        &mut self,
        id: &str,
        r: &swaps::SwapRequest,
        n: &Node,
        pricing: &[Due],
    ) -> Result<Vec<Due>, String> {
        if self.policy.claim_measure == CashflowMeasure::Pricing {
            return Ok(pricing
                .iter()
                .map(|p| Due {
                    event_id: p.event_id.clone(),
                    cash: p.cash,
                    sources: p.sources.clone(),
                })
                .collect());
        }
        let mut result = pricing
            .iter()
            .filter(|p| p.event_id.starts_with("fixed:"))
            .map(|p| Due {
                event_id: p.event_id.clone(),
                cash: p.cash,
                sources: Vec::new(),
            })
            .collect::<Vec<_>>();
        for p in r
            .floating_periods
            .iter()
            .filter(|p| p.payment_ordinal == n.ordinal)
        {
            let mut period = p.clone();
            let mut path = r.rate_path.clone();
            if let Some(override_path) = self
                .physical_paths
                .get(&(id.to_string(), p.period_id.clone()))
            {
                let Coupon::Indexed { observations, .. } = &mut period.coupon else {
                    return Err("Physical index override requires mapped indexed coupon".into());
                };
                *observations = override_path.observations.clone();
                path = override_path.path.clone();
                self.used_paths.insert((id.into(), p.period_id.clone()));
            }
            let Coupon::Indexed { observations, .. } = &period.coupon else {
                return Err("Physical floating claim requires explicit index".into());
            };
            for o in observations {
                if let RateSource::ConditionalProjection {
                    measure,
                    information_unix_seconds,
                    ..
                } = &o.source
                {
                    if *measure != CashflowMeasure::Physical
                        || *information_unix_seconds > n.valuation_cutoff_unix_seconds
                    {
                        return Err("Physical MPOR cash cannot reuse Pricing projection or future information".into());
                    }
                }
            }
            let built = build_path(&ContractPathRequest {
                schema: crate::term_enhancements::SCHEMA.into(),
                model_version: crate::term_enhancements::MODEL_VERSION.into(),
                contract: r.contract.clone(),
                assumption: r.index_basis.clone(),
                path,
                position_id: r.position_id.clone(),
                scenario_id: r.scenario_id.clone(),
                currency: r.currency.clone(),
                monetary_unit: r.monetary_unit.clone(),
                direction: Direction::Receipt,
                measure: CashflowMeasure::Physical,
                asof_ordinal: n.ordinal,
                reporting_end_ordinal: n.ordinal,
                tail_end_ordinal: n.ordinal.max(period.end_ordinal),
                observed_publication_cutoff_unix_seconds: n.valuation_cutoff_unix_seconds,
                opening_principal: r.notional,
                principal_plan: PrincipalPlan::Supplied {
                    principal_payments: vec![r.notional],
                },
                periods: vec![period],
            })?;
            let sign = if r.side == swaps::Side::Payer {
                1.
            } else {
                -1.
            };
            let amount_due = built
                .schedule
                .events
                .iter()
                .try_fold(0., |a, e| amount(a + sign * e.components.cash_interest))?;
            result.push(Due {
                event_id: format!("floating:{}", p.period_id),
                cash: amount_due,
                sources: built
                    .audit
                    .into_iter()
                    .flat_map(|a| a.selected_rate_sources)
                    .collect(),
            });
        }
        Ok(result)
    }
    pub fn settle(&mut self, date: i32) -> Result<Vec<(String, f64)>, String> {
        let mut paid = Vec::new();
        for claim in &mut self.ledger {
            if claim.settled_ordinal.is_none() && !claim.included_in_closeout {
                if let ObligationDisposition::SettleOnNode { ordinal, .. } = &claim.disposition {
                    if *ordinal == date {
                        claim.settled_ordinal = Some(date);
                        paid.push((claim.bucket.clone(), claim.amount));
                    }
                }
            }
        }
        Ok(paid)
    }
    pub fn unpaid(&self, bucket: &str) -> Result<f64, String> {
        self.ledger
            .iter()
            .filter(|c| {
                c.bucket == bucket && c.settled_ordinal.is_none() && !c.included_in_closeout
            })
            .try_fold(0., |a, c| amount(a + c.amount))
    }
    pub fn close(&mut self, bucket: &str) {
        for c in &mut self.ledger {
            if c.bucket == bucket && c.settled_ordinal.is_none() {
                c.included_in_closeout = true;
            }
        }
    }
    pub fn finish(&self) -> Result<(), String> {
        if self.used.len() != self.instructions.len()
            || self.used_paths.len() != self.policy.physical_index_paths.len()
        {
            return Err("unused MPOR event/Physical index instruction does not identify a captured contractual right".into());
        }
        if self
            .ledger
            .iter()
            .any(|c| c.settled_ordinal.is_none() && !c.included_in_closeout)
        {
            return Err("MPOR unpaid obligation tail unresolved".into());
        }
        Ok(())
    }
}
fn swap_due(r: &swaps::SwapRequest, date: i32) -> Result<Vec<Due>, String> {
    let mut before = r.clone();
    before.payment_timing = swaps::PaymentTiming::BeforeScheduledPayments;
    let result = swaps::value_swap(&before)?;
    result
        .schedule
        .events
        .iter()
        .filter(|e| e.payment_ordinal == date)
        .map(|e| {
            Ok(Due {
                event_id: e.event_id.clone(),
                cash: e.components.cash_total()?,
                sources: if let Some(period) = e.event_id.strip_prefix("floating:") {
                    result
                        .floating_audit
                        .iter()
                        .find(|a| a.period_id == period)
                        .map_or(Vec::new(), |a| a.selected_rate_sources.clone())
                } else {
                    Vec::new()
                },
            })
        })
        .collect()
}
fn dates(t: &Trade) -> Vec<i32> {
    match t {
        Trade::Swap { request: r } => r
            .fixed_periods
            .iter()
            .map(|p| p.payment_ordinal)
            .chain(r.floating_periods.iter().map(|p| p.payment_ordinal))
            .collect(),
        Trade::EuropeanSwaption { request: r } => vec![r.expiry_ordinal, r.settlement_ordinal],
        Trade::ExpiredEuropean {
            terminal,
            delivered_underlying,
            ..
        } => {
            let mut dates = vec![terminal.expiry_ordinal, terminal.settlement_ordinal];
            if let Some(u) = delivered_underlying {
                dates.extend(u.fixed_periods.iter().map(|p| p.payment_ordinal));
                dates.extend(u.floating_periods.iter().map(|p| p.payment_ordinal));
            }
            dates
        }
    }
}
