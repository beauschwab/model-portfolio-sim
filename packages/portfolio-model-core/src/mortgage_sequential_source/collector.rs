//! Retain actual borrowed receipts only after the raw owner admitted both sources.
use super::*;
use crate::mortgage_receipts::*;
#[derive(Default)]
struct Collector {
    intervals: Vec<MortgageIntervalReceipt>,
    capture: Capture,
    pending: Option<Boundary>,
    expected: usize,
}
impl MortgageReceiptSink for Collector {
    fn admit(&mut self, v: AdmissionView<'_>) -> Result<(), String> {
        admit_receipts(&v.request.identity, &v.request.ownership, v.interval_count)?;
        self.expected = v.interval_count;
        Ok(())
    }
    fn interval(&mut self, v: IntervalView<'_>) -> Result<(), String> {
        if self.pending.is_some()
            || v.interval_index != self.intervals.len()
            || self.intervals.len() >= self.expected
        {
            return Err("same-loop interval ownership/order mismatch".into());
        }
        self.intervals.push(MortgageIntervalReceipt {
            schema: super::RECEIPT_SCHEMA,
            financial_source_sha256: generator_source_sha256(),
            identity: v.request.identity.clone(),
            ownership: serde_json::to_value(&v.request.ownership).map_err(|e| e.to_string())?,
            interval_index: v.interval_index,
            start_ordinal: v.start_ordinal,
            end_ordinal: v.end_ordinal,
            path_start_ordinal: v.path_start_ordinal,
            duration_years_act365f: v.duration_years_act365f,
            contractual_day_fraction: v.contractual_day_fraction,
            note_rate_before_boundary: v.note_rate_before_boundary,
            investor_coupon_before_boundary: v.investor_coupon_before_boundary,
            payment_before_boundary: v.payment_before_boundary,
            reset_count_before_boundary: v.reset_count_before_boundary,
            burnout_before_interval: v.burnout_before_interval,
            loan_operator_request: v.loan_operator_request.clone(),
            investor_operator_request: v.investor_operator_request.clone(),
            loan_transition: *v.loan_transition,
            loan_earned_interest: *v.loan_earned_interest,
            investor_transition: *v.investor_transition,
            investor_earned_interest: *v.investor_earned_interest,
            opening_probability: *v.opening_probability,
            opening_borrower_principal: *v.opening_borrower_principal,
            opening_borrower_interest: *v.opening_borrower_interest,
            opening_investor_principal: *v.opening_investor_principal,
            opening_investor_interest: *v.opening_investor_interest,
            default_claim_start_index: v.default_claim_start_index,
            payment_is_boundary: v.payment_is_boundary,
            arm_reset_is_boundary: v.arm_reset_is_boundary,
            event_order:
                "interval_transport_default_capture_prepayment_payment_arm_reset_snapshot-1",
        });
        self.pending = Some(Boundary {
            interval_index: v.interval_index,
            ordinal: v.end_ordinal,
            transported_loan_principal: v.transported_loan.principal,
            transported_loan_interest: v.transported_loan.accrued_interest,
            transported_investor_principal: v.transported_investor.principal,
            transported_investor_interest: v.transported_investor.accrued_interest,
            default_claim_start: v.default_claim_start_index,
            default_claim_end: v.default_claim_start_index,
            default_recoveries: vec![],
            prepayments: vec![],
            scheduled: vec![],
            closing_investor_principal: [0.; 3],
            closing_investor_interest: [0.; 3],
        });
        Ok(())
    }
    fn recovery(&mut self, v: RecoveryView<'_>) -> Result<(), String> {
        self.pending
            .as_mut()
            .ok_or("recovery outside actual interval")?
            .default_recoveries
            .push(DefaultRecovery {
                claim_index: v.claim_index,
                origin: v.origin,
                tail_index: v.tail_index,
                ordinal: v.ordinal,
                principal: v.principal,
                entitlement_id: v.entitlement_id.map(str::to_owned),
            });
        Ok(())
    }
    fn prepayment(&mut self, v: PrepaymentView<'_>) -> Result<(), String> {
        self.pending
            .as_mut()
            .ok_or("payoff outside actual interval")?
            .prepayments
            .push(Prepayment {
                kind: v.kind.into(),
                fraction: v.fraction,
                principal: v.principal,
                interest: v.interest,
                recognized_interest: v.recognized_interest,
                entitlement_id: v.entitlement_id.map(str::to_owned),
            });
        Ok(())
    }
    fn scheduled(&mut self, v: ScheduledView<'_>) -> Result<(), String> {
        self.pending
            .as_mut()
            .ok_or("scheduled cash outside actual interval")?
            .scheduled
            .push(Scheduled {
                origin: v.origin,
                principal: v.principal,
                interest: v.interest,
                recognized_interest: v.recognized_interest,
                entitlement_id: v.entitlement_id.map(str::to_owned),
            });
        Ok(())
    }
    fn boundary(&mut self, v: BoundaryView<'_>) -> Result<(), String> {
        let mut b = self
            .pending
            .take()
            .ok_or("boundary without actual interval")?;
        if b.interval_index != v.interval_index || b.ordinal != v.ordinal {
            return Err("native boundary order mismatch".into());
        }
        b.default_claim_end = v.default_claim_end_index;
        b.closing_investor_principal = *v.closing_investor_principal;
        b.closing_investor_interest = *v.closing_investor_interest;
        self.capture.boundaries.push(b);
        Ok(())
    }
    fn cash_link(&mut self, v: CashLinkView<'_>) -> Result<(), String> {
        self.capture.cash_links.push(CashLink {
            entitlement_id: v.entitlement_id.into(),
            cash_event_id: v.cash_event_id.into(),
        });
        Ok(())
    }
    fn complete(&mut self, result: &MortgageResult) -> Result<(), String> {
        if self.pending.is_some()
            || self.intervals.len() != self.expected
            || self.capture.boundaries.len() != self.expected
            || self.capture.cash_links.len() != result.entitlements.len()
        {
            return Err("incomplete actual-loop source receipt".into());
        }
        Ok(())
    }
}
pub fn project_with_check(
    r: &MortgageRequest,
    check: &mut dyn FnMut() -> Result<(), String>,
) -> Result<ResultData, String> {
    let mut collector = Collector::default();
    let native_projection = crate::mortgage_state::project_with_receipts(r, &mut collector, check)?;
    Ok(ResultData {
        native_projection,
        intervals: collector.intervals,
        capture: collector.capture,
    })
}
