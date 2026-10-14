//! Diagnostic representative-versus-member supplied native graph audit.
//! No inferred representative state, calibration or cohort publication permission.
use crate::{advanced_graph, card_graph, commercial_graph, deposit_graph, mortgage_graph};
use portfolio_model_core::limits::MAX_MONETARY_AMOUNT;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{
    collections::{BTreeMap, BTreeSet},
    io::{self, Write},
};
pub const SCHEMA: &str = "graph-cohort-audit-1";
pub const MAX_ROWS: usize = 4096;
pub const MAX_WORK: usize = 32_000_000;
pub const MAX_INPUT_BYTES: usize = 32 * 1024 * 1024;
pub const MAX_RESULT_BYTES: usize = 128 * 1024 * 1024;

#[derive(Clone, Serialize, Deserialize)]
#[serde(
    tag = "kind",
    content = "request",
    rename_all = "snake_case",
    deny_unknown_fields
)]
pub enum GraphRequest {
    ConsumerInstallment(Box<advanced_graph::Request>),
    Card(Box<card_graph::Request>),
    Mortgage(Box<mortgage_graph::Request>),
    Deposit(Box<deposit_graph::Request>),
    Commercial(Box<commercial_graph::Request>),
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Row {
    pub row_id: String,
    pub amount_multiplier: f64,
    pub graph: GraphRequest,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Tolerance {
    pub metric: String,
    pub absolute_amount: f64,
    pub relative: f64,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Cohort {
    pub cohort_id: String,
    pub representative: Row,
    pub members: Vec<Row>,
    pub tolerances: Vec<Tolerance>,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Request {
    pub schema: String,
    pub source_revision: String,
    pub monetary_unit: String,
    pub cohorts: Vec<Cohort>,
}
#[derive(Clone, Serialize)]
pub struct Amount {
    pub scenario_id: String,
    pub metric: String,
    pub amount: f64,
}
#[derive(Clone, Serialize)]
pub struct RowResult {
    pub cohort_id: String,
    pub row_id: String,
    pub role: String,
    pub product: String,
    pub currency: String,
    pub amount_multiplier: f64,
    pub amounts: Vec<Amount>,
    pub native_result: Value,
}
#[derive(Clone, Serialize)]
pub struct ErrorRow {
    pub cohort_id: String,
    pub product: String,
    pub currency: String,
    pub scenario_id: String,
    pub metric: String,
    pub representative: f64,
    pub summed_members: f64,
    pub signed_error: f64,
    pub absolute_error: f64,
    pub relative_error: Option<f64>,
    pub absolute_tolerance: f64,
    pub relative_tolerance: f64,
    pub allowed_error: f64,
    pub passes: bool,
}
#[derive(Serialize)]
pub struct ResultData {
    pub schema: String,
    pub source_revision: String,
    pub monetary_unit: String,
    pub raw_source: Request,
    pub admitted_work: usize,
    pub rows: Vec<RowResult>,
    pub errors: Vec<ErrorRow>,
    pub all_metrics_within_tolerance: bool,
    pub diagnostic_only: bool,
    pub empirical_validation: bool,
    pub cohort_publication_allowed: bool,
    pub statement: String,
}
struct Count {
    bytes: usize,
    limit: usize,
}
impl Write for Count {
    fn write(&mut self, b: &[u8]) -> io::Result<usize> {
        portfolio_compute_control::checkpoint().map_err(io::Error::other)?;
        self.bytes = self.bytes.saturating_add(b.len());
        if self.bytes > self.limit {
            return Err(io::Error::other("graph cohort byte admission exceeded"));
        }
        Ok(b.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}
fn size<T: Serialize>(v: &T, limit: usize) -> Result<usize, String> {
    let mut c = Count { bytes: 0, limit };
    serde_json::to_writer(&mut c, v).map_err(|e| e.to_string())?;
    Ok(c.bytes)
}
fn label(s: &str) -> Result<(), String> {
    if s.trim().is_empty() || s.len() > 256 || s.chars().any(char::is_control) {
        Err("graph cohort bounded source identity required".into())
    } else {
        Ok(())
    }
}
fn money(v: f64) -> Result<f64, String> {
    if !v.is_finite() || v.abs() > MAX_MONETARY_AMOUNT {
        Err("graph cohort signed amount admission exceeded".into())
    } else {
        Ok(v)
    }
}
impl GraphRequest {
    fn product(&self) -> &'static str {
        match self {
            Self::ConsumerInstallment(_) => "consumer_installment",
            Self::Card(_) => "card",
            Self::Mortgage(_) => "mortgage",
            Self::Deposit(_) => "deposit",
            Self::Commercial(_) => "commercial",
        }
    }
    fn request(&self) -> Result<Value, String> {
        match self {
            Self::ConsumerInstallment(r) => serde_json::to_value(r),
            Self::Card(r) => serde_json::to_value(r),
            Self::Mortgage(r) => serde_json::to_value(r),
            Self::Deposit(r) => serde_json::to_value(r),
            Self::Commercial(r) => serde_json::to_value(r),
        }
        .map_err(|e| e.to_string())
    }
    fn work(&self) -> Result<usize, String> {
        match self {
            Self::ConsumerInstallment(r) => advanced_graph::audit_admitted_work(r),
            Self::Card(r) => card_graph::audit_admitted_work(r),
            Self::Mortgage(r) => mortgage_graph::audit_admitted_work(r),
            Self::Deposit(r) => deposit_graph::audit_admitted_work(r),
            Self::Commercial(r) => commercial_graph::audit_admitted_work(r),
        }
    }
    fn run(&self) -> Result<Value, String> {
        match self {
            Self::ConsumerInstallment(r) => serde_json::to_value(advanced_graph::analyze(r)?),
            Self::Card(r) => serde_json::to_value(card_graph::analyze(r)?),
            Self::Mortgage(r) => serde_json::to_value(mortgage_graph::analyze(r)?),
            Self::Deposit(r) => serde_json::to_value(deposit_graph::analyze(r)?),
            Self::Commercial(r) => serde_json::to_value(commercial_graph::analyze(r)?),
        }
        .map_err(|e| e.to_string())
    }
}
fn context(row: &Row) -> Result<Value, String> {
    let raw = row.graph.request()?;
    let positions = raw["positions"]
        .as_array()
        .ok_or("graph audit position missing")?;
    if positions.len() != 1 || positions[0]["position_id"] != row.row_id {
        return Err("graph audit row must identify exactly one native position".into());
    }
    let target = &positions[0]["oas_target"];
    if target["kind"] != "locked" {
        return Err(
            "graph cohort requires frozen supplied per-row OAS; no calibration allowed".into(),
        );
    }
    let histories = positions[0]
        .get("histories")
        .unwrap_or(&positions[0]["product"]["histories"])
        .as_array()
        .ok_or("graph audit history missing")?;
    let mut pairing = Vec::new();
    for scenario in histories {
        let mut roles = serde_json::Map::new();
        roles.insert("scenario_id".into(), scenario["scenario_id"].clone());
        for role in ["physical", "pricing", "pricing_down", "pricing_up"] {
            let paths = scenario[role]
                .as_array()
                .ok_or("graph audit history role missing")?;
            roles.insert(
                role.into(),
                Value::Array(
                    paths
                        .iter()
                        .map(|p| json!([p["path_id"], p["weight"]]))
                        .collect(),
                ),
            );
        }
        pairing.push(Value::Object(roles));
    }
    let mut out = serde_json::Map::new();
    for key in [
        "schema",
        "as_of_ordinal",
        "currency",
        "reporting_ordinals",
        "base_scenario_id",
        "bump_bp",
        "scenarios",
        "terminal_policy",
        "tail_policy",
        "cash_policy",
        "cash_timing",
    ] {
        if let Some(v) = raw.get(key) {
            out.insert(key.into(), v.clone());
        }
    }
    let identity = &raw["identity"];
    if row.graph.product() == "consumer_installment" {
        out.insert(
            "contract_product".into(),
            positions[0]["product"]["contract"]["product"]["kind"].clone(),
        );
    }
    if row.graph.product() == "mortgage" {
        out.insert(
            "ownership_kind".into(),
            positions[0]["contract"]["ownership"]["kind"].clone(),
        );
    }
    out.insert(
        "pairing_identity".into(),
        json!([
            identity["model_revision"],
            identity["paired_history_source"],
            identity["paired_history_revision"],
            identity["randomness_domain"]
        ]),
    );
    out.insert("history_pairing".into(), Value::Array(pairing));
    Ok(Value::Object(out))
}
fn flatten(value: &Value, path: &str, out: &mut BTreeMap<String, f64>) -> Result<(), String> {
    match value {
        Value::Number(n) => {
            out.insert(
                path.into(),
                money(n.as_f64().ok_or("graph audit metric not f64")?)?,
            );
        }
        Value::Object(v) => {
            for (k, x) in v {
                if k != "position_id" && k != "scenario_id" {
                    flatten(
                        x,
                        &if path.is_empty() {
                            k.clone()
                        } else {
                            format!("{path}/{k}")
                        },
                        out,
                    )?;
                }
            }
        }
        Value::Array(v) => {
            for (i, x) in v.iter().enumerate() {
                flatten(x, &format!("{path}/{i}"), out)?;
            }
        }
        _ => return Err("graph audit metrics must be explicit monetary numeric reports".into()),
    }
    Ok(())
}
fn add(out: &mut BTreeMap<String, f64>, name: &str, value: f64) -> Result<(), String> {
    let old = out.get(name).copied().unwrap_or(0.);
    out.insert(name.into(), money(old + value)?);
    Ok(())
}
fn full_source(
    source: &Value,
    product: &str,
    out: &mut BTreeMap<String, f64>,
) -> Result<(), String> {
    let p = &source["projection"];
    let w = source["weight"]
        .as_f64()
        .ok_or("graph audit source weight missing")?;
    if product == "commercial" {
        for name in ["principal", "interest", "fees"] {
            out.entry(format!("source_full_cash/{name}")).or_insert(0.);
            for cash in source["cash"]
                .as_array()
                .ok_or("commercial audit complete actual cash missing")?
            {
                add(
                    out,
                    &format!("source_full_cash/{name}"),
                    money(
                        w * cash[name]
                            .as_f64()
                            .ok_or("commercial audit cash amount missing")?,
                    )?,
                )?;
            }
        }
        out.entry("source_full_gross_interest".into()).or_insert(0.);
        for contribution in p["carry"]["earned_origin_contributions"]
            .as_array()
            .ok_or("commercial audit earned coupon ownership missing")?
        {
            add(
                out,
                "source_full_gross_interest",
                money(
                    w * contribution["contractual_earned"]
                        .as_f64()
                        .ok_or("commercial audit earned amount missing")?,
                )?,
            )?;
        }
        out.entry("source_full_effective_interest".into())
            .or_insert(0.);
        for group in p["accounting"]["groups"]
            .as_array()
            .ok_or("commercial audit reviewed accounting missing")?
        {
            for line in group["postings"]
                .as_array()
                .ok_or("commercial audit owned postings missing")?
            {
                if line["posting"]["account"] == "interest_income" {
                    add(
                        out,
                        "source_full_effective_interest",
                        money(
                            -w * line["posting"]["amount"]
                                .as_f64()
                                .ok_or("commercial audit effective income missing")?,
                        )?,
                    )?;
                }
            }
        }
        for (metric, field) in [
            ("principal", "principal"),
            ("principal_loss", "economic_principal_loss"),
        ] {
            out.entry(format!("source_full_default/{metric}"))
                .or_insert(0.);
            for path in p["carry"]["credit"]["result"]["paths"]
                .as_array()
                .ok_or("commercial audit complete credit path missing")?
            {
                if !path["default"].is_null() {
                    add(
                        out,
                        &format!("source_full_default/{metric}"),
                        money(
                            w * path["default"][field]
                                .as_f64()
                                .ok_or("commercial audit default claim missing")?,
                        )?,
                    )?;
                }
            }
        }
        return Ok(());
    }
    // All source cash, including opening and beyond-report/principal-end tails.
    for name in ["principal", "interest", "fees"] {
        out.entry(format!("source_full_cash/{name}")).or_insert(0.);
    }
    out.entry("source_full_gross_interest".into()).or_insert(0.);
    for cash in p["cashflows"]
        .as_array()
        .ok_or("graph audit full cash source missing")?
    {
        for name in ["principal", "interest", "fees"] {
            let field = if product == "deposit" && name == "interest" {
                "cash_interest"
            } else {
                name
            };
            let value = if product == "deposit" && name == "fees" {
                0.
            } else {
                cash[field]
                    .as_f64()
                    .ok_or("graph audit cash source amount missing")?
            };
            add(out, &format!("source_full_cash/{name}"), money(value * w)?)?;
        }
    }
    let recognition = if product == "deposit" {
        "interest"
    } else {
        "recognition"
    };
    let field = if product == "deposit" {
        "amount"
    } else {
        "interest"
    };
    for accrual in p[recognition]
        .as_array()
        .ok_or("graph audit full recognition source missing")?
    {
        add(
            out,
            "source_full_gross_interest",
            money(
                accrual[field]
                    .as_f64()
                    .ok_or("graph audit full interest missing")?
                    * w,
            )?,
        )?;
    }
    if product == "consumer_installment" || product == "card" {
        for field in ["principal", "principal_loss"] {
            let value = if p["default"].is_null() {
                0.
            } else {
                p["default"][field]
                    .as_f64()
                    .ok_or("graph audit default amount missing")?
            };
            add(
                out,
                &format!("source_full_default/{field}"),
                money(value * w)?,
            )?;
        }
    }
    Ok(())
}
fn amounts(
    result: &Value,
    product: &str,
    multiplier: f64,
) -> Result<BTreeMap<(String, String), f64>, String> {
    let mut scenarios = BTreeMap::<String, BTreeMap<String, f64>>::new();
    for metric in result["metrics"]
        .as_array()
        .ok_or("graph audit native metrics missing")?
    {
        let scenario = metric["scenario_id"]
            .as_str()
            .ok_or("graph audit metric scenario missing")?
            .to_owned();
        let mut flat = BTreeMap::new();
        if product == "commercial" {
            let ordinal = metric["ordinal"]
                .as_i64()
                .ok_or("commercial audit metric date missing")?;
            let entry = scenarios.entry(scenario).or_default();
            for name in ["pv", "dv01"] {
                if entry
                    .insert(
                        format!("date/{ordinal}/{name}"),
                        money(
                            metric[name]
                                .as_f64()
                                .ok_or("commercial audit dated monetary metric missing")?,
                        )?,
                    )
                    .is_some()
                {
                    return Err("duplicate commercial audit metric/date".into());
                }
            }
            continue;
        }
        flatten(metric, "", &mut flat)?;
        if scenarios.insert(scenario, flat).is_some() {
            return Err("graph audit duplicate metric scenario".into());
        }
    }
    for source in result["native_sources"]
        .as_array()
        .ok_or("graph audit native sources missing")?
    {
        if source["role"] == "physical" {
            let scenario = source["scenario_id"]
                .as_str()
                .ok_or("graph audit source scenario missing")?;
            full_source(
                source,
                product,
                scenarios
                    .get_mut(scenario)
                    .ok_or("graph audit physical scenario missing")?,
            )?;
        }
    }
    let mut out = BTreeMap::new();
    for (scenario, flat) in scenarios {
        for (metric, value) in flat {
            out.insert((scenario.clone(), metric), money(value * multiplier)?);
        }
    }
    Ok(out)
}
pub fn evaluate(r: &Request) -> Result<ResultData, String> {
    portfolio_compute_control::checkpoint()?;
    if r.schema != SCHEMA
        || r.monetary_unit != "currency_units"
        || r.cohorts.is_empty()
        || r.cohorts.len() > MAX_ROWS
    {
        return Err("graph audit explicit version/currency units/cohort admission required".into());
    }
    label(&r.source_revision)?;
    let input = size(r, MAX_INPUT_BYTES)?;
    let (mut work, mut count) = (input, 0usize);
    let mut ids = BTreeSet::new();
    let mut cohort_ids = BTreeSet::new();
    // Complete structural/domain and aggregate charged work admission before ANY generation.
    for c in &r.cohorts {
        label(&c.cohort_id)?;
        if !cohort_ids.insert(&c.cohort_id)
            || c.members.is_empty()
            || c.members.len() + 1 > 256
            || c.tolerances.is_empty()
            || c.tolerances.len() > 10000
        {
            return Err("graph audit cohort dimensions/tolerances exceed admission".into());
        }
        let reference = context(&c.representative)?;
        let mut labels = BTreeSet::new();
        for t in &c.tolerances {
            label(&t.metric)?;
            money(t.absolute_amount)?;
            if t.absolute_amount < 0.
                || !t.relative.is_finite()
                || !(0.0..=1.0).contains(&t.relative)
                || !labels.insert(&t.metric)
            {
                return Err("graph audit duplicate/invalid tolerance".into());
            }
        }
        for row in std::iter::once(&c.representative).chain(&c.members) {
            portfolio_compute_control::checkpoint()?;
            label(&row.row_id)?;
            if !ids.insert(&row.row_id)
                || !row.amount_multiplier.is_finite()
                || row.amount_multiplier <= 0.
                || row.amount_multiplier > 1e12
                || row.graph.product() != c.representative.graph.product()
                || context(row)? != reference
            {
                return Err("graph audit row/product/context/pairing/multiplier mismatch".into());
            }
            count += 1;
            work = work
                .checked_add(row.graph.work()?)
                .and_then(|w| w.checked_add(c.tolerances.len() * 13 * 4))
                .ok_or("graph audit work overflow")?;
            if count > MAX_ROWS || work > MAX_WORK {
                return Err(
                    "graph audit aggregate work/row admission exceeded before generation".into(),
                );
            }
        }
    }
    let mut rows = Vec::with_capacity(count);
    let mut errors = Vec::new();
    let mut retained = input;
    for c in &r.cohorts {
        let mut reference = None;
        let mut sum = BTreeMap::<(String, String), f64>::new();
        let mut currency = String::new();
        for (index, row) in std::iter::once(&c.representative)
            .chain(&c.members)
            .enumerate()
        {
            portfolio_compute_control::checkpoint()?;
            let result = row.graph.run()?;
            let values = amounts(&result, row.graph.product(), row.amount_multiplier)?;
            let expected: BTreeSet<_> = c.tolerances.iter().map(|t| t.metric.as_str()).collect();
            let actual: BTreeSet<_> = values.keys().map(|(_, m)| m.as_str()).collect();
            if expected != actual {
                return Err("graph audit requires exact tolerance coverage: missing or unknown metric labels".into());
            }
            if index == 0 {
                reference = Some(values.clone());
                currency = row.graph.request()?["currency"]
                    .as_str()
                    .ok_or("graph audit currency missing")?
                    .to_owned();
            } else {
                let known = reference.as_ref().unwrap();
                if known.keys().ne(values.keys()) {
                    return Err("graph audit member metric/period/scenario coverage differs".into());
                }
                for (key, value) in &values {
                    let old = sum.get(key).copied().unwrap_or(0.);
                    sum.insert(key.clone(), money(old + value)?);
                }
            }
            let captured = RowResult {
                cohort_id: c.cohort_id.clone(),
                row_id: row.row_id.clone(),
                role: if index == 0 {
                    "representative"
                } else {
                    "member"
                }
                .into(),
                product: row.graph.product().into(),
                currency: currency.clone(),
                amount_multiplier: row.amount_multiplier,
                amounts: values
                    .into_iter()
                    .map(|((scenario_id, metric), amount)| Amount {
                        scenario_id,
                        metric,
                        amount,
                    })
                    .collect(),
                native_result: result,
            };
            retained = retained
                .checked_add(size(&captured, MAX_RESULT_BYTES)?)
                .ok_or("graph audit retained byte overflow")?;
            if retained.saturating_mul(4) > MAX_RESULT_BYTES {
                return Err("graph audit retained results exceed admission".into());
            }
            rows.push(captured);
        }
        let policies: BTreeMap<_, _> = c
            .tolerances
            .iter()
            .map(|t| (t.metric.as_str(), t))
            .collect();
        for ((scenario, metric), value) in reference.unwrap() {
            let members = sum[&(scenario.clone(), metric.clone())];
            let error = money(value - members)?;
            let absolute = error.abs();
            let t = policies[metric.as_str()];
            let allowed = money(t.absolute_amount + t.relative * members.abs())?;
            let relative_error = if members == 0. {
                None
            } else {
                let ratio = absolute / members.abs();
                if !ratio.is_finite() {
                    return Err(
                        "graph audit relative diagnostic exceeds finite numeric domain".into(),
                    );
                }
                Some(ratio)
            };
            errors.push(ErrorRow {
                cohort_id: c.cohort_id.clone(),
                product: c.representative.graph.product().into(),
                currency: currency.clone(),
                scenario_id: scenario,
                metric,
                representative: value,
                summed_members: members,
                signed_error: error,
                absolute_error: absolute,
                relative_error,
                absolute_tolerance: t.absolute_amount,
                relative_tolerance: t.relative,
                allowed_error: allowed,
                passes: absolute <= allowed,
            });
        }
    }
    let all_metrics_within_tolerance = errors.iter().all(|e| e.passes);
    let result=ResultData{schema:SCHEMA.into(),source_revision:r.source_revision.clone(),monetary_unit:r.monetary_unit.clone(),raw_source:r.clone(),admitted_work:work,rows,errors,all_metrics_within_tolerance,diagnostic_only:true,empirical_validation:false,cohort_publication_allowed:false,statement:"Supplied representative versus explicitly supplied member graph amounts under fixed per-row OAS; no inferred cohort state, empirical validation or publication authorization".into()};
    if size(&result, MAX_RESULT_BYTES)?.saturating_mul(4) > MAX_RESULT_BYTES {
        return Err("graph audit final retained results exceed admission".into());
    }
    portfolio_compute_control::checkpoint()?;
    Ok(result)
}
