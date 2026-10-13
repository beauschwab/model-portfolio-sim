//! Exact saved-trade coverage over existing native derivative accounting owners.
//! This source component does not yet join the mixed Sim or pricing graph.
use crate::products::derivatives::accounting::{
    self as derivative_accounting, DerivativeAccountingRequest, DerivativeAccountingResult,
};
use crate::products::derivatives::contract::{csa::Trade, Identity, Operation};
use crate::products::derivatives::lifecycle::{Side as LegacySide, Swap as LegacySwap};
use crate::products::term::contracts::{identity, indexed::Coupon, text};
use portfolio_compute_control::checkpoint;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

pub const SCHEMA: &str = "saved-derivative-source-1";
pub const MAX_REQUEST_BYTES: usize = 32 * 1024 * 1024;
#[derive(Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct SavedBook {
    pub identity: Identity,
    pub asof_ordinal: i32,
    pub entity: String,
    pub currency: String,
    pub monetary_unit: String,
    pub legacy_hedges: Vec<LegacySwap>,
    pub trades: Vec<Trade>,
}
#[derive(Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct LegacyBinding {
    pub row: u32,
    pub position_id: String,
}
#[derive(Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Request {
    pub schema: String,
    pub model_version: String,
    pub book: SavedBook,
    pub contractual_upgrade_review: Option<Identity>,
    pub legacy_bindings: Vec<LegacyBinding>,
    pub physical: DerivativeAccountingRequest,
}
#[derive(Serialize)]
pub struct ResultData {
    pub schema: &'static str,
    pub book: SavedBook,
    pub contractual_upgrade_review: Option<Identity>,
    pub legacy_bindings: Vec<LegacyBinding>,
    pub source: DerivativeAccountingResult,
    pub exact_saved_mapping_verified: bool,
    pub mixed_daily_journal_adoption: bool,
    pub pricing_graph_adoption: bool,
    pub empirical_validation: bool,
}
fn position(trade: &Trade) -> &str {
    match trade {
        Trade::Swap { request } => &request.position_id,
        Trade::EuropeanSwaption { request } => &request.position_id,
        Trade::ExpiredEuropean { terminal, .. } => &terminal.position_id,
    }
}
fn contract(trade: &Trade) -> &Identity {
    match trade {
        Trade::Swap { request } => &request.contract,
        Trade::EuropeanSwaption { request } => &request.contract,
        Trade::ExpiredEuropean { terminal, .. } => &terminal.contract,
    }
}
fn exact<T: Serialize>(a: &T, b: &T) -> Result<bool, String> {
    Ok(serde_json::to_vec(a).map_err(|e| e.to_string())?
        == serde_json::to_vec(b).map_err(|e| e.to_string())?)
}
pub fn validate_mapping(r: &Request) -> Result<(), String> {
    checkpoint()?;
    portfolio_model_core::strict_json::serialized_size(r, MAX_REQUEST_BYTES)?;
    checkpoint()?;
    if r.schema != SCHEMA || r.model_version != SCHEMA {
        return Err("explicit saved derivative source schema/model required".into());
    }
    let mut metadata = 0;
    identity(&r.book.identity, &mut metadata)?;
    text(&r.book.entity, &mut metadata)?;
    if let Some(review) = &r.contractual_upgrade_review {
        identity(review, &mut metadata)?;
    }
    let Operation::Netting { request: csa } = &r.physical.derivative.operation else {
        return Err("saved source requires complete raw netting/accounting lifecycle".into());
    };
    if r.book.trades.is_empty()
        || r.book.trades.len() > 256
        || r.book.legacy_hedges.len() > 256
        || csa.paths.len() != 1
        || r.book.entity != r.physical.entity
        || r.book.currency != r.physical.currency
        || r.book.monetary_unit != r.physical.monetary_unit
        || r.book.asof_ordinal != csa.asof_ordinal
        || csa.mapping.len() != r.book.trades.len()
    {
        return Err("saved book/header/trade coverage does not match Physical source".into());
    }
    let opening = csa.paths[0]
        .nodes
        .first()
        .ok_or("missing saved opening node")?;
    if opening.ordinal != r.book.asof_ordinal || opening.trades.len() != r.book.trades.len() {
        return Err("saved opening node must cover every original trade".into());
    }
    let mut seen = HashSet::new();
    for (index, trade) in r.book.trades.iter().enumerate() {
        checkpoint()?;
        text(position(trade), &mut metadata)?;
        identity(contract(trade), &mut metadata)?;
        if !seen.insert(position(trade))
            || !exact(trade, &opening.trades[index])?
            || csa.mapping[index].position_id != position(trade)
            || csa.mapping[index].contract != *contract(trade)
        {
            return Err("saved trade identity/order/contract or legal mapping differs".into());
        }
    }
    if r.legacy_bindings.len() != r.book.legacy_hedges.len()
        || (!r.book.legacy_hedges.is_empty() && r.contractual_upgrade_review.is_none())
    {
        return Err(
            "legacy hedge upgrade requires explicit review and complete row coverage".into(),
        );
    }
    let mut rows = HashSet::new();
    let mut owners = HashSet::new();
    for binding in &r.legacy_bindings {
        checkpoint()?;
        text(&binding.position_id, &mut metadata)?;
        let row = binding.row as usize;
        if row >= r.book.legacy_hedges.len()
            || !rows.insert(row)
            || !owners.insert(&binding.position_id)
        {
            return Err("duplicate/outside legacy row or trade owner".into());
        }
        let legacy = &r.book.legacy_hedges[row];
        let Some(Trade::Swap { request }) = r
            .book
            .trades
            .iter()
            .find(|t| position(t) == binding.position_id)
        else {
            return Err("legacy saved swap must map to an explicit complete swap contract".into());
        };
        let same_side = matches!(
            (legacy.side, request.side),
            (
                LegacySide::Payer,
                crate::products::derivatives::contract::swaps::Side::Payer
            ) | (
                LegacySide::Receiver,
                crate::products::derivatives::contract::swaps::Side::Receiver
            )
        );
        let fixed_end = request.fixed_periods.last().map(|p| p.end_ordinal);
        let floating_end = request.floating_periods.last().map(|p| p.end_ordinal);
        if !same_side
            || legacy.notional.to_bits() != request.notional.to_bits()
            || legacy.fixed_rate.to_bits() != request.fixed_rate.to_bits()
            || fixed_end != Some(legacy.maturity)
            || floating_end != Some(legacy.maturity)
            || request.floating_periods.iter().any(|period| {
                !matches!(&period.coupon,
                Coupon::Indexed { margin, .. } if margin.to_bits() == legacy.float_spread.to_bits())
            })
        {
            return Err("reviewed contract upgrade changed a saved swap economic anchor".into());
        }
    }
    checkpoint()
}
pub fn evaluate(r: &Request) -> Result<ResultData, String> {
    // Coverage is refused before calling the native financial source owner.
    validate_mapping(r)?;
    let source = derivative_accounting::evaluate(&r.physical)?;
    checkpoint()?;
    Ok(ResultData {
        schema: SCHEMA,
        book: r.book.clone(),
        contractual_upgrade_review: r.contractual_upgrade_review.clone(),
        legacy_bindings: r.legacy_bindings.clone(),
        source,
        exact_saved_mapping_verified: true,
        mixed_daily_journal_adoption: false,
        pricing_graph_adoption: false,
        empirical_validation: false,
    })
}
