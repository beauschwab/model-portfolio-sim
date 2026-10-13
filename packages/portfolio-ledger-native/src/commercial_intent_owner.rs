//! Borrowed complete native commercial groups, applied only by the existing Sim.
//! No deserializable checkpoint, money override, credit loop or source projector.
use crate::consumer_intent_owner::state::{accrue, gl_with_owner, IncomeOwner, Line};
use crate::types::Spec;
use portfolio_compute_control::checkpoint;
use portfolio_model_core::commercial_coupon_accounting::{AtomicGroup, ResultData, Snapshot};
use portfolio_model_core::credit_accounting::AccountCode;
use std::collections::{BTreeMap, BTreeSet};

pub const DATE_POLICY: &str = "actual_credit_clock_order_day_end_journal_no_retiming_source-1";
pub const PROTOCOL: u32 = 6;
/// Each group has at most 7 financial endpoints plus two transfers and two
/// balanced memo endpoints per distinct coupon/origin. The extra row covers
/// group metadata. All-day reconciliation is charged for the full calendar.
pub fn admit_upper(groups: usize, lots: usize, days: usize) -> Result<(usize, usize), String> {
    let rows = groups
        .checked_mul(
            lots.checked_mul(4)
                .and_then(|n| n.checked_add(8))
                .ok_or("commercial ledger upper overflow")?,
        )
        .ok_or("commercial ledger upper overflow")?;
    let work = rows
        .checked_mul(
            days.checked_add(1)
                .ok_or("commercial ledger upper overflow")?,
        )
        .and_then(|n| n.checked_mul(16))
        .ok_or("commercial ledger upper overflow")?;
    let bytes = rows
        .checked_mul(4096)
        .ok_or("commercial ledger upper overflow")?;
    if work > 8_000_000 || bytes > 32 << 20 {
        return Err("commercial daily upper exceeds 8M/32MiB before source generation".into());
    }
    Ok((work, bytes))
}
/// Sealed native financial ownership; never accepted as a caller money override.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SourceClass {
    CommercialLoan,
    IssuerAcSecurity,
}
impl SourceClass {
    pub fn identity(self) -> &'static str {
        match self {
            Self::CommercialLoan => "commercial_loan-1",
            Self::IssuerAcSecurity => "issuer_ac_security-1",
        }
    }
    fn accepts(self, kind: &str) -> bool {
        matches!(
            (self, kind),
            (Self::CommercialLoan, "loan") | (Self::IssuerAcSecurity, "security")
        )
    }
}
pub struct Binding<'a> {
    pub source_class: SourceClass,
    pub scenario: &'a str,
    pub position_id: &'a str,
    pub path_key: u64,
    pub origin_day: usize,
    pub source_weight: f64,
    pub allocated_face: f64,
    pub native_unit_face: f64,
    pub accounting: &'a ResultData,
}
pub struct FeeBinding<'a> {
    pub loan_binding:usize,
    pub source:&'a portfolio_model_core::nonintegral_fee::ResultData,
    pub native_years:&'a [f64],
}
#[derive(Clone,Copy)]
pub(crate) enum Scheduled {Loan(usize,usize),Fee(usize,usize)}
pub struct Compiled<'a> {
    pub(crate) bindings: Vec<Binding<'a>>,
    pub(crate) positions: Vec<usize>,
    pub(crate) position_owners: Vec<bool>,
    events: BTreeMap<(String, usize), Vec<(usize, usize)>>,
    pub(crate) fees: Vec<FeeBinding<'a>>,
    fee_events: BTreeMap<(String,usize),Vec<(usize,usize)>>,
    pub work_upper: usize,
    pub retained_bytes_upper: usize,
}
fn amount(x: f64) -> Result<f64, String> {
    if x.is_finite() && x.abs() <= 1e15 {
        Ok(x)
    } else {
        Err("commercial ledger monetary domain".into())
    }
}
fn near(a: f64, b: f64) -> bool {
    (a - b).abs() <= 1e-9_f64.max(64. * f64::EPSILON * a.abs().max(b.abs()))
}
fn day(year: f64, horizon: usize) -> Result<usize, String> {
    if !year.is_finite() || year < 0. || year > horizon as f64 / 365. {
        return Err("complete native commercial tail exceeds daily horizon".into());
    }
    let elapsed = year * 365.;
    let integer = elapsed.round() as usize;
    // Actual integer source dates bind exact native ACT365 bits. Fractional
    // transition actions preserve order and clock metadata within end-of-day.
    let mut result = if (integer as f64 / 365.).to_bits() == year.to_bits() {
        integer
    } else {
        elapsed.ceil() as usize
    };
    // Multiplication can round a just-later native year back onto an integer.
    // Compare the retained clock directly; never post a future event today.
    if year > result as f64 / 365. {
        result = result.checked_add(1).ok_or("commercial calendar overflow")?;
    }
    if result > horizon {
        return Err("commercial clock cannot be clamped to reporting end".into());
    }
    Ok(result)
}
#[cfg(test)]
mod calendar_tests {
    use super::day;
    #[test]
    fn exact_and_adjacent_native_clock_preserve_day_end() {
        let year = 10. / 365.;
        assert_eq!(day(year, 35).unwrap(), 10);
        assert_eq!(day(f64::from_bits(year.to_bits() - 1), 35).unwrap(), 10);
        assert_eq!(day(f64::from_bits(year.to_bits() + 1), 35).unwrap(), 11);
        assert!(day(f64::from_bits(year.to_bits() + 1), 10).is_err());
    }
}
impl<'a> Compiled<'a> {
    pub fn attach_fees(&mut self,spec:&Spec,fees:Vec<FeeBinding<'a>>)->Result<(),String>{
        if !self.fees.is_empty(){return Err("fee ownership already bound".into());}
        let mut seen=BTreeSet::new();
        for(fi,f)in fees.iter().enumerate(){
            checkpoint()?;
            let b=self.bindings.get(f.loan_binding).ok_or("fee parent binding missing")?;
            if !seen.insert(f.loan_binding) || f.native_years.len()!=f.source.groups().len(){return Err("unique complete fee group timeline required".into());}
            for fee in &f.source.raw_request().fees{
                if fee.position_id!=b.position_id{return Err("fee source position mismatch".into());}
            }
            for(i,(g,year))in f.source.groups().iter().zip(f.native_years).enumerate(){
                let elapsed=day(*year,spec.horizon_days-b.origin_day)?;
                if f.source.raw_request().asof_ordinal+elapsed as i32!=g.event.ordinal{return Err("fee action cannot be retimed against native calendar".into());}
                self.fee_events.entry((b.scenario.into(),b.origin_day+elapsed)).or_default().push((fi,i));
                let rows=g.postings.len()+2;
                self.work_upper=self.work_upper.checked_add(rows*(spec.horizon_days+1)*16).ok_or("fee work overflow")?;
                self.retained_bytes_upper=self.retained_bytes_upper.checked_add(rows*4096).ok_or("fee byte overflow")?;
            }
        }
        if self.work_upper>8_000_000 || self.retained_bytes_upper>32<<20{return Err("joined commercial/fee ledger exceeds8M/32MiB".into());}
        self.fees=fees;Ok(())
    }
    pub(crate) fn ordered_events(&self,scenario:&str,day:usize)->Vec<Scheduled>{
        let mut out=self.events(scenario,day).iter().map(|&(b,i)|Scheduled::Loan(b,i)).collect::<Vec<_>>();
        if let Some(events)=self.fee_events.get(&(scenario.into(),day)){out.extend(events.iter().map(|&(b,i)|Scheduled::Fee(b,i)));}
        let key=|e:&Scheduled|match *e{
            Scheduled::Loan(b,i)=>(self.bindings[b].origin_day as f64/365.+self.bindings[b].accounting.groups()[i].year,0,b,i),
            Scheduled::Fee(b,i)=>(self.bindings[self.fees[b].loan_binding].origin_day as f64/365.+self.fees[b].native_years[i],1,b,i)};
        out.sort_by(|a,b|{let a=key(a);let b=key(b);a.0.total_cmp(&b.0).then(a.1.cmp(&b.1)).then(a.2.cmp(&b.2)).then(a.3.cmp(&b.3))});out
    }
    pub fn new(spec: &Spec, bindings: Vec<Binding<'a>>) -> Result<Self, String> {
        checkpoint()?;
        crate::spec::validate(spec, 60000, 90_000_000)?;
        let mut result = Self {
            positions: Vec::new(),
            position_owners: vec![false; spec.positions.len()],
            bindings,
            events: BTreeMap::new(),
            fees: Vec::new(),
            fee_events: BTreeMap::new(),
            work_upper: 0,
            retained_bytes_upper: 0,
        };
        if result.bindings.is_empty() {
            return Ok(result);
        }
        if !spec.reverse_severities.is_empty() {
            return Err("commercial reverse scenarios require explicit native path sources".into());
        }
        let mut unique = BTreeSet::new();
        let scenarios: BTreeSet<_> = std::iter::once("baseline")
            .chain(spec.scenarios.iter().map(|s| s.name.as_str()))
            .collect();
        let mut weights = BTreeMap::<(&str, usize), f64>::new();
        for (bi, b) in result.bindings.iter().enumerate() {
            checkpoint()?;
            let pi = spec
                .positions
                .iter()
                .position(|p| p.id.as_str() == b.position_id)
                .ok_or("commercial native position mapping missing")?;
            let p = &spec.positions[pi];
            if !scenarios.contains(b.scenario) || !unique.insert((b.scenario, pi, b.path_key)) {
                return Err("commercial duplicate or undeclared run/path".into());
            }
            if !b.source_class.accepts(p.kind.as_str())
                || p.classification != "ac"
                || p.duration != 0.
                || p.opening_market_price != 1.
                || p.balance != 0.
                || p.book_adjustment != 0.
                || p.opening_accrued != 0.
                || p.allowance != 0.
                || p.commitment != 0.
                || p.rate != 0.
                || p.annual_pd != 0.
                || p.migration_rate != 0.
                || p.risk_weight != 0.
                || p.rsf_weight != 0.
                || p.hqla_weight != 0.
                || p.lcr_inflow_weight != 0.
                || p.encumbered_fraction != 0.
            {
                return Err("commercial exclusive raw owner requires a zero placeholder and separately reviewed capital/liquidity mapping".into());
            }
            let a = spec
                .accounts
                .iter()
                .find(|a| a.id == p.account.as_str())
                .ok_or("commercial account mapping missing")?;
            if a.cet1_floor != 0.
                || a.leverage_floor != 0.
                || a.lcr_floor != 0.
                || a.nsfr_floor != 0.
                || a.htm_asset_limit != 1.
            {
                return Err("commercial capital/FTP/HTM eligibility is not adopted".into());
            }
            if spec.cashflows.iter().any(|f| f.position == b.position_id)
                || spec.policies.iter().any(|f| f.position == b.position_id)
                || !p.collateral_position.is_empty()
                || p.asf_weight != 0.
                || p.lcr_outflow_weight != 0.
            {
                return Err("commercial actual cash cannot also enter generic cashflows".into());
            }
            b.factor()?;
            *weights.entry((b.scenario, pi)).or_default() += b.source_weight;
            let groups = b.accounting.groups();
            let opening = groups
                .iter()
                .find(|g| g.path_key == b.path_key && g.phase == "reviewed_opening_snapshot")
                .ok_or("native commercial acquisition opening missing")?;
            if opening.next.principal.to_bits() != b.native_unit_face.to_bits() {
                return Err("commercial allocation denominator must bind actual native unit opening principal".into());
            }
            let mut found = false;
            for (index, g) in groups
                .iter()
                .enumerate()
                .filter(|(_, g)| g.path_key == b.path_key)
            {
                checkpoint()?;
                found = true;
                let remaining = spec
                    .horizon_days
                    .checked_sub(b.origin_day)
                    .ok_or("commercial purchase after complete daily horizon")?;
                let at = b
                    .origin_day
                    .checked_add(day(g.year, remaining)?)
                    .ok_or("commercial global day overflow")?;
                result
                    .events
                    .entry((b.scenario.into(), at))
                    .or_default()
                    .push((bi, index));
                let rows = g
                    .postings
                    .len()
                    .checked_add(g.next.lots.len() * 2 + 1)
                    .ok_or("commercial ledger upper overflow")?;
                result.work_upper = result
                    .work_upper
                    .checked_add(
                        rows.checked_mul(spec.horizon_days + 1)
                            .and_then(|v| v.checked_mul(16))
                            .ok_or("commercial ledger upper overflow")?,
                    )
                    .ok_or("commercial ledger upper overflow")?;
                result.retained_bytes_upper = result
                    .retained_bytes_upper
                    .checked_add(
                        rows.checked_mul(4096)
                            .ok_or("commercial ledger upper overflow")?,
                    )
                    .ok_or("commercial ledger upper overflow")?;
            }
            if !found
                || !b
                    .accounting
                    .closing()
                    .iter()
                    .any(|s| s.path_key == b.path_key)
            {
                return Err(
                    "complete native commercial path/closing missing even for zero participation"
                        .into(),
                );
            }
            result.positions.push(pi);
            result.position_owners[pi] = true;
        }
        for (&(_, pi), &weight) in &weights {
            if !near(weight, 1.) {
                return Err(
                    "commercial physical path weights require complete unit probability".into(),
                );
            }
            for &scenario in &scenarios {
                if !weights.contains_key(&(scenario, pi)) {
                    return Err("commercial every named run needs complete native paths".into());
                }
            }
        }
        if result.work_upper > 8_000_000 || result.retained_bytes_upper > 32 << 20 {
            return Err("commercial daily owner exceeds 8M/32MiB before Sim".into());
        }
        for refs in result.events.values_mut() {
            refs.sort_by(|&(a, i), &(b, j)| {
                (result.bindings[a].origin_day as f64 / 365.
                    + result.bindings[a].accounting.groups()[i].year)
                    .total_cmp(
                        &(result.bindings[b].origin_day as f64 / 365.
                            + result.bindings[b].accounting.groups()[j].year),
                    )
                    .then(a.cmp(&b))
                    .then(i.cmp(&j))
            });
        }
        checkpoint()?;
        Ok(result)
    }
    pub(crate) fn events(&self, scenario: &str, day: usize) -> &[(usize, usize)] {
        self.events
            .get(&(scenario.into(), day))
            .map_or(&[], Vec::as_slice)
    }
}
impl Binding<'_> {
    pub(crate) fn factor(&self) -> Result<f64, String> {
        if !self.source_weight.is_finite()
            || !(0. ..=1.).contains(&self.source_weight)
            || !self.allocated_face.is_finite()
            || !(0. ..=1e15).contains(&self.allocated_face)
            || !self.native_unit_face.is_finite()
            || !(0. ..=1e15).contains(&self.native_unit_face)
            || self.native_unit_face == 0.
        {
            return Err("commercial explicit allocation and source probability required".into());
        }
        amount((self.allocated_face / self.native_unit_face) * self.source_weight)
    }
}
type Stock = (AccountCode, Option<String>, Option<usize>);
#[derive(Default, Clone)]
struct Tracking {
    stocks: Vec<(Stock, f64)>,
    legal: BTreeMap<(String, usize), f64>,
    snapshot: Option<Snapshot>,
    legal_principal: Option<f64>,
}
pub(crate) struct Runtime {
    paths: Vec<Tracking>,
    pub(crate) fees:Vec<crate::commercial_fee_owner::FeeTracking>,
}
pub(crate) struct Stage {
    pub lines: Vec<Line>,
    pub cash: f64,
    pub equity: f64,
    next: Tracking,
}
impl Runtime {
    pub(crate) fn snapshots<'a>(
        &'a self,
        c: &'a Compiled<'_>,
        scenario: &str,
    ) -> Result<Vec<(usize, &'a Snapshot)>, String> {
        let mut result = Vec::new();
        for (index, _) in c
            .bindings
            .iter()
            .enumerate()
            .filter(|(_, b)| b.scenario == scenario)
        {
            checkpoint()?;
            let Some(snapshot) = self.paths[index].snapshot.as_ref() else {
                continue;
            };
            result.push((index, snapshot));
        }
        Ok(result)
    }
    pub(crate) fn new(c: &Compiled<'_>) -> Self {
        Self {
            paths: vec![Tracking::default(); c.bindings.len()],
            fees:vec![crate::commercial_fee_owner::FeeTracking::default();c.fees.len()],
        }
    }
    pub(crate) fn stage(
        &self,
        c: &Compiled<'_>,
        binding: usize,
        g: &AtomicGroup,
    ) -> Result<Stage, String> {
        let b = &c.bindings[binding];
        let factor = b.factor()?;
        let mut next = self.paths[binding].clone();
        let mut lines = Vec::new();
        let mut cash = 0.;
        let mut equity = 0.;
        for owned in &g.postings {
            checkpoint()?;
            let p = &owned.posting;
            let key = (
                p.account,
                owned.coupon_id.clone(),
                owned.earning_origin_index,
            );
            let prior = next
                .stocks
                .iter()
                .find(|(k, _)| *k == key)
                .map_or(0., |(_, v)| *v);
            let value = amount(prior + p.amount)?;
            if let Some((_, v)) = next.stocks.iter_mut().find(|(k, _)| *k == key) {
                *v = value;
            } else {
                next.stocks.push((key, value));
            }
            if matches!(
                p.account,
                AccountCode::LiveAccruedInterest | AccountCode::DefaultAccruedInterest
            ) {
                accrue(
                    &mut lines,
                    prior,
                    value,
                    p.account == AccountCode::DefaultAccruedInterest,
                    factor,
                )?;
            } else {
                let gl = gl_with_owner(p.account, IncomeOwner::Commercial);
                let amount = amount(p.amount * factor)?;
                if amount != 0. {
                    lines.push(Line { gl, amount });
                }
                if p.account == AccountCode::Cash {
                    cash += amount;
                }
                if gl.starts_with("pnl:") {
                    equity -= amount;
                }
            }
        }
        for lot in &g.next.lots {
            checkpoint()?;
            let key = (lot.coupon_id.clone(), lot.earning_origin_index);
            let prior = *next.legal.get(&key).unwrap_or(&0.);
            let delta = amount((lot.legal_remaining - prior) * factor)?;
            if delta != 0. {
                lines.push(Line {
                    gl: "memo:commercial_legal_coupon_right",
                    amount: delta,
                });
                lines.push(Line {
                    gl: "memo:commercial_legal_coupon_contra",
                    amount: -delta,
                });
            }
            next.legal.insert(key, lot.legal_remaining);
        }
        if let Some(reviewed) = &g.next.reviewed_credit_state {
            let delta =
                amount((reviewed.legal_principal - next.legal_principal.unwrap_or(0.)) * factor)?;
            if delta != 0. {
                lines.push(Line {
                    gl: "memo:commercial_legal_principal",
                    amount: delta,
                });
                lines.push(Line {
                    gl: "memo:commercial_legal_principal_contra",
                    amount: -delta,
                });
            }
            next.legal_principal = Some(reviewed.legal_principal);
        }
        reconcile(&next, &g.next)?;
        next.snapshot = Some(g.next.clone());
        amount(cash)?;
        amount(equity)?;
        Ok(Stage {
            lines,
            cash,
            equity,
            next,
        })
    }
    pub(crate) fn commit(&mut self, binding: usize, stage: Stage) {
        self.paths[binding] = stage.next;
    }
    pub(crate) fn values(
        &self,
        c: &Compiled<'_>,
        scenario: &str,
    ) -> Result<Vec<(usize, &'static str, f64)>, String> {
        let mut result = Vec::new();
        for(i,b)in c.fees.iter().enumerate().filter(|(_,f)|c.bindings[f.loan_binding].scenario==scenario){
            result.extend(self.fees[i].values(b,c)?);
        }
        for (bi, b) in c
            .bindings
            .iter()
            .enumerate()
            .filter(|(_, b)| b.scenario == scenario)
        {
            let f = b.factor()?;
            let state = &self.paths[bi];
            for ((code, _, _), v) in &state.stocks {
                if matches!(
                    code,
                    AccountCode::Cash
                        | AccountCode::InterestIncome
                        | AccountCode::CreditLossExpense
                        | AccountCode::BasisRecognitionExpense
                        | AccountCode::RecoveryIncome
                ) {
                    continue;
                }
                if matches!(
                    code,
                    AccountCode::LiveAccruedInterest | AccountCode::DefaultAccruedInterest
                ) {
                    let mut lines = Vec::new();
                    accrue(
                        &mut lines,
                        0.,
                        *v,
                        *code == AccountCode::DefaultAccruedInterest,
                        f,
                    )?;
                    result.extend(lines.into_iter().map(|l| (c.positions[bi], l.gl, l.amount)));
                } else {
                    result.push((
                        c.positions[bi],
                        gl_with_owner(*code, IncomeOwner::Commercial),
                        amount(*v * f)?,
                    ));
                }
            }
            if let Some(principal) = state.legal_principal {
                result.push((
                    c.positions[bi],
                    "memo:commercial_legal_principal",
                    amount(principal * f)?,
                ));
                result.push((
                    c.positions[bi],
                    "memo:commercial_legal_principal_contra",
                    amount(-principal * f)?,
                ));
            }
            let legal: f64 = state.legal.values().sum();
            result.push((
                c.positions[bi],
                "memo:commercial_legal_coupon_right",
                amount(legal * f)?,
            ));
            result.push((
                c.positions[bi],
                "memo:commercial_legal_coupon_contra",
                amount(-legal * f)?,
            ));
        }
        Ok(result)
    }
}
fn reconcile(t: &Tracking, s: &Snapshot) -> Result<(), String> {
    let sum = |code| {
        t.stocks
            .iter()
            .filter(|((c, _, _), _)| *c == code)
            .map(|(_, v)| *v)
            .sum::<f64>()
    };
    for (code, expected) in [
        (
            AccountCode::LivePrincipal,
            if s.in_default { 0. } else { s.principal },
        ),
        (
            AccountCode::DefaultPrincipal,
            if s.in_default { s.principal } else { 0. },
        ),
        (
            AccountCode::LiveBookAdjustment,
            if s.in_default { 0. } else { s.book_adjustment },
        ),
        (
            AccountCode::DefaultBookAdjustment,
            if s.in_default { s.book_adjustment } else { 0. },
        ),
    ] {
        if !near(sum(code), expected) {
            return Err("commercial complete native group stock reconciliation".into());
        }
    }
    if let Some(reviewed) = &s.reviewed_credit_state {
        if !near(sum(AccountCode::Allowance), -reviewed.allowance)
            || t.legal_principal
                .is_none_or(|v| v.to_bits() != reviewed.legal_principal.to_bits())
        {
            return Err("commercial reviewed allowance/legal principal reconciliation".into());
        }
    }
    for lot in &s.lots {
        let live = (
            AccountCode::LiveAccruedInterest,
            Some(lot.coupon_id.clone()),
            Some(lot.earning_origin_index),
        );
        let default = (
            AccountCode::DefaultAccruedInterest,
            Some(lot.coupon_id.clone()),
            Some(lot.earning_origin_index),
        );
        for (key, expected) in [
            (
                live,
                if lot.default_claim {
                    0.
                } else {
                    lot.recognized_remaining
                },
            ),
            (
                default,
                if lot.default_claim {
                    lot.recognized_remaining
                } else {
                    0.
                },
            ),
        ] {
            if !near(
                t.stocks
                    .iter()
                    .find(|(k, _)| *k == key)
                    .map_or(0., |(_, v)| *v),
                expected,
            ) {
                return Err("commercial gross recognized origin reconciliation".into());
            }
        }
        if t.legal
            .get(&(lot.coupon_id.clone(), lot.earning_origin_index))
            .is_none_or(|v| v.to_bits() != lot.legal_remaining.to_bits())
        {
            return Err("commercial native legal coupon identity mismatch".into());
        }
    }
    Ok(())
}
