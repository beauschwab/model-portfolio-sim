use portfolio_model_core::cashflow::ordinal_from_ymd;
use serde_json::{json, Value};

pub fn request(pay: u32, end: u32, interest: f64, principal: f64, payment: bool) -> Value {
    let asof = ordinal_from_ymd(2024, 2, 28).unwrap();
    let s = if payment { -1. } else { 1. };
    json!({"schema":"dated-ledger-1","monetary_unit":"currency_units",
        "recognition_policy":{"policy_id":"explicit-coupon","policy_revision":"r1","recognition_model":"supplied_calendar_segments-1","fees":"immediate_earnings"},
        "specification":{"version":"balance-stress-2","horizon_days":30,
            "accounts":[{"id":"a","entity":"bank","currency":"USD","cash":if payment{100.}else{0.},"equity":if payment{0.}else{100.},"cet1_floor":0.,"leverage_floor":0.}],
            "positions":[{"id":"p","account":"a","kind":if payment{"deposit"}else{"loan"},"balance":100.,"rate":0.5,"maturity_day":1}],
            "scenarios":[{"name":"shock","rate_shift":0.02,"deposit_flight":0.5,"pd_multiplier":10.}]},
        "dated":{"schema":"dated-store-1","as_of_ordinal":asof,"reporting_horizon_days":30,
            "runs":[{"run_id":"base","scenario_id":"baseline","severity":1.,"input_revision":"base-1"},{"run_id":"stress","scenario_id":"shock","severity":1.,"input_revision":"shock-1"}],
            "owners":[{"position_id":"p","account_id":"a","currency":"USD","direction":if payment{"payment"}else{"receipt"},"ownership":"contractual_dated_native_state","source_id":"terms","source_revision":"r1"}],
            "sources":[{"source_id":"terms","source_revision":"r1","coverage":[{"position_id":"p","run":{"kind":"all"},"status":{"kind":"complete_event_set"}}],
                "schedule":{"schema":"dated-cashflow-1","as_of_ordinal":asof,"horizon_days":end.max(pay).max(30),"scenarios":["base","stress"],
                    "events":[{"position_id":"p","event_id":"coupon-1","scenario":{"kind":"all"},"payment_ordinal":asof+pay as i32,"day_offset":pay,
                        "accrual_period":{"start_ordinal":asof,"end_ordinal":asof+end as i32},"currency":"USD",
                        "components":{"principal":s*principal,"cash_interest":s*interest,"accrued_interest":s*interest,"book_amortization":0.,"fees":0.},
                        "measure":"contractual","provenance":{"kind":"contractual","source_id":"contract-p","source_revision":"r1"},"adjustment":{"kind":"none"}}]}}],
            "accrual_plans":[{"position_id":"p","event_id":"coupon-1","run":{"kind":"all"},"segments":[{"start_ordinal":asof,"end_ordinal":asof+end as i32,"accrued_interest":s*interest,"book_amortization":0.}]}]}})
}
