//! Stage B (issue #44): native acquisition replay over an admitted plan. A security purchase with
//! its financing advance and pledge, a hedge entry with cash variation margin and supplied exact
//! initial margin, replayed day by day with reservation, obligation creation and settlement kept
//! separate. The engine produces one ordered journal and daily closings for the ledger owner.
//!
//! Hedge entry clean values are native-derived: the Decision coordinator prices the raw swap and
//! passes the value in `ResolvedHedge`. Callers cannot supply intents.
use super::admission::Admitted;
use super::journal::{Closing, Intent, Inventory, Kind, Line};
use super::types::*;
use crate::validation::limits::MAX_MONETARY_AMOUNT;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap};

pub const MAX_ACQUISITION_DAYS: i32 = 366;
pub const MARGIN_REFRESH_REQUIRED: &str = "MARGIN_REFRESH_REQUIRED";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SettlementMechanism {
    /// Delivery and payment are one supported exchange of value.
    DeliveryVersusPayment,
}
/// A new security lot bought at a reviewed quote. The lot is in the plan with its acquisition
/// date equal to the settlement date; `original_cost` there must equal clean cost plus fees.
#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct SecurityPurchase {
    pub lot_id: String,
    pub quote: Revision,
    pub trade_ordinal: i32,
    pub settlement_ordinal: i32,
    pub clean_price_per_100: f64,
    pub accrued_interest: f64,
    pub fees: f64,
    pub mechanism: SettlementMechanism,
}
/// A new funding lot advanced on a date, secured by a plan pledge of a purchased lot.
#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct FinancingAdvance {
    pub lot_id: String,
    pub advance_ordinal: i32,
    pub principal: f64,
    pub pledge_id: String,
}
#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct VariationMarginTerms {
    pub receive_threshold: f64,
    pub post_threshold: f64,
    pub minimum_transfer: f64,
}
/// A new derivative lot entered under a margin agreement. Any nonzero entry value must be paid
/// or received as explicit upfront consideration: no day-one gain or loss is created.
#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct HedgeEntry {
    pub lot_id: String,
    pub agreement_id: String,
    pub entry_ordinal: i32,
    pub upfront_consideration: f64,
    pub variation_margin: VariationMarginTerms,
}
/// Reviewed IM requirement bound to the exact legal portfolio, scenario and date.
#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct SuppliedInitialMargin {
    pub agreement_id: String,
    pub portfolio_lot_ids: Vec<String>,
    pub scenario_id: String,
    pub ordinal: i32,
    pub posted_requirement: f64,
    pub received_requirement: f64,
    pub review: Revision,
}
#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct AcquisitionRequest {
    pub scenario_id: String,
    pub horizon_end_ordinal: i32,
    #[serde(default)]
    pub purchases: Vec<SecurityPurchase>,
    #[serde(default)]
    pub financings: Vec<FinancingAdvance>,
    #[serde(default)]
    pub hedge_entries: Vec<HedgeEntry>,
    #[serde(default)]
    pub initial_margin: Vec<SuppliedInitialMargin>,
}

/// Native-derived entry value of a hedge lot, from the raw contract (never caller supplied).
#[derive(Debug, Clone, PartialEq)]
pub struct ResolvedHedge {
    pub lot_id: String,
    pub clean_value: f64,
    pub pricing_source: Revision,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Receipt {
    pub opening_cash: f64,
    pub closing_cash: f64,
    pub settled_cash_movements: f64,
    pub gross_derivative_assets: f64,
    pub gross_derivative_liabilities: f64,
    pub posted_vm: f64,
    pub received_vm: f64,
    pub posted_im: f64,
    pub received_im_custody: f64,
    pub fees_expensed: f64,
    pub cash_bridge_reconciled: bool,
    pub inventory_conserved: bool,
    pub collateral_conserved: bool,
    pub formal_hedge_accounting: bool,
}
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct AcquisitionJournal {
    pub account_id: String,
    pub scenario_id: String,
    pub as_of_ordinal: i32,
    pub tail_ordinal: i32,
    pub opening_cash: f64,
    pub opening_equity: f64,
    pub intents: Vec<Intent>,
    pub daily: Vec<Closing>,
    pub receipt: Receipt,
}

fn money(x: f64, what: &str) -> Result<f64, Refusal> {
    if x.is_finite() && x.abs() <= MAX_MONETARY_AMOUNT {
        Ok(x)
    } else {
        Err(Refusal::new(
            MALFORMED_INPUT,
            format!("{what} must be finite and within the monetary domain"),
        ))
    }
}
fn near(a: f64, b: f64) -> bool {
    (a - b).abs() <= 1e-9_f64.max(1e-12 * a.abs().max(b.abs()))
}

struct Book {
    financial: BTreeMap<(Kind, String), f64>,
    custody: BTreeMap<(Kind, String), f64>,
    cash: f64,
    restricted: f64,
    settled: f64,
    owned: BTreeMap<String, f64>,
    pledged: BTreeMap<String, f64>,
    intents: Vec<Intent>,
}
impl Book {
    fn post(
        &mut self,
        event_id: &str,
        ordinal: i32,
        phase: u32,
        sequence: u32,
        lines: Vec<(Kind, &str, f64)>,
    ) -> Result<(), Refusal> {
        let memo = lines.iter().all(|l| l.0.memo());
        if !memo && lines.iter().any(|l| l.0.memo()) {
            return Err(Refusal::new(
                UNSUPPORTED_POLICY,
                format!("`{event_id}` mixes custody memo and financial lines"),
            ));
        }
        let total: f64 = lines.iter().map(|l| l.2).sum();
        if !near(total, 0.) {
            return Err(Refusal::new(
                UNSUPPORTED_POLICY,
                format!("`{event_id}` does not balance"),
            ));
        }
        let lines: Vec<Line> = lines
            .into_iter()
            .filter(|l| l.2 != 0.)
            .map(|(kind, instrument, debit)| Line {
                kind,
                instrument: instrument.into(),
                debit,
            })
            .collect();
        if lines.is_empty() {
            return Ok(());
        }
        for l in &lines {
            let book = if memo {
                &mut self.custody
            } else {
                &mut self.financial
            };
            *book.entry((l.kind, l.instrument.clone())).or_default() += l.debit;
            if l.kind == Kind::Cash {
                self.cash += l.debit;
                self.settled += l.debit;
            }
        }
        if self.cash < -1e-9 {
            return Err(Refusal::new(
                INSUFFICIENT_SETTLEMENT_LIQUIDITY,
                format!("`{event_id}` leaves cash at {}; no cash plug or emergency facility is invented", self.cash),
            ));
        }
        self.intents.push(Intent {
            event_id: event_id.into(),
            ordinal,
            phase,
            sequence,
            memo,
            lines,
        });
        Ok(())
    }
    fn closing(&self, ordinal: i32, lots: &[&PositionLot]) -> Closing {
        let sum = |book: &BTreeMap<(Kind, String), f64>| {
            let mut out: BTreeMap<Kind, f64> = BTreeMap::new();
            for ((k, _), v) in book {
                *out.entry(*k).or_default() += v;
            }
            out.into_iter().collect::<Vec<_>>()
        };
        Closing {
            ordinal,
            financial: sum(&self.financial),
            custody: sum(&self.custody),
            cash: self.cash,
            usable_cash: self.cash - self.restricted,
            inventory: lots
                .iter()
                .filter(|l| l.kind == LotKind::Security)
                .map(|l| {
                    let owned = self.owned.get(&l.lot_id).copied().unwrap_or(0.);
                    let pledged = self.pledged.get(&l.lot_id).copied().unwrap_or(0.);
                    Inventory {
                        lot_id: l.lot_id.clone(),
                        owned,
                        pledged,
                        outside_encumbrance: l.outside_encumbrance,
                        available: owned - pledged - l.outside_encumbrance,
                    }
                })
                .collect(),
        }
    }
}

pub fn replay(
    plan: &Admitted,
    acq: &AcquisitionRequest,
    hedges: &[ResolvedHedge],
    account_id: &str,
) -> Result<AcquisitionJournal, Refusal> {
    let r = &plan.request;
    let account = r
        .accounts
        .iter()
        .find(|a| a.account_id == account_id)
        .ok_or_else(|| {
            Refusal::new(
                LIFECYCLE_MAPPING_REQUIRED,
                format!("unknown account `{account_id}`"),
            )
        })?;
    let asof = r.as_of_ordinal;
    if acq.horizon_end_ordinal < asof || acq.horizon_end_ordinal - asof > MAX_ACQUISITION_DAYS {
        return Err(Refusal::new(
            BOUND_EXCEEDED,
            "acquisition horizon must lie within 366 days of the as-of date",
        ));
    }
    let lots: HashMap<&str, &PositionLot> = r.lots.iter().map(|l| (l.lot_id.as_str(), l)).collect();
    let agreements: HashMap<&str, &MarginAgreement> = r
        .margin_agreements
        .iter()
        .map(|a| (a.agreement_id.as_str(), a))
        .collect();
    let pledges: HashMap<&str, &Pledge> = r
        .pledges
        .iter()
        .map(|p| (p.pledge_id.as_str(), p))
        .collect();
    let lot = |id: &str, what: &str| -> Result<&PositionLot, Refusal> {
        let l = lots.get(id).copied().ok_or_else(|| {
            Refusal::new(
                LIFECYCLE_MAPPING_REQUIRED,
                format!("{what} names unknown lot `{id}`"),
            )
        })?;
        if l.account_id != account_id {
            return Err(Refusal::new(
                LIFECYCLE_MAPPING_REQUIRED,
                format!("{what} lot `{id}` belongs to another account"),
            ));
        }
        Ok(l)
    };
    // Formal treatment of an acquisition needs the #35 owner's intents bound to this journal.
    // This stage replays economic (undesignated) acquisitions only; it never falls back.
    let touched: Vec<&str> = acq
        .purchases
        .iter()
        .map(|p| p.lot_id.as_str())
        .chain(acq.financings.iter().map(|f| f.lot_id.as_str()))
        .chain(acq.hedge_entries.iter().map(|h| h.lot_id.as_str()))
        .collect();
    for a in &r.actions {
        if a.accounting_treatment == AccountingTreatment::Formal
            && a.lots.iter().any(|x| touched.contains(&x.lot_id.as_str()))
        {
            return Err(Refusal::new(FORMAL_OWNER_REQUIRED, format!(
                "action `{}` selects formal hedge accounting for an acquired lot; this stage binds no hedge-accounting-1 intents and does not fall back to undesignated treatment",
                a.action_id)));
        }
    }

    // Validate each instruction against the plan and build the dated event list.
    #[derive(Clone)]
    enum Event<'a> {
        Advance(&'a FinancingAdvance, &'a PositionLot),
        Purchase(&'a SecurityPurchase, &'a PositionLot, f64, f64),
        Pledge(&'a Pledge),
        Entry(&'a HedgeEntry, &'a PositionLot, f64),
        VmSettle(&'a HedgeEntry, &'a MarginAgreement, f64),
        Im(&'a SuppliedInitialMargin, &'a MarginAgreement),
    }
    let mut events: Vec<(i32, u32, Event)> = Vec::new();
    let mut purchased: HashMap<&str, &SecurityPurchase> = HashMap::new();
    for p in &acq.purchases {
        let l = lot(&p.lot_id, "purchase")?;
        if l.kind != LotKind::Security
            || l.acquisition_ordinal != p.settlement_ordinal
            || p.settlement_ordinal <= asof
        {
            return Err(Refusal::new(UNSUPPORTED_POLICY, format!(
                "purchase `{}` must acquire a new security lot on its settlement date after the as-of date", p.lot_id)));
        }
        if p.trade_ordinal < asof
            || p.trade_ordinal > p.settlement_ordinal
            || p.settlement_ordinal > acq.horizon_end_ordinal
        {
            return Err(Refusal::new(
                MALFORMED_INPUT,
                format!(
                    "purchase `{}` dates are out of order or beyond the horizon",
                    p.lot_id
                ),
            ));
        }
        let price = money(p.clean_price_per_100, "clean price")?;
        let accrued = money(p.accrued_interest, "accrued interest")?;
        let fees = money(p.fees, "fees")?;
        if price <= 0. || accrued < 0. || fees < 0. {
            return Err(Refusal::new(
                MALFORMED_INPUT,
                format!(
                    "purchase `{}` needs a positive price and nonnegative accrued/fees",
                    p.lot_id
                ),
            ));
        }
        let cost = price / 100. * l.quantity;
        if !near(l.original_cost, cost + fees) {
            return Err(Refusal::new(UNSUPPORTED_POLICY, format!(
                "purchase `{}` original cost {} must equal clean cost {cost} plus fees {fees}: no purchase-price gain is manufactured", p.lot_id, l.original_cost)));
        }
        if purchased.insert(p.lot_id.as_str(), p).is_some() {
            return Err(Refusal::new(
                DUPLICATE_IDENTITY,
                format!("lot `{}` is purchased twice", p.lot_id),
            ));
        }
        events.push((
            p.settlement_ordinal,
            2,
            Event::Purchase(p, l, cost, accrued),
        ));
    }
    let mut advanced: HashMap<&str, ()> = HashMap::new();
    for f in &acq.financings {
        let l = lot(&f.lot_id, "financing")?;
        if l.kind != LotKind::Funding
            || l.acquisition_ordinal != f.advance_ordinal
            || f.advance_ordinal <= asof
        {
            return Err(Refusal::new(UNSUPPORTED_POLICY, format!(
                "financing `{}` must advance a new funding lot on its advance date; an opening contract is never advanced again", f.lot_id)));
        }
        if !near(money(f.principal, "principal")?, l.quantity)
            || f.principal <= 0.
            || f.advance_ordinal > acq.horizon_end_ordinal
        {
            return Err(Refusal::new(
                MALFORMED_INPUT,
                format!(
                    "financing `{}` principal must equal the funding lot quantity",
                    f.lot_id
                ),
            ));
        }
        let pledge = pledges.get(f.pledge_id.as_str()).copied().ok_or_else(|| {
            Refusal::new(
                LIFECYCLE_MAPPING_REQUIRED,
                format!(
                    "financing `{}` names unknown pledge `{}`",
                    f.lot_id, f.pledge_id
                ),
            )
        })?;
        if pledge.agreement_id != f.lot_id {
            return Err(Refusal::new(
                LIFECYCLE_MAPPING_REQUIRED,
                format!(
                    "pledge `{}` does not secure funding lot `{}`",
                    f.pledge_id, f.lot_id
                ),
            ));
        }
        let collateral = lot(&pledge.lot_id, "pledge")?;
        if collateral.acquisition_ordinal > f.advance_ordinal {
            return Err(Refusal::new(
                INVENTORY_UNAVAILABLE,
                format!(
                    "pledge `{}` delivers lot `{}` before it is owned",
                    f.pledge_id, pledge.lot_id
                ),
            ));
        }
        if advanced.insert(f.lot_id.as_str(), ()).is_some() {
            return Err(Refusal::new(
                DUPLICATE_IDENTITY,
                format!("funding lot `{}` is advanced twice", f.lot_id),
            ));
        }
        events.push((f.advance_ordinal, 1, Event::Advance(f, l)));
        events.push((f.advance_ordinal, 3, Event::Pledge(pledge)));
    }
    let resolved: HashMap<&str, &ResolvedHedge> =
        hedges.iter().map(|h| (h.lot_id.as_str(), h)).collect();
    let mut entered: Vec<&str> = Vec::new();
    for h in &acq.hedge_entries {
        let l = lot(&h.lot_id, "hedge entry")?;
        if l.kind != LotKind::Derivative
            || l.acquisition_ordinal != h.entry_ordinal
            || h.entry_ordinal <= asof
            || h.entry_ordinal > acq.horizon_end_ordinal
        {
            return Err(Refusal::new(
                UNSUPPORTED_POLICY,
                format!(
                    "hedge entry `{}` must enter a new derivative lot on its entry date",
                    h.lot_id
                ),
            ));
        }
        let agreement = agreements
            .get(h.agreement_id.as_str())
            .copied()
            .ok_or_else(|| {
                Refusal::new(
                    LIFECYCLE_MAPPING_REQUIRED,
                    format!("hedge entry `{}` names unknown agreement", h.lot_id),
                )
            })?;
        if !agreement.lot_ids.contains(&h.lot_id) {
            return Err(Refusal::new(
                LIFECYCLE_MAPPING_REQUIRED,
                format!(
                    "agreement `{}` does not scope lot `{}`",
                    h.agreement_id, h.lot_id
                ),
            ));
        }
        let value = resolved.get(h.lot_id.as_str()).copied().ok_or_else(|| {
            Refusal::new(
                LIFECYCLE_MAPPING_REQUIRED,
                format!("hedge entry `{}` has no native entry valuation", h.lot_id),
            )
        })?;
        let clean = money(value.clean_value, "entry clean value")?;
        if !near(
            money(h.upfront_consideration, "upfront consideration")?,
            clean,
        ) {
            return Err(Refusal::new(UNSUPPORTED_POLICY, format!(
                "hedge entry `{}` clean value {clean} needs equal upfront consideration {}; no day-one gain or loss is created",
                h.lot_id, h.upfront_consideration)));
        }
        let vm = &h.variation_margin;
        if money(vm.receive_threshold, "receive threshold")? < 0.
            || money(vm.post_threshold, "post threshold")? < 0.
            || money(vm.minimum_transfer, "minimum transfer")? < 0.
        {
            return Err(Refusal::new(
                MALFORMED_INPUT,
                "variation margin terms must be nonnegative",
            ));
        }
        if entered.contains(&h.lot_id.as_str()) {
            return Err(Refusal::new(
                DUPLICATE_IDENTITY,
                format!("derivative lot `{}` is entered twice", h.lot_id),
            ));
        }
        entered.push(&h.lot_id);
        events.push((h.entry_ordinal, 4, Event::Entry(h, l, clean)));
        // Exposure above a threshold is called at entry and settles after the agreement's lag.
        let call = if clean > vm.receive_threshold {
            clean - vm.receive_threshold
        } else if clean < -vm.post_threshold {
            clean + vm.post_threshold
        } else {
            0.
        };
        if call != 0. && call.abs() >= vm.minimum_transfer {
            let settle = h.entry_ordinal + agreement.settlement_lag_days as i32;
            if settle > acq.horizon_end_ordinal {
                return Err(Refusal::new(UNSUPPORTED_POLICY, format!(
                    "hedge entry `{}` variation margin settles beyond the horizon; the complete collateral tail is required", h.lot_id)));
            }
            events.push((settle, 5, Event::VmSettle(h, agreement, call)));
        }
    }
    for im in &acq.initial_margin {
        let agreement = agreements
            .get(im.agreement_id.as_str())
            .copied()
            .ok_or_else(|| {
                Refusal::new(
                    LIFECYCLE_MAPPING_REQUIRED,
                    format!(
                        "initial margin names unknown agreement `{}`",
                        im.agreement_id
                    ),
                )
            })?;
        if agreement.initial_margin != InitialMarginMode::SuppliedRequirements {
            return Err(Refusal::new(
                UNSUPPORTED_POLICY,
                format!(
                    "agreement `{}` does not select supplied IM requirements",
                    im.agreement_id
                ),
            ));
        }
        let mut portfolio = agreement.lot_ids.clone();
        portfolio.sort();
        let mut supplied = im.portfolio_lot_ids.clone();
        supplied.sort();
        let entry = acq
            .hedge_entries
            .iter()
            .find(|h| h.agreement_id == im.agreement_id);
        let bound = supplied == portfolio
            && im.scenario_id == acq.scenario_id
            && entry.is_some_and(|h| h.entry_ordinal == im.ordinal);
        if !bound {
            return Err(Refusal::new(MARGIN_REFRESH_REQUIRED, format!(
                "initial margin for `{}` is not bound to the exact live portfolio, scenario and entry date", im.agreement_id)));
        }
        if money(im.posted_requirement, "posted IM")? < 0.
            || money(im.received_requirement, "received IM")? < 0.
        {
            return Err(Refusal::new(
                MALFORMED_INPUT,
                "initial margin requirements must be nonnegative",
            ));
        }
        if agreement.segregation != Segregation::Segregated && im.received_requirement > 0. {
            return Err(Refusal::new(
                UNSUPPORTED_POLICY,
                "received initial margin is custody memo only under a segregated agreement",
            ));
        }
        events.push((im.ordinal, 6, Event::Im(im, agreement)));
    }
    let agreement_with_entry: Vec<&str> = acq
        .hedge_entries
        .iter()
        .map(|h| h.agreement_id.as_str())
        .collect();
    for a in &agreement_with_entry {
        if agreements[a].initial_margin == InitialMarginMode::SuppliedRequirements
            && !acq.initial_margin.iter().any(|im| im.agreement_id == *a)
        {
            return Err(Refusal::new(
                MARGIN_REFRESH_REQUIRED,
                format!(
                    "agreement `{a}` requires a supplied IM requirement for the entered portfolio"
                ),
            ));
        }
    }
    events.sort_by_key(|(ordinal, phase, _)| (*ordinal, *phase));

    // Opening: supplied once. Opening lots of this account carry no acquisition events here.
    let mut book = Book {
        financial: BTreeMap::new(),
        custody: BTreeMap::new(),
        cash: account.cash,
        restricted: 0.,
        settled: 0.,
        owned: r
            .lots
            .iter()
            .filter(|l| l.account_id == account_id && l.acquisition_ordinal <= asof)
            .map(|l| (l.lot_id.clone(), l.quantity))
            .collect(),
        pledged: BTreeMap::new(),
        intents: Vec::new(),
    };
    for p in &r.pledges {
        if let Some(l) = lots.get(p.lot_id.as_str()) {
            if l.account_id == account_id && l.acquisition_ordinal <= asof {
                *book.pledged.entry(p.lot_id.clone()).or_default() += p.quantity;
            }
        }
    }
    book.financial
        .insert((Kind::Cash, String::new()), account.cash);
    // The owner opening is its cash against its own equity share: this stage posts no opening
    // stocks, and the shared Sim posts the account cash and equity exactly once.
    book.financial
        .insert((Kind::OpeningEquity, String::new()), -account.cash);
    let account_lots: Vec<&PositionLot> = r
        .lots
        .iter()
        .filter(|l| l.account_id == account_id)
        .collect();
    let mut daily = Vec::new();
    let (mut fees_total, mut posted_vm, mut received_vm, mut posted_im, mut received_im) =
        (0., 0., 0., 0., 0.);
    let mut cursor = 0;
    let mut sequence = 0u32;
    for ordinal in asof..=acq.horizon_end_ordinal {
        while cursor < events.len() && events[cursor].0 == ordinal {
            let (_, phase, event) = &events[cursor];
            sequence += 1;
            match event {
                Event::Advance(f, l) => {
                    book.post(
                        &format!("financing_advance:{}", f.lot_id),
                        ordinal,
                        *phase,
                        sequence,
                        vec![
                            (Kind::Cash, "", f.principal),
                            (Kind::FundingPrincipal, &l.lot_id, -f.principal),
                        ],
                    )?;
                }
                Event::Purchase(p, l, cost, accrued) => {
                    // Reservation at trade date created the obligation; settlement moves cash and
                    // title together under delivery versus payment.
                    book.post(
                        &format!("security_purchase:{}", p.lot_id),
                        ordinal,
                        *phase,
                        sequence,
                        vec![
                            (Kind::AssetPrincipal, &l.lot_id, l.quantity),
                            (Kind::BookAdjustment, &l.lot_id, cost - l.quantity),
                            (Kind::InterestReceivable, &l.lot_id, *accrued),
                            (Kind::FeeExpense, &l.lot_id, p.fees),
                            (Kind::Cash, "", -(cost + accrued + p.fees)),
                        ],
                    )?;
                    fees_total += p.fees;
                    *book.owned.entry(l.lot_id.clone()).or_default() += l.quantity;
                }
                Event::Pledge(p) => {
                    let l = lots[p.lot_id.as_str()];
                    let owned = book.owned.get(&p.lot_id).copied().unwrap_or(0.);
                    let pledged = book.pledged.get(&p.lot_id).copied().unwrap_or(0.);
                    let available = owned - pledged - l.outside_encumbrance;
                    if p.quantity > available + 1e-9 * l.quantity.max(1.) {
                        return Err(Refusal::new(INVENTORY_UNAVAILABLE, format!(
                            "pledge `{}` needs {} of lot `{}` but only {available} is unencumbered and owned", p.pledge_id, p.quantity, p.lot_id)));
                    }
                    *book.pledged.entry(p.lot_id.clone()).or_default() += p.quantity;
                }
                Event::Entry(h, l, clean) => {
                    let (kind, debit) = if *clean >= 0. {
                        (Kind::DerivativeAsset, *clean)
                    } else {
                        (Kind::DerivativeLiability, *clean)
                    };
                    book.post(
                        &format!("hedge_entry:{}", h.lot_id),
                        ordinal,
                        *phase,
                        sequence,
                        vec![
                            (kind, &l.lot_id, debit),
                            (Kind::Cash, "", -h.upfront_consideration),
                        ],
                    )?;
                }
                Event::VmSettle(h, agreement, call) => {
                    let id = format!("variation_margin:{}", h.lot_id);
                    if *call > 0. {
                        book.post(
                            &id,
                            ordinal,
                            *phase,
                            sequence,
                            vec![
                                (Kind::Cash, "", *call),
                                (Kind::ReceivedVm, &agreement.agreement_id, -*call),
                            ],
                        )?;
                        received_vm += call;
                        if agreement.segregation == Segregation::Segregated {
                            book.restricted += call;
                        }
                    } else {
                        book.post(
                            &id,
                            ordinal,
                            *phase,
                            sequence,
                            vec![
                                (Kind::PostedVm, &agreement.agreement_id, -*call),
                                (Kind::Cash, "", *call),
                            ],
                        )?;
                        posted_vm -= call;
                    }
                }
                Event::Im(im, agreement) => {
                    if im.posted_requirement > 0. {
                        book.post(
                            &format!("initial_margin_posted:{}", agreement.agreement_id),
                            ordinal,
                            *phase,
                            sequence,
                            vec![
                                (
                                    Kind::PostedIm,
                                    &agreement.agreement_id,
                                    im.posted_requirement,
                                ),
                                (Kind::Cash, "", -im.posted_requirement),
                            ],
                        )?;
                        posted_im += im.posted_requirement;
                    }
                    if im.received_requirement > 0. {
                        sequence += 1;
                        book.post(
                            &format!("initial_margin_received:{}", agreement.agreement_id),
                            ordinal,
                            *phase,
                            sequence,
                            vec![
                                (
                                    Kind::CustodyAsset,
                                    &agreement.agreement_id,
                                    im.received_requirement,
                                ),
                                (
                                    Kind::CustodyObligation,
                                    &agreement.agreement_id,
                                    -im.received_requirement,
                                ),
                            ],
                        )?;
                        received_im += im.received_requirement;
                    }
                }
            }
            cursor += 1;
        }
        daily.push(book.closing(ordinal, &account_lots));
    }
    let stock = |kind: Kind| -> f64 {
        book.financial
            .iter()
            .filter(|((k, _), _)| *k == kind)
            .map(|(_, v)| v)
            .sum()
    };
    let custody = |kind: Kind| -> f64 {
        book.custody
            .iter()
            .filter(|((k, _), _)| *k == kind)
            .map(|(_, v)| v)
            .sum()
    };
    let cash_bridge =
        near(account.cash + book.settled, book.cash) && near(stock(Kind::Cash), book.cash);
    let inventory = daily.last().is_some_and(|d| {
        d.inventory.iter().all(|i| {
            i.available >= -1e-9 && near(i.owned - i.pledged - i.outside_encumbrance, i.available)
        })
    });
    let collateral = near(stock(Kind::PostedVm), posted_vm)
        && near(-stock(Kind::ReceivedVm), received_vm)
        && near(stock(Kind::PostedIm), posted_im)
        && near(custody(Kind::CustodyAsset), received_im)
        && near(-custody(Kind::CustodyObligation), received_im);
    let receipt = Receipt {
        opening_cash: account.cash,
        closing_cash: book.cash,
        settled_cash_movements: book.settled,
        gross_derivative_assets: stock(Kind::DerivativeAsset),
        gross_derivative_liabilities: -stock(Kind::DerivativeLiability),
        posted_vm,
        received_vm,
        posted_im,
        received_im_custody: received_im,
        fees_expensed: fees_total,
        cash_bridge_reconciled: cash_bridge,
        inventory_conserved: inventory,
        collateral_conserved: collateral,
        formal_hedge_accounting: false,
    };
    if !(cash_bridge && inventory && collateral) {
        return Err(Refusal::new(
            UNSUPPORTED_POLICY,
            "acquisition replay failed its own conservation receipt",
        ));
    }
    Ok(AcquisitionJournal {
        account_id: account_id.into(),
        scenario_id: acq.scenario_id.clone(),
        as_of_ordinal: asof,
        tail_ordinal: acq.horizon_end_ordinal,
        opening_cash: account.cash,
        opening_equity: account.equity,
        intents: book.intents,
        daily,
        receipt,
    })
}
