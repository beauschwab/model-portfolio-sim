//! Daily physical candidate sources from immutable native product preparation.
//! Acquisition is separate from repayment/interest; pricing cash never posts.
use crate::product_strategy::{self, Allocation, Prepared};
use portfolio_model_core::{
    cashflow::{validate_ordinal, CashflowMeasure},
    dated_income::{self, DatedIncomeRequest, DatedIncomeResult, Identity, IncomeEvent},
};
use serde::Serialize;
use serde_json::Value;

pub const MAX_SOURCE_DAYS: usize = 4095;
pub const MAX_SOURCE_ROWS: usize = 250_000;
pub const MAX_CANDIDATE_WORK: usize = 8_000_000;

#[derive(Serialize)]
pub struct SourceCash {
    pub event_id: String,
    pub ordinal: i32,
    pub principal: f64,
    pub interest: f64,
}
#[derive(Serialize)]
pub struct Recognition {
    pub origin_id: String,
    pub start_ordinal: i32,
    pub end_ordinal: i32,
    pub interest: f64,
}
#[derive(Serialize)]
pub struct Path {
    pub unit_index: usize,
    pub native_source_index: usize,
    pub unit_id: String,
    pub product: String,
    pub scenario_id: String,
    pub run_id: String,
    pub path_id: String,
    pub source_weight: f64,
    pub allocated_face: f64,
    pub unit_face: f64,
    pub allocation_factor: f64,
    pub side: f64,
    /// Uniform contractual coupon sign from unweighted physical earned/cash.
    /// This is independent of principal direction and original effective yield.
    pub interest_direction: f64,
    pub purchase_ordinal: i32,
    pub full_end_ordinal: i32,
    pub cash: Vec<SourceCash>,
    pub recognition: Vec<Recognition>,
    pub daily_income_request: DatedIncomeRequest,
    pub daily_income: DatedIncomeResult,
}
#[derive(Serialize)]
pub struct Capture {
    pub schema: &'static str,
    pub paths: Vec<Path>,
    pub admitted_work: usize,
    pub admitted_daily_rows: usize,
    pub scope: &'static str,
}
/// Contractual cash-obligation direction, independent of the original EIR.
/// A single physical path may have a uniform signed coupon or no interest.
/// Sign-changing contracts need separate gross obligation ownership.
pub fn contractual_interest_sign(path: &Path) -> Result<f64, String> {
    interest_sign(
        path.recognition
            .iter()
            .map(|r| r.interest)
            .chain(path.cash.iter().map(|c| c.interest)),
    )
}
fn interest_sign(values: impl Iterator<Item = f64>) -> Result<f64, String> {
    let mut sign = 0.;
    for (i, value) in values.enumerate() {
        if i.is_multiple_of(256) {
            portfolio_compute_control::checkpoint()?;
        }
        money(value)?;
        if value != 0. {
            let next = value.signum();
            if sign != 0. && sign != next {
                return Err(
                    "candidate mixed-sign contractual interest requires gross direction ownership"
                        .into(),
                );
            }
            sign = next;
        }
    }
    Ok(if sign == 0. { 1. } else { sign })
}

pub(crate) fn money(x: f64) -> Result<f64, String> {
    if x.is_finite() && x.abs() <= product_strategy::MAX_ALLOCATION_AMOUNT {
        Ok(x)
    } else {
        Err("candidate source monetary admission exceeded".into())
    }
}
fn array<'a>(v: &'a Value, key: &str) -> Result<&'a Vec<Value>, String> {
    v[key]
        .as_array()
        .ok_or_else(|| format!("candidate source {key} array missing"))
}
fn num(v: &Value, key: &str) -> Result<f64, String> {
    money(
        v[key]
            .as_f64()
            .ok_or_else(|| format!("candidate source {key} amount missing"))?,
    )
}
fn day(v: &Value, key: &str) -> Result<i32, String> {
    let d = i32::try_from(
        v[key]
            .as_i64()
            .ok_or("candidate source integer date required")?,
    )
    .map_err(|_| "candidate source date overflow")?;
    validate_ordinal(d)?;
    Ok(d)
}
fn text<'a>(v: &'a Value, key: &str) -> Result<&'a str, String> {
    v[key]
        .as_str()
        .ok_or_else(|| format!("candidate source {key} identity missing"))
}
fn near(a: f64, b: f64) -> Result<(), String> {
    if !a.is_finite() || !b.is_finite() || (a - b).abs() > 1e-9 + 1e-12 * b.abs() {
        Err("candidate daily source/reference amount mismatch".into())
    } else {
        Ok(())
    }
}
fn product(u: &crate::product_units::ProductUnit) -> (&'static str, Value) {
    use crate::product_units::GraphRequest;
    match &u.purchase_graph {
        GraphRequest::ConsumerInstallment(r) => {
            ("consumer_installment", serde_json::to_value(r).unwrap())
        }
        GraphRequest::Card(r) => ("card", serde_json::to_value(r).unwrap()),
        GraphRequest::Mortgage(r) => ("mortgage", serde_json::to_value(r).unwrap()),
        GraphRequest::Deposit(r) => ("deposit", serde_json::to_value(r).unwrap()),
    }
}
fn day_count(raw: &Value, kind: &str) -> Result<(), String> {
    let count = if kind == "deposit" {
        &raw["positions"][0]["histories"][0]["physical"][0]["request"]["day_count"]
    } else if kind == "consumer_installment" {
        &raw["positions"][0]["product"]["contract"]["day_count"]
    } else {
        &raw["positions"][0]["contract"]["day_count"]
    };
    if !matches!(count.as_str(), Some("act360" | "act365_fixed")) {
        return Err("candidate daily recognition supports explicit ACT360/ACT365 only".into());
    }
    if kind == "deposit" {
        for history in array(&raw["positions"][0], "histories")? {
            for path in array(history, "physical")? {
                if !matches!(
                    path["request"]["day_count"].as_str(),
                    Some("act360" | "act365_fixed")
                ) {
                    return Err(
                        "candidate daily recognition supports explicit ACT360/ACT365 only".into(),
                    );
                }
            }
        }
    }
    Ok(())
}

/// Aggregate admission before daily request/result or journal vectors exist.
pub fn admitted_work(
    prepared: &Prepared,
    as_of: i32,
    report_end: i32,
) -> Result<(usize, usize, i32), String> {
    validate_ordinal(as_of)?;
    validate_ordinal(report_end)?;
    let p = prepared.source();
    if p.reporting_ordinals.first() != Some(&as_of)
        || p.reporting_ordinals.last() != Some(&report_end)
    {
        return Err("candidate reporting boundaries must match immutable prepared grid".into());
    }
    let (mut work, mut rows, mut full) = (0usize, 0usize, report_end);
    for (ui, u) in p.raw_source.units.iter().enumerate() {
        portfolio_compute_control::checkpoint()?;
        let (kind, raw) = product(u);
        day_count(&raw, kind)?;
        for source in array(&p.units[ui].complete_purchase_result, "native_sources")? {
            if source["role"] != "physical" {
                continue;
            }
            let scenario = text(source, "scenario_id")?;
            let path = text(source, "path_id")?;
            let receipt = p.units[ui]
                .income
                .iter()
                .find(|r| r.scenario_id == scenario && r.path_id == path)
                .ok_or("candidate physical income receipt missing")?;
            let opening = receipt
                .opening_states
                .iter()
                .find(|s| s.ordinal == u.purchase_ordinal)
                .ok_or("candidate purchase opening missing")?;
            if opening.principal != u.unit_face || opening.accrued_interest != 0. {
                return Err(
                    "candidate actual face must equal native original principal with zero accrued"
                        .into(),
                );
            }
            let old = array(&receipt.native_income["native_income_result"], "rows")?;
            let end = day(
                old.last().ok_or("candidate income source empty")?,
                "payment_ordinal",
            )?;
            let days = usize::try_from(end - u.purchase_ordinal)
                .map_err(|_| "candidate source reversed dates")?;
            if days == 0 || days > MAX_SOURCE_DAYS {
                return Err("candidate complete daily tail exceeds 4095 days".into());
            }
            let projection = &source["projection"];
            let recognition = array(
                projection,
                if kind == "deposit" {
                    "interest"
                } else {
                    "recognition"
                },
            )?;
            let cash = array(projection, "cashflows")?;
            if cash.iter().any(|c| {
                c["ordinal"]
                    .as_i64()
                    .is_none_or(|d| d <= u.purchase_ordinal as i64 || d > end as i64)
            }) {
                return Err(
                    "candidate settlement must follow acquisition and remain inside complete tail"
                        .into(),
                );
            }
            rows = rows
                .checked_add(days)
                .ok_or("candidate source row overflow")?;
            let cost = days
                .checked_mul(recognition.len() + cash.len() + 24)
                .ok_or("candidate work overflow")?;
            work = work.checked_add(cost).ok_or("candidate work overflow")?;
            full = full.max(end);
        }
    }
    if rows > MAX_SOURCE_ROWS || work > MAX_CANDIDATE_WORK {
        return Err("candidate aggregate daily source/work admission exceeded".into());
    }
    Ok((work, rows, full))
}

pub fn capture(
    prepared: &Prepared,
    allocation: &[Allocation],
    as_of: i32,
    report_end: i32,
) -> Result<Capture, String> {
    // Ordered identity and allocation-domain validation also independently checks
    // every retained funding tail, including zero weights/allocations.
    let selected = product_strategy::evaluate(prepared, allocation)?;
    let (work, rows, _) = admitted_work(prepared, as_of, report_end)?;
    let p = prepared.source();
    let mut paths = Vec::new();
    for (ui, u) in p.raw_source.units.iter().enumerate() {
        let (kind, _) = product(u);
        let side = if kind == "deposit" { -1. } else { 1. };
        for (native_source_index, source) in
            array(&p.units[ui].complete_purchase_result, "native_sources")?
                .iter()
                .enumerate()
        {
            if source["role"] != "physical" {
                continue;
            }
            portfolio_compute_control::checkpoint()?;
            let scenario = text(source, "scenario_id")?;
            let path = text(source, "path_id")?;
            let weight = num(source, "weight")?;
            let receipt = p.units[ui]
                .income
                .iter()
                .find(|r| r.scenario_id == scenario && r.path_id == path)
                .ok_or("candidate physical source missing")?;
            near(weight, receipt.source_weight)?;
            let old = array(&receipt.native_income["native_income_result"], "rows")?;
            let end = day(old.last().unwrap(), "payment_ordinal")?;
            let mut cash = Vec::new();
            let mut recognition = Vec::new();
            let projection = &source["projection"];
            for e in array(projection, "cashflows")? {
                let principal = money(num(e, "principal")? * side)?;
                let interest = money(
                    num(
                        e,
                        if kind == "deposit" {
                            "cash_interest"
                        } else {
                            "interest"
                        },
                    )? * side,
                )?;
                if principal < 0. || (kind != "deposit" && num(e, "fees")? != 0.) {
                    return Err("candidate source draws/fees unsupported".into());
                }
                cash.push(SourceCash {
                    event_id: text(e, "event_id")?.into(),
                    ordinal: day(e, "ordinal")?,
                    principal,
                    interest,
                });
            }
            for e in array(
                projection,
                if kind == "deposit" {
                    "interest"
                } else {
                    "recognition"
                },
            )? {
                let a = day(e, "start_ordinal")?;
                let b = day(e, "end_ordinal")?;
                if a < u.purchase_ordinal || b <= a || b > end {
                    return Err(
                        "candidate source recognition interval outside complete admitted tail"
                            .into(),
                    );
                }
                let origin_id = if kind == "deposit" {
                    format!("deposit:{}:{a}:{b}", text(e, "segment_id")?)
                } else {
                    text(e, "origin_id")?.into()
                };
                recognition.push(Recognition {
                    origin_id,
                    start_ordinal: a,
                    end_ordinal: b,
                    interest: num(
                        e,
                        if kind == "deposit" {
                            "amount"
                        } else {
                            "interest"
                        },
                    )?,
                });
            }
            let identity = Identity {
                id: u.unit_id.clone(),
                revision: u.income_policy.review.revision.clone(),
            };
            let interest_direction = interest_sign(
                recognition
                    .iter()
                    .map(|r| r.interest)
                    .chain(cash.iter().map(|c| c.interest)),
            )?;
            let mut events = Vec::with_capacity((end - u.purchase_ordinal) as usize);
            for ordinal in u.purchase_ordinal + 1..=end {
                if (ordinal - u.purchase_ordinal) % 64 == 0 {
                    portfolio_compute_control::checkpoint()?;
                }
                let mut earned = 0.;
                let (mut principal, mut interest) = (0., 0.);
                for s in &recognition {
                    if s.start_ordinal < ordinal && s.end_ordinal >= ordinal {
                        earned =
                            money(earned + s.interest / (s.end_ordinal - s.start_ordinal) as f64)?;
                    }
                }
                for c in cash.iter().filter(|c| c.ordinal == ordinal) {
                    principal = money(principal + c.principal)?;
                    interest = money(interest + c.interest)?;
                }
                events.push(IncomeEvent {
                    event_id: format!("{}:{scenario}:{path}:income:{ordinal}", u.unit_id),
                    source: identity.clone(),
                    payment_ordinal: ordinal,
                    contractual_interest_earned: earned,
                    principal_cash: principal,
                    interest_cash: interest,
                    integral_fee_cash: 0.,
                    nonintegral_fee_cash: 0.,
                    yield_change: None,
                });
            }
            let request = DatedIncomeRequest {
                schema: dated_income::SCHEMA.into(),
                contract: identity,
                original_yield_anchor: u.income_policy.original_yield_anchor.clone(),
                policy: u.income_policy.review.clone(),
                currency: p.currency.clone(),
                monetary_unit: p.monetary_unit.clone(),
                measure: CashflowMeasure::Physical,
                as_of_ordinal: u.purchase_ordinal,
                full_contract_end_ordinal: end,
                opening_principal: u.unit_face,
                opening_accrued_interest: 0.,
                opening_gross_carrying_value: money(u.unit_face * u.quote_price_per_face_unit)?,
                original_annual_effective_rate: u.income_policy.original_annual_effective_rate,
                opening_annual_effective_rate: u.income_policy.original_annual_effective_rate,
                events,
            };
            let result = dated_income::evaluate(&request)?;
            if result.remaining_principal != 0. {
                return Err(
                    "candidate complete principal tail must close without synthetic payoff".into(),
                );
            }
            near(result.remaining_accrued_interest, 0.)?;
            near(result.remaining_gross_carrying_value, 0.)?;
            let mut cumulative = 0.;
            let mut cursor = 0;
            for old_row in old {
                let ordinal = day(old_row, "payment_ordinal")?;
                while cursor < result.rows.len() && result.rows[cursor].payment_ordinal <= ordinal {
                    cumulative = money(cumulative + result.rows[cursor].effective_interest)?;
                    cursor += 1;
                }
                let r = result
                    .rows
                    .get(
                        cursor
                            .checked_sub(1)
                            .ok_or("candidate source date missing")?,
                    )
                    .ok_or("candidate daily date missing")?;
                near(r.closing_principal, num(old_row, "closing_principal")?)?;
                near(
                    r.closing_accrued_interest,
                    num(old_row, "closing_accrued_interest")?,
                )?;
                near(
                    r.closing_gross_carrying_value,
                    num(old_row, "closing_gross_carrying_value")?,
                )?;
            }
            let old_effective: f64 = old
                .iter()
                .map(|r| num(r, "effective_interest"))
                .collect::<Result<Vec<_>, _>>()?
                .iter()
                .sum();
            near(cumulative, old_effective)?;
            paths.push(Path {
                unit_index: ui,
                native_source_index,
                unit_id: u.unit_id.clone(),
                product: kind.into(),
                scenario_id: scenario.into(),
                run_id: text(source, "run_id")?.into(),
                path_id: path.into(),
                source_weight: weight,
                allocated_face: allocation[ui].face,
                unit_face: u.unit_face,
                allocation_factor: money(allocation[ui].face / u.unit_face)?,
                side,
                interest_direction,
                purchase_ordinal: u.purchase_ordinal,
                full_end_ordinal: end,
                cash,
                recognition,
                daily_income_request: request,
                daily_income: result,
            });
        }
    }
    // Compare daily original-EIR expected income with immutable selected tensor.
    let h = p.shape[3];
    for s in &selected.scenarios {
        for period in 0..h {
            let a = p.reporting_ordinals[period];
            let b = p.reporting_ordinals[period + 1];
            let mut actual = 0.;
            for path in paths.iter().filter(|p| p.scenario_id == s.scenario_id) {
                for r in path
                    .daily_income
                    .rows
                    .iter()
                    .filter(|r| r.payment_ordinal > a && r.payment_ordinal <= b)
                {
                    actual = money(
                        actual
                            + r.effective_interest
                                * path.source_weight
                                * path.allocation_factor
                                * path.side,
                    )?;
                }
            }
            near(actual, s.values[period])?;
        }
    }
    Ok(Capture {schema:"product-candidate-source-1",paths,admitted_work:work,admitted_daily_rows:rows,
        scope:"immutable supplied physical performing purchase/cash/recognition with reviewed original-EIR daily source; pricing histories retained separately; no observed cash, accounting-standard or mixed ledger verification claim"})
}

#[cfg(test)]
mod interest_direction_tests {
    use super::interest_sign;
    #[test]
    fn uniform_contractual_direction_uses_earned_and_cash_not_yield() {
        for (values, expected) in [
            (vec![0., 0.], 1.),
            (vec![0., 0.1, 0.1], 1.),
            (vec![-0.1, 0., -0.1], -1.),
        ] {
            assert_eq!(interest_sign(values.into_iter()).unwrap(), expected);
        }
        for values in [
            vec![-0.1, 0.1],
            vec![0.1, -0.1],
            vec![f64::NAN],
            vec![1e15 + 1.],
        ] {
            assert!(interest_sign(values.into_iter()).is_err());
        }
    }
}
