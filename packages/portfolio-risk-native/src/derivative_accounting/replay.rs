//! Immutable replay from native marks, contractual rights and collateral/default outputs.
use super::{
    inputs::*,
    journal::{Engine, TradeState},
    types::*,
};
use crate::derivative_contract::{
    self, amount, csa::Trade, near, options, swaps, DerivativeResult,
};
use std::collections::{BTreeMap, HashMap};
pub(super) fn evaluate(
    r: &DerivativeAccountingRequest,
) -> Result<DerivativeAccountingResult, String> {
    admit(r)?;
    let c = csa_request(&r.derivative)?;
    let path = &c.paths[0];
    let mut cash_inputs = CashInputs::new(r);
    // Validate/derive Physical cash under the already bounded request before allocating a full native path result.
    let mut normal_cash: BTreeMap<(i32, String), Vec<CashRight>> = BTreeMap::new();
    for node in &path.nodes {
        let after_default = path
            .default
            .as_ref()
            .is_some_and(|d| node.ordinal >= d.default_ordinal);
        let opening_paid = node.ordinal == c.asof_ordinal
            && node.payment_timing == swaps::PaymentTiming::AfterScheduledPayments;
        let default_after_payment = path
            .default
            .as_ref()
            .is_some_and(|d| node.ordinal == d.default_ordinal)
            && node.payment_timing == swaps::PaymentTiming::AfterScheduledPayments
            && node.ordinal > c.asof_ordinal;
        for trade in &node.trades {
            if (!after_default && !opening_paid) || default_after_payment {
                normal_cash.insert(
                    (node.ordinal, position(trade).into()),
                    cash_inputs.due(trade, node)?,
                );
            }
        }
    }
    cash_inputs.finish()?;
    let DerivativeResult::Netting(native) = derivative_contract::evaluate(&r.derivative)? else {
        return Err("native accounting lifecycle did not return netting result".into());
    };
    let pricing_xva_diagnostic = if let Some(raw) = &r.pricing_xva_reference {
        let DerivativeResult::Netting(x) = derivative_contract::evaluate(raw)? else {
            return Err("native XVA diagnostic result mismatch".into());
        };
        if !near(x.clean_value, native.clean_value) || x.crn != native.crn {
            return Err("pricing XVA reference differs from opening clean/CRN convention".into());
        }
        Some(x)
    } else {
        None
    };
    let result_path = native
        .paths
        .first()
        .ok_or("native Physical lifecycle path missing")?;
    let opening_values = raw_values(result_path, c.asof_ordinal);
    let mut trades = BTreeMap::new();
    let mut buckets = BTreeMap::new();
    for anchor in &r.opening.trades {
        let mark = opening_values
            .get(&anchor.position_id)
            .ok_or("opening anchor lacks native clean mark")?;
        if !near(anchor.clean_value, *mark) {
            return Err(
                "opening derivative carrying value does not reconcile native clean anchor".into(),
            );
        }
        let raw = path.nodes[0]
            .trades
            .iter()
            .find(|t| position(t) == anchor.position_id)
            .ok_or("opening raw contract missing")?;
        let b = bucket(c, &anchor.position_id)?;
        buckets.insert(b.clone(), ());
        trades.insert(
            anchor.position_id.clone(),
            TradeState {
                owner: owner(raw).into(),
                bucket: b,
                value: anchor.clean_value,
            },
        );
    }
    for (a, b) in [
        (r.opening.posted_vm_asset, (-c.policy.opening_vm).max(0.)),
        (r.opening.received_vm_liability, c.policy.opening_vm.max(0.)),
        (r.opening.posted_im_asset, c.policy.opening_posted_im),
        (r.opening.received_im_custody, c.policy.opening_received_im),
    ] {
        if !near(a, b) {
            return Err(
                "opening accounting collateral does not reconcile native CSA anchors".into(),
            );
        }
    }
    let bucket_ids = buckets.into_keys().collect::<Vec<_>>();
    let mut engine = Engine::new(r, trades, &bucket_ids)?;
    let mut daily = Vec::new();
    let mut rights = Vec::new();
    let reviews = r
        .policy
        .trades
        .iter()
        .map(|p| (p.position_id.as_str(), p))
        .collect::<HashMap<_, _>>();
    let mut known_rights: HashMap<(String, String, i32), usize> = HashMap::new();
    let mut closed = false;
    for node in &path.nodes {
        let current = raw_values(result_path, node.ordinal);
        let closeouts = result_path
            .closeouts
            .iter()
            .filter(|cl| cl.ordinal == node.ordinal)
            .collect::<Vec<_>>();
        let at_default = path
            .default
            .as_ref()
            .is_some_and(|d| d.default_ordinal == node.ordinal);
        let after_default = path
            .default
            .as_ref()
            .is_some_and(|d| node.ordinal >= d.default_ordinal);
        let default_after =
            at_default && node.payment_timing == swaps::PaymentTiming::AfterScheduledPayments;
        let opening_paid = node.ordinal == c.asof_ordinal
            && node.payment_timing == swaps::PaymentTiming::AfterScheduledPayments;
        for movement in result_path.movements.iter().filter(|m| {
            m.ordinal == node.ordinal
                && m.kind != "default_closeout"
                && m.kind != "mpor_contractual_settlement"
        }) {
            let source = if movement.kind == "collateral_interest" {
                vec![
                    c.policy.identity.clone(),
                    node.collateral_rate_source.clone(),
                ]
            } else {
                vec![c.policy.identity.clone(), result_path.source.clone()]
            };
            if movement.kind != "variation_margin"
                && movement.kind != "collateral_interest"
                && movement.kind != "tail_return"
            {
                return Err("unknown native collateral movement; no accounting fallback".into());
            }
            engine.collateral(
                &movement.kind,
                node.ordinal,
                &movement.bucket,
                movement.entity_cash,
                movement.vm_delta,
                movement.posted_im_delta,
                movement.received_im_delta,
                source,
            )?;
        }
        for trade in &node.trades {
            let id = position(trade);
            let review = reviews.get(id).ok_or("unreviewed native trade")?;
            let raw_mark = *current.get(id).ok_or("missing native current clean mark")?;
            let due = normal_cash
                .remove(&(node.ordinal, id.into()))
                .unwrap_or_default();
            let mpor = result_path
                .obligations
                .iter()
                .filter(|o| o.position_id == id && o.captured_ordinal == node.ordinal)
                .collect::<Vec<_>>();
            let mut derived = due;
            for obligation in mpor {
                let pricing_due = mpor_pricing_amount(trade, node.ordinal, &obligation.event_id)?;
                derived.push(CashRight {
                    position_id: id.into(),
                    contract_position_id: owner(trade).into(),
                    event_id: obligation.event_id.clone(),
                    payment_ordinal: obligation.payment_ordinal,
                    physical_amount: obligation.amount,
                    pricing_due_amount: pricing_due,
                    contract: obligation.contract.clone(),
                    recognition_sources: {
                        let mut ids = source_ids(trade);
                        ids.extend(obligation.settlement_price_sources.iter().cloned());
                        if let Some(e) = &obligation.origin_election {
                            ids.push(e.clone());
                        }
                        if let Some(p) = &result_path.mpor_policy {
                            ids.push(p.clone());
                        }
                        ids
                    },
                    sources: obligation.selected_rate_sources.clone(),
                    physical_path: None,
                    settled_ordinal: obligation.settled_ordinal,
                    closeout_ordinal: obligation.included_in_closeout.then_some(
                        path.default
                            .as_ref()
                            .ok_or("missing native default")?
                            .closeout_ordinal,
                    ),
                });
            }
            let due_pricing = derived
                .iter()
                .try_fold(0., |a, d| amount(a + d.pricing_due_amount))?;
            let add_due_to_raw = node.payment_timing
                == swaps::PaymentTiming::AfterScheduledPayments
                && !opening_paid
                && !closed;
            let pre_target = if closed {
                0.
            } else {
                amount(raw_mark + if add_due_to_raw { due_pricing } else { 0. })?
            };
            let mut sources = source_ids(trade);
            sources.push(review.designation_review.clone());
            engine.mark(id, pre_target, node.ordinal, sources.clone())?;
            if !closed && owner(trade) != engine.trades.get(id).ok_or("missing owner slot")?.owner {
                let delivery_review = review
                    .delivery_review
                    .as_ref()
                    .ok_or("physical delivery lacks reviewed recognition")?;
                if let Trade::ExpiredEuropean {
                    terminal,
                    delivered_underlying: Some(_),
                    ..
                } = trade
                {
                    if terminal.expiry_ordinal == node.ordinal {
                        let result = options::value_european(terminal)?;
                        let sign = if terminal.side == options::OptionSide::Long {
                            1.
                        } else {
                            -1.
                        };
                        if !near(pre_target, sign * result.states[0].intrinsic_value) {
                            return Err("expiry delivery clean mark inconsistent with reviewed terminal exercise value".into());
                        }
                    }
                }
                sources.push(delivery_review.clone());
                engine.transfer_owner(id, owner(trade), node.ordinal, sources.clone())?;
            }
            for cash in derived {
                let key = (id.into(), cash.event_id.clone(), cash.payment_ordinal);
                if known_rights.insert(key, rights.len()).is_some() {
                    return Err("native contractual cash recognized twice".into());
                }
                let unpaid = after_default && !default_after && !closed;
                let mut cash_sources = sources.clone();
                cash_sources.extend(cash.recognition_sources.iter().cloned());
                for selected in &cash.sources {
                    match selected {
                        crate::term_enhancements::indexed::RateSource::Observed {
                            fixing, ..
                        } => cash_sources.push(fixing.clone()),
                        crate::term_enhancements::indexed::RateSource::ConditionalProjection {
                            model,
                            path,
                            ..
                        } => cash_sources.extend([model.clone(), path.clone()]),
                    }
                }
                if let Some(p) = &cash.physical_path {
                    cash_sources.push(p.clone());
                }
                engine.due(&cash, unpaid, review, node.ordinal, cash_sources)?;
                rights.push(cash);
            }
            let remaining = if closed {
                0.
            } else {
                amount(
                    raw_mark
                        - if node.payment_timing == swaps::PaymentTiming::BeforeScheduledPayments {
                            due_pricing
                        } else {
                            0.
                        },
                )?
            };
            engine.mark(id, remaining, node.ordinal, sources)?;
        }
        // Native MPOR settlements are reconstructed by identity from retained rights, not posted again from aggregate movements.
        let settled = result_path
            .obligations
            .iter()
            .filter(|o| o.settled_ordinal == Some(node.ordinal))
            .collect::<Vec<_>>();
        let mut settled_sum: BTreeMap<String, f64> = BTreeMap::new();
        for o in settled {
            let key = (o.position_id.clone(), o.event_id.clone(), o.payment_ordinal);
            let idx = *known_rights
                .get(&key)
                .ok_or("settled native MPOR right missing accounting capture")?;
            let right = &rights[idx];
            let mut sources = vec![o.contract.clone(), r.policy.identity.clone()];
            if let crate::derivative_contract::csa::ObligationDisposition::SettleOnNode {
                evidence,
                ..
            } = &o.disposition
            {
                sources.push(evidence.clone());
            }
            engine.settle_unpaid(right, node.ordinal, sources)?;
            let value = settled_sum.entry(o.bucket.clone()).or_default();
            *value = amount(*value + o.amount)?;
        }
        for b in &bucket_ids {
            let native_cash = result_path
                .movements
                .iter()
                .filter(|m| {
                    m.ordinal == node.ordinal
                        && m.bucket == *b
                        && m.kind == "mpor_contractual_settlement"
                })
                .try_fold(0., |a, m| amount(a + m.entity_cash))?;
            if !near(native_cash, settled_sum.get(b).copied().unwrap_or(0.)) {
                return Err(
                    "MPOR settlement cash does not reconcile source ledger/native movement".into(),
                );
            }
        }
        for close in closeouts {
            let movement = result_path
                .movements
                .iter()
                .find(|m| {
                    m.ordinal == node.ordinal
                        && m.bucket == close.bucket
                        && m.kind == "default_closeout"
                })
                .ok_or("native default closeout movement missing")?;
            if !near(movement.entity_cash, close.entity_cash) {
                return Err("native default cash mismatch".into());
            }
            let mut sources = vec![c.policy.identity.clone(), result_path.source.clone()];
            if let Some(p) = &result_path.mpor_policy {
                sources.push(p.clone());
            }
            if let Some(p) = &result_path.closing_legal_review {
                sources.push(p.clone());
            }
            if let Some(p) = &result_path.replacement_value_policy {
                sources.push(p.clone());
            }
            engine.closeout(close, node.ordinal, sources)?;
        }
        if path
            .default
            .as_ref()
            .is_some_and(|d| d.closeout_ordinal == node.ordinal)
        {
            closed = true;
        }
        daily.push(engine.closing(node.ordinal)?);
    }
    if !normal_cash.is_empty() {
        return Err("unprocessed Physical cash date tail".into());
    }
    let closing = daily.last().ok_or("missing daily closing tail")?;
    if closing.trades.iter().any(|t| !near(t.clean_carrying, 0.))
        || !near(closing.unpaid_receivable, 0.)
        || !near(closing.unpaid_payable, 0.)
        || !near(
            closing.posted_vm_asset,
            result_path.residual_vm.min(0.).abs(),
        )
        || !near(
            closing.received_vm_liability,
            result_path.residual_vm.max(0.),
        )
        || !near(closing.posted_im_asset, result_path.residual_posted_im)
        || !near(
            closing.received_im_custody,
            result_path.residual_received_im,
        )
    {
        return Err(
            "full derivative rights/collateral tail did not reconcile native closing".into(),
        );
    }
    Ok(DerivativeAccountingResult{schema:super::SCHEMA.into(),model_version:super::MODEL_VERSION.into(),run:r.run.clone(),measure:r.measure,basis:r.basis,entity:r.entity.clone(),currency:r.currency.clone(),monetary_unit:r.monetary_unit.clone(),policy:r.policy.clone(),opening:r.opening.clone(),intents:engine.intents,daily,cash_rights:rights,native,pricing_xva_diagnostic,disclosure:"Reviewed modeled Physical daily scenario intents, not observed transactions or accounting-standard/hedge-designation validation. Raw clean marks and native cash-right lineage retained. Clean-only earnings and coupon presentation are explicit; Pricing XVA/funding diagnostics never create journal cash or automatic valuation adjustment. Segregated received IM is custody memo only; custodian interest flows remain third-party diagnostics. Gross derivative/VM assets/liabilities, unpaid rights, physical delivery and reviewed default shortfalls are reconciled exactly once. No persistence, independent publication verification, automatic capital/FTP or formal hedge accounting.".into()})
}
fn mpor_pricing_amount(trade: &Trade, date: i32, event_id: &str) -> Result<f64, String> {
    match trade {
        Trade::Swap { request } => swap_pricing_due(request, date, event_id),
        Trade::ExpiredEuropean {
            terminal,
            delivered_underlying,
            ..
        } => {
            if let Some(u) = delivered_underlying {
                swap_pricing_due(u, date, event_id)
            } else {
                let result = options::value_european(terminal)?;
                let sign = if terminal.side == options::OptionSide::Long {
                    1.
                } else {
                    -1.
                };
                if event_id != format!("option_cash:{}", result.states[0].state_id)
                    || terminal.settlement_ordinal != date
                {
                    return Err("native MPOR option cash event identity mismatch".into());
                }
                amount(sign * result.states[0].settlement_cash)
            }
        }
        Trade::EuropeanSwaption { .. } => {
            Err("pre-expiry option cannot have native contractual cash".into())
        }
    }
}
fn swap_pricing_due(r: &swaps::SwapRequest, date: i32, event_id: &str) -> Result<f64, String> {
    let mut before = r.clone();
    before.payment_timing = swaps::PaymentTiming::BeforeScheduledPayments;
    let priced = swaps::value_swap(&before)?;
    priced
        .schedule
        .events
        .iter()
        .find(|e| e.payment_ordinal == date && e.event_id == event_id)
        .ok_or_else(|| "native MPOR coupon identity absent raw pricing contract".to_string())?
        .components
        .cash_total()
}
