// Typed raw inputs with explicit research-model defaults, validated in spec.rs.
use serde::{Deserialize, Serialize};
/// Immutable contract labels are shared by daily snapshots; cloning a position
/// never allocates copies of its strings. Wire representation remains a string.
#[derive(Clone, Debug, Default, Deserialize, Serialize, Eq, PartialEq, Hash)]
#[serde(transparent)]
pub struct SharedText(std::sync::Arc<str>);
impl SharedText {
    pub fn as_str(&self) -> &str {
        &self.0
    }
}
impl std::ops::Deref for SharedText {
    type Target = str;
    fn deref(&self) -> &str {
        &self.0
    }
}
impl std::borrow::Borrow<str> for SharedText {
    fn borrow(&self) -> &str {
        &self.0
    }
}
impl std::fmt::Display for SharedText {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}
impl From<&str> for SharedText {
    fn from(value: &str) -> Self {
        Self(value.into())
    }
}
impl PartialEq<&str> for SharedText {
    fn eq(&self, other: &&str) -> bool {
        self.as_str() == *other
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Account {
    pub id: String,
    pub entity: String,
    pub currency: String,
    pub cash: f64,
    pub equity: f64,
    #[serde(default)]
    pub cash_floor: f64,
    #[serde(default = "default_account_cet1_floor")]
    pub cet1_floor: f64,
    #[serde(default = "default_account_leverage_floor")]
    pub leverage_floor: f64,
    #[serde(default)]
    pub capital_deductions: f64,
    #[serde(default = "default_account_include_aoci")]
    pub include_aoci: bool,
    #[serde(default)]
    pub annual_fees: f64,
    #[serde(default)]
    pub annual_costs: f64,
    #[serde(default)]
    pub tax_rate: f64,
    #[serde(default)]
    pub annual_dividends: f64,
    #[serde(default)]
    pub lcr_floor: f64,
    #[serde(default)]
    pub nsfr_floor: f64,
    #[serde(default = "default_account_htm_asset_limit")]
    pub htm_asset_limit: f64,
}
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Position {
    pub id: SharedText,
    pub account: SharedText,
    pub kind: SharedText,
    pub balance: f64,
    #[serde(default)]
    pub rate: f64,
    #[serde(default = "default_position_payment_interval_days")]
    pub payment_interval_days: usize,
    #[serde(default)]
    pub floating_beta: f64,
    #[serde(default)]
    pub maturity_day: usize,
    #[serde(default = "default_position_classification")]
    pub classification: SharedText,
    #[serde(default = "default_position_risk_weight")]
    pub risk_weight: f64,
    #[serde(default)]
    pub asf_weight: f64,
    #[serde(default = "default_position_rsf_weight")]
    pub rsf_weight: f64,
    #[serde(default)]
    pub lcr_outflow_weight: f64,
    #[serde(default)]
    pub hqla_weight: f64,
    #[serde(default)]
    pub duration: f64,
    #[serde(default)]
    pub allowance: f64,
    #[serde(default)]
    pub annual_pd: f64,
    #[serde(default = "default_position_lgd")]
    pub lgd: f64,
    #[serde(default)]
    pub watch_fraction: f64,
    #[serde(default = "default_position_watch_pd_multiplier")]
    pub watch_pd_multiplier: f64,
    #[serde(default)]
    pub migration_rate: f64,
    #[serde(default = "default_position_recovery_days")]
    pub recovery_days: usize,
    #[serde(default)]
    pub commitment: f64,
    #[serde(default)]
    pub draw_fraction: f64,
    #[serde(default)]
    pub monthly_runoff: f64,
    #[serde(default)]
    pub uninsured_fraction: f64,
    #[serde(default)]
    pub concentration: f64,
    #[serde(default)]
    pub operational_fraction: f64,
    #[serde(default)]
    pub digital_fraction: f64,
    #[serde(default)]
    pub rollover: f64,
    #[serde(default)]
    pub collateral_pool: SharedText,
    #[serde(default)]
    pub eligible_fraction: f64,
    #[serde(default)]
    pub encumbered_fraction: f64,
    #[serde(default)]
    pub haircut: f64,
    #[serde(default)]
    pub source_id: SharedText,
    #[serde(default)]
    pub book_adjustment: f64,
    #[serde(default)]
    pub opening_accrued: f64,
    #[serde(default)]
    pub collateral_position: SharedText,
    #[serde(default)]
    pub pledged_face: f64,
    #[serde(default)]
    pub start_day: usize,
    #[serde(default = "default_position_hqla_level")]
    pub hqla_level: SharedText,
    #[serde(default)]
    pub lcr_inflow_weight: f64,
    #[serde(default = "default_position_opening_market_price")]
    pub opening_market_price: f64,
}
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct NettingSet {
    pub id: String,
    pub account: String,
    pub counterparty: String,
    #[serde(default)]
    pub fair_value: f64,
    #[serde(default)]
    pub posted_margin: f64,
    #[serde(default)]
    pub received_margin: f64,
    #[serde(default)]
    pub stress_loss: f64,
    #[serde(default)]
    pub margin_threshold: f64,
    #[serde(default = "default_nettingset_margin_delay")]
    pub margin_delay: usize,
    #[serde(default)]
    pub annual_pd: f64,
    #[serde(default = "default_nettingset_lgd")]
    pub lgd: f64,
    #[serde(default = "default_nettingset_wrong_way_multiplier")]
    pub wrong_way_multiplier: f64,
    #[serde(default = "default_nettingset_risk_weight")]
    pub risk_weight: f64,
}
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Policy {
    pub id: String,
    pub account: String,
    pub kind: String,
    pub trigger_cash: f64,
    pub limit: f64,
    #[serde(default = "default_policy_delay_days")]
    pub delay_days: usize,
    #[serde(default)]
    pub position: String,
    #[serde(default)]
    pub destination: String,
    #[serde(default)]
    pub execution_cost: f64,
    #[serde(default = "default_policy_funding_rate")]
    pub funding_rate: f64,
    #[serde(default)]
    pub allow_htm_sale: bool,
    #[serde(default)]
    pub htm_sale_limit: f64,
    #[serde(default = "default_policy_funding_tenor_days")]
    pub funding_tenor_days: usize,
}
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Scenario {
    pub name: String,
    #[serde(default = "default_scenario_start_day")]
    pub start_day: usize,
    #[serde(default)]
    pub rate_shift: f64,
    #[serde(default)]
    pub spread_shift: f64,
    #[serde(default)]
    pub deposit_flight: f64,
    #[serde(default = "default_scenario_pd_multiplier")]
    pub pd_multiplier: f64,
    #[serde(default = "default_scenario_migration_multiplier")]
    pub migration_multiplier: f64,
    #[serde(default)]
    pub lgd_add: f64,
    #[serde(default)]
    pub draw_multiplier: f64,
    #[serde(default)]
    pub rollover_loss: f64,
    #[serde(default)]
    pub haircut_add: f64,
    #[serde(default)]
    pub market_shock: f64,
    #[serde(default)]
    pub outage_days: usize,
}
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Cashflow {
    pub position: String,
    pub day: usize,
    #[serde(default)]
    pub principal: f64,
    #[serde(default)]
    pub cash_interest: f64,
    #[serde(default)]
    pub accrual_interest: f64,
    #[serde(default)]
    pub book_amortization: f64,
    #[serde(default = "default_cashflow_scenario")]
    pub scenario: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Spec {
    #[serde(default = "default_spec_horizon_days")]
    pub horizon_days: usize,
    #[serde(default)]
    pub accounts: Vec<Account>,
    #[serde(default)]
    pub positions: Vec<Position>,
    #[serde(default)]
    pub netting_sets: Vec<NettingSet>,
    #[serde(default)]
    pub policies: Vec<Policy>,
    #[serde(default)]
    pub scenarios: Vec<Scenario>,
    #[serde(default)]
    pub reverse_severities: Vec<f64>,
    #[serde(default)]
    pub cashflows: Vec<Cashflow>,
    #[serde(default = "default_spec_ruleset")]
    pub ruleset: String,
}

fn default_account_cet1_floor() -> f64 {
    0.07
}
fn default_account_leverage_floor() -> f64 {
    0.04
}
fn default_account_include_aoci() -> bool {
    true
}
fn default_account_htm_asset_limit() -> f64 {
    1.0
}
fn default_position_payment_interval_days() -> usize {
    30
}
fn default_position_classification() -> SharedText {
    "ac".into()
}
fn default_position_risk_weight() -> f64 {
    1.0
}
fn default_position_rsf_weight() -> f64 {
    1.0
}
fn default_position_lgd() -> f64 {
    0.45
}
fn default_position_watch_pd_multiplier() -> f64 {
    3.0
}
fn default_position_recovery_days() -> usize {
    90
}
fn default_position_hqla_level() -> SharedText {
    "level1".into()
}
fn default_position_opening_market_price() -> f64 {
    1.0
}
fn default_nettingset_margin_delay() -> usize {
    1
}
fn default_nettingset_lgd() -> f64 {
    0.6
}
fn default_nettingset_wrong_way_multiplier() -> f64 {
    1.0
}
fn default_nettingset_risk_weight() -> f64 {
    1.0
}
fn default_policy_delay_days() -> usize {
    1
}
fn default_policy_funding_rate() -> f64 {
    0.05
}
fn default_policy_funding_tenor_days() -> usize {
    30
}
fn default_scenario_start_day() -> usize {
    1
}
fn default_scenario_pd_multiplier() -> f64 {
    1.0
}
fn default_scenario_migration_multiplier() -> f64 {
    1.0
}
fn default_cashflow_scenario() -> String {
    "all".into()
}
fn default_spec_horizon_days() -> usize {
    360
}
fn default_spec_ruleset() -> String {
    "research-weights-v2".into()
}

impl PartialEq<String> for SharedText {
    fn eq(&self, other: &String) -> bool {
        self.as_str() == other.as_str()
    }
}
