//! Contained supplied-policy preparation. No source generator or ledger authority.
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

pub const SCHEMA: &str = "modeled-product-ftp-source-channels-1";
pub const LANES: [&str; 18] = [
    "active_principal",
    "pending_default_principal",
    "pending_prepay_principal",
    "default_claim_principal",
    "live_interest_receivable",
    "live_interest_payable",
    "default_interest_receivable",
    "default_interest_payable",
    "principal_basis",
    "live_interest_basis",
    "default_principal_basis",
    "default_interest_basis",
    "borrower_principal_memo",
    "borrower_interest_memo",
    "economic_principal_memo",
    "economic_interest_memo",
    "undrawn_memo",
    "allowance",
];
pub const INCOME_LANES: [&str; 7] = [
    "original_eir",
    "reviewed_basis",
    "incurred_credit",
    "recovery_income",
    "recovery_cost",
    "initial_allowance",
    "deposit_revision",
];
pub const MAX_RAW_BYTES: usize = 1_048_576;
pub const MAX_OUTPUT_BYTES: usize = 1_048_576;
const MAX_EVENTS: usize = 4096;
const MAX_WORK: usize = 2_000_000;
/// Separate source2 tier. All financial calculations below are shared verbatim.
pub const PORTFOLIO_SCHEMA: &str = "modeled-product-ftp-source-channels-2";
pub const PORTFOLIO_MAX_DAYS: usize = 365;
pub const PORTFOLIO_MAX_WORK: usize = 8_000_000;
pub const PORTFOLIO_MAX_BYTES: usize = 8 * 1024 * 1024;
struct Tier {
    schema: &'static str,
    days: usize,
    origins: usize,
    resets: usize,
    work: usize,
    bytes: usize,
    output: usize,
}
const LEGACY: Tier = Tier {
    schema: SCHEMA,
    days: 64,
    origins: 128,
    resets: 65,
    work: MAX_WORK,
    bytes: MAX_RAW_BYTES,
    output: MAX_OUTPUT_BYTES,
};
const PORTFOLIO: Tier = Tier {
    schema: PORTFOLIO_SCHEMA,
    days: PORTFOLIO_MAX_DAYS,
    origins: 368,
    resets: 366,
    work: PORTFOLIO_MAX_WORK,
    bytes: PORTFOLIO_MAX_BYTES,
    output: PORTFOLIO_MAX_BYTES,
};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Identity {
    pub id: String,
    pub revision: String,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ChannelPolicy {
    pub channel: String,
    pub coefficient: i8,
    pub review: Identity,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Policy {
    pub review: Identity,
    pub accrual_basis: String,
    pub averaging: String,
    pub negative_funding: String,
    pub principal_zero_rights: String,
    pub termination: String,
    pub tail: String,
    pub channels: Vec<ChannelPolicy>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Amount {
    pub channel: String,
    pub amount: f64,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Origin {
    pub origin_id: String,
    pub source_event_id: String,
    pub channel: String,
    pub amount: f64,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct State {
    pub ordinal: i32,
    pub source_checkpoint_id: String,
    pub channels: Vec<Amount>,
    pub origins: Vec<Origin>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Cash {
    pub event_id: String,
    pub ordinal: i32,
    pub kind: String,
    pub bank_cash: f64,
    pub funding_change_sequence: Option<u32>,
    pub exclusion_review: Option<Identity>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Change {
    pub sequence: u32,
    pub ordinal: i32,
    pub source_event_id: String,
    pub kind: String,
    pub amount: f64,
    pub cash_event_id: Option<String>,
    pub review: Identity,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Earned {
    pub event_id: String,
    pub ordinal: i32,
    pub channel: String,
    /// Signed bank profit, never the sign of a cash movement.
    pub bank_earnings: f64,
    pub review: Identity,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Reset {
    pub ordinal: i32,
    pub reference_years: f64,
    pub liquidity_years: f64,
    pub review: Identity,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Request {
    pub schema: String,
    pub measure: String,
    pub source: Identity,
    pub original_yield_anchor: Identity,
    pub path_id: String,
    pub entity: String,
    pub currency_unit: String,
    pub scenario_id: String,
    pub side: String,
    pub start_ordinal: i32,
    pub full_end_ordinal: i32,
    pub source_weight: f64,
    pub unit_face: f64,
    pub allocated_face: f64,
    pub opening_funding_balance: f64,
    pub policy: Policy,
    pub states: Vec<State>,
    pub cash: Vec<Cash>,
    pub changes: Vec<Change>,
    pub earned: Vec<Earned>,
    pub resets: Vec<Reset>,
}
#[derive(Debug, Serialize)]
pub struct Day {
    pub ordinal: i32,
    pub direction: f64,
    pub opening_funding_balance: f64,
    pub closing_funding_balance: f64,
    pub average_funding_balance: f64,
    pub scaled_average_funding_balance: f64,
    pub accrual_fraction: f64,
    pub reference_years: f64,
    pub liquidity_years: f64,
    pub reset_review: Identity,
    pub funding_change_sequences: Vec<u32>,
    pub cash_event_ids: Vec<String>,
    pub earned_event_ids: Vec<String>,
    pub bank_cash: f64,
    pub original_eir: f64,
    pub earnings_by_channel: Vec<Amount>,
    pub external_earnings: f64,
}
#[derive(Debug, Serialize)]
pub struct ResultData {
    pub schema: &'static str,
    pub raw_request: Request,
    pub scale: f64,
    pub days: Vec<Day>,
    pub closing_unweighted_channels: Vec<Amount>,
    pub closing_funding_balance: f64,
    pub admitted_work: usize,
    pub complete_unweighted_tail_checked: bool,
    pub source_reconstructed: bool,
    pub ordered_journal_verified: bool,
    pub ftp_bridge_adoption: bool,
    pub capital_ftp_eligible: bool,
    pub accounting_standard_approval: bool,
    pub empirical_validation: bool,
    pub treasury_nonnegative_positions_compatible: bool,
}
fn text(s: &str) -> Result<(), String> {
    if s.trim().is_empty() || s.len() > 200 || s.chars().any(char::is_control) {
        Err("bounded explicit label required".into())
    } else {
        Ok(())
    }
}
fn identity(x: &Identity) -> Result<(), String> {
    text(&x.id)?;
    text(&x.revision)
}
fn money(x: f64) -> Result<(), String> {
    if !x.is_finite() || x.abs() > 10_000. {
        Err("finite component amount <=10000 required".into())
    } else {
        Ok(())
    }
}
fn near(a: f64, b: f64) -> bool {
    (a - b).abs() <= 1e-9 + 1e-12 * b.abs()
}
fn check_ordinal(d: i32) -> Result<(), String> {
    if !(1..=3_652_059).contains(&d) {
        Err("exact bounded ordinal required".into())
    } else {
        Ok(())
    }
}
fn literal(s: &str, options: &[&str]) -> Result<(), String> {
    if !options.contains(&s) {
        Err(format!("unsupported explicit policy {s}"))
    } else {
        Ok(())
    }
}
fn gross_lane(i: usize) -> bool {
    !matches!(i, 8..=11)
}
fn funding(state: &State, policy: &Policy) -> f64 {
    state
        .channels
        .iter()
        .zip(&policy.channels)
        .map(|(a, p)| a.amount * f64::from(p.coefficient))
        .sum()
}
fn serialized_size<T: Serialize>(v: &T, cap: usize) -> Result<(), String> {
    struct Counter {
        n: usize,
        cap: usize,
    }
    impl std::io::Write for Counter {
        fn write(&mut self, b: &[u8]) -> std::io::Result<usize> {
            self.n = self
                .n
                .checked_add(b.len())
                .filter(|x| *x <= self.cap)
                .ok_or_else(|| std::io::Error::other("bounded serialized capacity exceeded"))?;
            Ok(b.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }
    serde_json::to_writer(Counter { n: 0, cap }, v).map_err(|e| e.to_string())
}
pub fn prepare(r: &Request) -> Result<ResultData, String> {
    prepare_with_check(r, &mut || Ok(()))
}
pub fn prepare_with_check(
    r: &Request,
    check: &mut impl FnMut() -> Result<(), String>,
) -> Result<ResultData, String> {
    prepare_in_tier(r, check, &LEGACY)
}
pub fn prepare_portfolio_with_check(
    r: &Request,
    check: &mut impl FnMut() -> Result<(), String>,
) -> Result<ResultData, String> {
    prepare_in_tier(r, check, &PORTFOLIO)
}
fn prepare_in_tier(
    r: &Request,
    check: &mut impl FnMut() -> Result<(), String>,
    tier: &Tier,
) -> Result<ResultData, String> {
    check()?;
    serialized_size(r, tier.bytes)?;
    if r.schema != tier.schema || r.measure != "physical" {
        return Err("explicit physical proof schema required".into());
    }
    for s in [&r.path_id, &r.entity, &r.currency_unit, &r.scenario_id] {
        text(s)?;
    }
    for id in [&r.source, &r.original_yield_anchor, &r.policy.review] {
        identity(id)?;
    }
    check_ordinal(r.start_ordinal)?;
    check_ordinal(r.full_end_ordinal)?;
    let n = usize::try_from(r.full_end_ordinal - r.start_ordinal).map_err(|_| "negative clock")?;
    if !(1..=tier.days).contains(&n) || r.states.len() != n + 1 {
        return Err(if tier.schema == SCHEMA {
            "complete consecutive <=64 day states required"
        } else {
            "complete consecutive <=365 day source2 states required"
        }
        .into());
    }
    literal(&r.side, &["asset", "liability"])?;
    if r.policy.accrual_basis != "ACT/365F" {
        return Err("explicit ACT/365F funding accrual basis required".into());
    }
    literal(&r.policy.averaging, &["opening_daily", "trapezoid_daily"])?;
    literal(
        &r.policy.negative_funding,
        &["reject_negative", "research_signed"],
    )?;
    literal(
        &r.policy.principal_zero_rights,
        &["reject_unsettled", "explicit_reviewed_all_channels"],
    )?;
    if r.policy.termination != "complete_gross_rights_and_commitment"
        || r.policy.tail != "no_synthetic_cash_or_tail"
    {
        return Err("explicit complete gross termination/tail required".into());
    }
    if r.policy.channels.len() != LANES.len() {
        return Err("all funding channel policies required".into());
    }
    for (i, p) in r.policy.channels.iter().enumerate() {
        if p.channel != LANES[i] || !(-1..=1).contains(&p.coefficient) {
            return Err("ordered explicit channel coefficients required".into());
        }
        // Gross memo and allowance lanes are preserved but cannot become liquidity.
        if i >= 12 && p.coefficient != 0 {
            return Err("memo/allowance cannot be funding exposure".into());
        }
        identity(&p.review)?;
    }
    money(r.unit_face)?;
    money(r.allocated_face)?;
    money(r.opening_funding_balance)?;
    if r.unit_face <= 0.
        || r.allocated_face < 0.
        || !r.source_weight.is_finite()
        || !(0. ..=1.).contains(&r.source_weight)
    {
        return Err("explicit bounded monetary scale and physical path weight required".into());
    }
    let scale = r.allocated_face / r.unit_face * r.source_weight;
    if !scale.is_finite() || scale > 10_000. {
        return Err("monetary scale bound".into());
    }
    let events = r
        .cash
        .len()
        .checked_add(r.changes.len())
        .and_then(|x| x.checked_add(r.earned.len()))
        .ok_or("event count overflow")?;
    if events > MAX_EVENTS || r.resets.is_empty() || r.resets.len() > tier.resets {
        return Err("bounded complete event/reset domain".into());
    }
    // Includes stable repeated scans during daily preparation; no source-generation
    // work is hidden in this independently bounded analytical component.
    if r.states.iter().any(|s| s.origins.len() > tier.origins) {
        return Err("bounded complete source-origin inventory required".into());
    }
    let origins = r.states.iter().try_fold(0usize, |a, s| {
        a.checked_add(s.origins.len()).ok_or("origin overflow")
    })?;
    let states = n.checked_add(1).ok_or("state overflow")?;
    let event_work = events
        .checked_mul(states)
        .and_then(|x| x.checked_mul(20))
        .ok_or("event work overflow")?;
    let origin_work = origins.checked_mul(80).ok_or("origin work overflow")?;
    let reset_work = r
        .resets
        .len()
        .checked_mul(16)
        .ok_or("reset work overflow")?;
    let work = states
        .checked_mul(18 * 16)
        .and_then(|x| x.checked_add(event_work))
        .and_then(|x| x.checked_add(origin_work))
        .and_then(|x| x.checked_add(reset_work))
        .filter(|x| *x <= tier.work)
        .ok_or("pre-admitted work bound")?;
    let mut checkpoint_ids = BTreeSet::new();
    for (d, s) in r.states.iter().enumerate() {
        check()?;
        if s.ordinal != r.start_ordinal + d as i32 || s.channels.len() != LANES.len() {
            return Err("exact consecutive complete state domain".into());
        }
        text(&s.source_checkpoint_id)?;
        if !checkpoint_ids.insert(&s.source_checkpoint_id) {
            return Err("duplicate source checkpoint identity".into());
        }
        for (i, a) in s.channels.iter().enumerate() {
            if a.channel != LANES[i] {
                return Err("ordered complete gross channels required".into());
            }
            money(a.amount)?;
            if gross_lane(i) && a.amount < 0. {
                return Err("gross state lane must be nonnegative".into());
            }
        }
        let mut origin_keys = BTreeSet::new();
        for origin in &s.origins {
            text(&origin.origin_id)?;
            text(&origin.source_event_id)?;
            money(origin.amount)?;
            let i = LANES
                .iter()
                .position(|x| *x == origin.channel)
                .ok_or("unknown origin channel")?;
            if !origin_keys.insert((&origin.channel, &origin.origin_id))
                || gross_lane(i) && origin.amount < 0.
            {
                return Err("source origin identity/gross domain".into());
            }
        }
        for a in &s.channels {
            let total: f64 = s
                .origins
                .iter()
                .filter(|o| o.channel == a.channel)
                .map(|o| o.amount)
                .sum();
            if !near(total, a.amount) {
                return Err("complete channel/origin ownership reconciliation".into());
            }
        }
        let principal: f64 = s.channels[..4].iter().map(|a| a.amount).sum();
        if r.policy.principal_zero_rights == "reject_unsettled"
            && near(principal, 0.)
            && s.channels[4..].iter().any(|a| !near(a.amount, 0.))
        {
            return Err("principal-zero rights/basis require explicit reviewed ownership".into());
        }
        let b = funding(s, &r.policy);
        money(b)?;
        if r.policy.negative_funding == "reject_negative" && b < 0. {
            return Err("negative funding balance rejected by review".into());
        }
        money(b * scale)?;
    }
    if !near(funding(&r.states[0], &r.policy), r.opening_funding_balance) {
        return Err("opening funding/state allocation mismatch".into());
    }
    if r.states[n].channels.iter().any(|a| !near(a.amount, 0.)) {
        return Err("unweighted gross rights/basis/memo/commitment tail remains".into());
    }
    if r.states[n].origins.iter().any(|o| !near(o.amount, 0.)) {
        return Err("unweighted individual origin tail remains".into());
    }
    let side = if r.side == "asset" { 1. } else { -1. };
    let mut cash_map = BTreeMap::new();
    for c in &r.cash {
        check()?;
        text(&c.event_id)?;
        money(c.bank_cash)?;
        literal(
            &c.kind,
            &[
                "acquisition",
                "principal",
                "interest",
                "gross_recovery",
                "source_atomic_cash",
                "recovery_cost",
            ],
        )?;
        if !(r.start_ordinal < c.ordinal && c.ordinal <= r.full_end_ordinal)
            || cash_map.insert(c.event_id.as_str(), c).is_some()
        {
            return Err("cash date/identity coverage".into());
        }
        if c.funding_change_sequence.is_some() == c.exclusion_review.is_some() {
            return Err(
                "cash requires exactly one funding assignment or reviewed exclusion".into(),
            );
        }
        if let Some(i) = &c.exclusion_review {
            identity(i)?;
        }
    }
    let mut assigned = BTreeSet::new();
    let mut change_ids = BTreeSet::new();
    let mut previous = r.start_ordinal;
    for (i, c) in r.changes.iter().enumerate() {
        check()?;
        text(&c.source_event_id)?;
        identity(&c.review)?;
        money(c.amount)?;
        if c.sequence as usize != i + 1
            || c.ordinal < previous
            || c.ordinal <= r.start_ordinal
            || c.ordinal > r.full_end_ordinal
        {
            return Err("ordered funding change sequence/date".into());
        }
        previous = c.ordinal;
        if !change_ids.insert(&c.source_event_id) {
            return Err("duplicate funding source-event identity".into());
        }
        literal(
            &c.kind,
            &[
                "cash_assignment",
                "reviewed_chargeoff_reallocation",
                "reviewed_basis_reallocation",
                "reviewed_other_reallocation",
            ],
        )?;
        if c.kind == "cash_assignment" {
            let key = c
                .cash_event_id
                .as_deref()
                .ok_or("cash assignment requires identity")?;
            let source = cash_map.get(key).ok_or("unknown cash assignment")?;
            if !assigned.insert(key)
                || source.ordinal != c.ordinal
                || source.funding_change_sequence != Some(c.sequence)
                || !near(c.amount, -side * source.bank_cash)
            {
                return Err("funding cash assignment must reconcile once".into());
            }
        } else if c.cash_event_id.is_some() {
            return Err("noncash funding reallocation cannot invent cash".into());
        }
    }
    if r.cash
        .iter()
        .any(|c| c.funding_change_sequence.is_some() && !assigned.contains(c.event_id.as_str()))
    {
        return Err("unmatched assigned cash".into());
    }
    let mut earned_ids = BTreeSet::new();
    for e in &r.earned {
        check()?;
        text(&e.event_id)?;
        identity(&e.review)?;
        money(e.bank_earnings)?;
        if e.ordinal <= r.start_ordinal
            || e.ordinal > r.full_end_ordinal
            || !earned_ids.insert(&e.event_id)
        {
            return Err("earned date/identity domain".into());
        }
        literal(&e.channel, &INCOME_LANES)?;
    }
    let mut previous_reset = None;
    for x in &r.resets {
        check()?;
        identity(&x.review)?;
        if x.ordinal < r.start_ordinal
            || x.ordinal >= r.full_end_ordinal
            || previous_reset.is_some_and(|d| x.ordinal <= d)
            || !x.reference_years.is_finite()
            || !x.liquidity_years.is_finite()
            || !(0. ..=100.).contains(&x.reference_years)
            || !(0. ..=100.).contains(&x.liquidity_years)
        {
            return Err("ordered supplied reset/reference/liquidity tenors required".into());
        }
        previous_reset = Some(x.ordinal);
    }
    if r.resets[0].ordinal != r.start_ordinal {
        return Err("reset schedule must cover opening day".into());
    }
    let mut balance = r.opening_funding_balance;
    let mut days = Vec::with_capacity(n);
    for d in 1..=n {
        check()?;
        let ordinal = r.start_ordinal + d as i32;
        let opening = balance;
        let changes: Vec<_> = r.changes.iter().filter(|x| x.ordinal == ordinal).collect();
        for c in &changes {
            balance += c.amount;
            money(balance)?;
            if r.policy.negative_funding == "reject_negative" && balance < 0. {
                return Err("negative intermediate funding balance rejected by review".into());
            }
        }
        let target = funding(&r.states[d], &r.policy);
        if !near(balance, target) {
            return Err(
                "funding change roll-forward does not match complete state allocation".into(),
            );
        }
        let average = if r.policy.averaging == "opening_daily" {
            opening
        } else {
            (opening + balance) / 2.
        };
        let reset = r
            .resets
            .iter()
            .rev()
            .find(|x| x.ordinal < ordinal)
            .ok_or("reset coverage")?;
        let cash: Vec<_> = r.cash.iter().filter(|x| x.ordinal == ordinal).collect();
        let earned: Vec<_> = r.earned.iter().filter(|x| x.ordinal == ordinal).collect();
        let income: Vec<_> = INCOME_LANES
            .iter()
            .map(|channel| Amount {
                channel: (*channel).into(),
                amount: earned
                    .iter()
                    .filter(|e| e.channel == *channel)
                    .map(|e| e.bank_earnings)
                    .sum::<f64>()
                    * scale,
            })
            .collect();
        let bank_cash = cash.iter().map(|c| c.bank_cash).sum::<f64>() * scale;
        let external = income.iter().map(|x| x.amount).sum();
        for x in income
            .iter()
            .map(|x| x.amount)
            .chain([average * scale, bank_cash, external])
        {
            money(x)?;
        }
        days.push(Day {
            ordinal,
            direction: side,
            opening_funding_balance: opening,
            closing_funding_balance: balance,
            average_funding_balance: average,
            scaled_average_funding_balance: average * scale,
            accrual_fraction: 1. / 365.,
            reference_years: reset.reference_years,
            liquidity_years: reset.liquidity_years,
            reset_review: reset.review.clone(),
            funding_change_sequences: changes.iter().map(|c| c.sequence).collect(),
            cash_event_ids: cash.iter().map(|c| c.event_id.clone()).collect(),
            earned_event_ids: earned.iter().map(|e| e.event_id.clone()).collect(),
            bank_cash,
            original_eir: income[0].amount,
            external_earnings: external,
            earnings_by_channel: income,
        });
    }
    if !near(balance, 0.) {
        return Err("full funding tail remains".into());
    }
    let compatible = days.iter().all(|d| d.scaled_average_funding_balance >= 0.);
    let out = ResultData {
        schema: if tier.schema == SCHEMA {
            "modeled-product-ftp-preparation-proof-result-1"
        } else {
            "modeled-product-ftp-preparation-proof-result-2"
        },
        raw_request: r.clone(),
        scale,
        days,
        closing_unweighted_channels: r.states[n].channels.clone(),
        closing_funding_balance: balance,
        admitted_work: work,
        complete_unweighted_tail_checked: true,
        source_reconstructed: false,
        ordered_journal_verified: false,
        ftp_bridge_adoption: false,
        capital_ftp_eligible: false,
        accounting_standard_approval: false,
        empirical_validation: false,
        treasury_nonnegative_positions_compatible: compatible,
    };
    check()?;
    serialized_size(&out, tier.output)?;
    check()?;
    Ok(out)
}

/// Source-adapter preflight; unchanged primitive policy domains, no source money.
pub fn validate_policy_before_source(
    p: &Policy,
    resets: &[Reset],
    start: i32,
    end: i32,
) -> Result<(), String> {
    validate_policy_in_tier(p, resets, start, end, &LEGACY)
}
pub fn validate_portfolio_policy_before_source(
    p: &Policy,
    resets: &[Reset],
    start: i32,
    end: i32,
) -> Result<(), String> {
    validate_policy_in_tier(p, resets, start, end, &PORTFOLIO)
}
fn validate_policy_in_tier(
    p: &Policy,
    resets: &[Reset],
    start: i32,
    end: i32,
    tier: &Tier,
) -> Result<(), String> {
    identity(&p.review)?;
    if p.accrual_basis != "ACT/365F" {
        return Err("explicit ACT/365F funding accrual basis required".into());
    }
    literal(&p.averaging, &["opening_daily", "trapezoid_daily"])?;
    literal(&p.negative_funding, &["reject_negative", "research_signed"])?;
    literal(
        &p.principal_zero_rights,
        &["reject_unsettled", "explicit_reviewed_all_channels"],
    )?;
    if p.termination != "complete_gross_rights_and_commitment"
        || p.tail != "no_synthetic_cash_or_tail"
    {
        return Err("explicit complete gross termination/tail required".into());
    }
    if p.channels.len() != LANES.len() {
        return Err("all funding channel policies required".into());
    }
    for (i, c) in p.channels.iter().enumerate() {
        if c.channel != LANES[i]
            || !(-1..=1).contains(&c.coefficient)
            || i >= 12 && c.coefficient != 0
        {
            return Err("ordered explicit channel coefficients; no memo funding".into());
        }
        identity(&c.review)?;
    }
    if resets.is_empty() || resets.len() > tier.resets || resets[0].ordinal != start {
        return Err("complete opening reset required".into());
    }
    let mut prior = None;
    for r in resets {
        identity(&r.review)?;
        if r.ordinal < start
            || r.ordinal >= end
            || prior.is_some_and(|p| r.ordinal <= p)
            || !r.reference_years.is_finite()
            || !r.liquidity_years.is_finite()
            || !(0.0..=100.0).contains(&r.reference_years)
            || !(0.0..=100.0).contains(&r.liquidity_years)
        {
            return Err("explicit dated reset/tenor domain".into());
        }
        prior = Some(r.ordinal);
    }
    Ok(())
}
