//! Shared bounded JSON protocol for the standalone executable and Python adapter.
use crate::mortgage_risk::RiskResult;
use crate::term_deck::{Deck, DeckRequest};
use crate::term_risk::TermRiskRequest;
use serde::{Deserialize, Serialize};
use std::ffi::{c_char, CString};
use std::io::{self, Write};
pub const MAX_INPUT: usize = 64 * 1024 * 1024;
const MAX_OUTPUT: usize = 128 * 1024 * 1024;

#[derive(Deserialize)]
#[serde(tag = "schema", deny_unknown_fields)]
enum Request {
    #[serde(rename = "floating-rate-1")]
    FloatingRate {
        threads: usize,
        request: Box<portfolio_model_core::floating_rate::FloatingRateRequest>,
    },
    #[serde(rename = "dated-term-1")]
    DatedTerm {
        threads: usize,
        request: Box<crate::dated_term::DatedTermRequest>,
    },
    #[serde(rename = "observed-credit-calibration-1")]
    ObservedCreditCalibration {
        threads: usize,
        request: Box<portfolio_model_core::credit_calibration::CreditCalibrationRequest>,
    },
    #[serde(rename = "credit-model-1")]
    CreditModel {
        threads: usize,
        request: Box<portfolio_model_core::credit::CreditRequest>,
    },
    #[serde(rename = "dated-cashflow-1")]
    DatedCashflow {
        threads: usize,
        request: Box<crate::model_lifecycle::CashflowRequest>,
    },
    #[serde(rename = "treasury-bridge-1")]
    TreasuryBridge {
        threads: usize,
        request: Box<crate::treasury_bridge::Request>,
    },
    #[serde(rename = "treasury-1")]
    Treasury {
        threads: usize,
        request: Box<crate::treasury::Request>,
    },
    #[serde(rename = "cohort-build-1")]
    Cohorts {
        threads: usize,
        request: Box<crate::cohorts::Request>,
    },
    #[serde(rename = "financial-controller-1")]
    Controller {
        threads: usize,
        request: Box<crate::controllers::Request>,
    },
    #[serde(rename = "whatif-overrides-1")]
    WhatIf {
        threads: usize,
        request: crate::whatif::Request,
    },
    #[serde(rename = "graph-1")]
    Graph {
        threads: usize,
        request: Box<crate::incremental::Request>,
    },
    #[serde(rename = "unit-coefficients-1")]
    Coefficients {
        threads: usize,
        request: Box<crate::unit_lifecycle::CoefficientRequest>,
    },
    #[serde(rename = "ledger-map-1")]
    LedgerMap {
        threads: usize,
        request: Box<crate::ledger_mapping::MappingRequest>,
    },
    #[serde(rename = "programs-1")]
    Programs {
        threads: usize,
        request: Box<crate::program_lifecycle::ProgramRequest>,
    },
    #[serde(rename = "unit-library-1")]
    UnitLibrary {
        threads: usize,
        request: Box<crate::unit_lifecycle::UnitRequest>,
    },
    #[serde(rename = "balance-risk-1")]
    BalanceRisk {
        threads: usize,
        request: Box<crate::balance_risk::BalanceRiskRequest>,
    },
    #[serde(rename = "kpi-1")]
    Kpi {
        threads: usize,
        request: Box<crate::kpi_lifecycle::KpiRequest>,
    },
    #[serde(rename = "accounting-1")]
    Accounting {
        threads: usize,
        request: Box<crate::accounting_lifecycle::AccountingRequest>,
    },
    #[serde(rename = "hedge-risk-1")]
    HedgeRisk {
        threads: usize,
        request: Box<crate::hedge_lifecycle::HedgeRequest>,
    },
    #[serde(rename = "hedge-deck-1")]
    HedgeDeck {
        threads: usize,
        request: crate::hedge_lifecycle::HedgeDeckInput,
    },
    #[serde(rename = "deposit-risk-1")]
    DepositRisk {
        threads: usize,
        request: Box<crate::deposit_lifecycle::DepositRequest>,
    },
    #[serde(rename = "deposit-stress-1")]
    DepositStress {
        threads: usize,
        request: Box<crate::deposit_lifecycle::DepositRequest>,
    },
    #[serde(rename = "deposit-deck-1")]
    DepositDeck {
        threads: usize,
        request: DepositDeckInput,
    },
    #[serde(rename = "term-risk-1")]
    Risk {
        threads: usize,
        request: Box<TermRiskRequest>,
    },
    #[serde(rename = "term-deck-1")]
    Deck {
        threads: usize,
        request: DeckRequest,
    },
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct DepositDeckInput {
    book: Vec<crate::deposit_lifecycle::Cohort>,
    assumptions: crate::deposit_lifecycle::Assumptions,
}
#[derive(Serialize)]
#[serde(untagged)]
enum Output {
    Coefficients(Vec<crate::unit_lifecycle::Coefficients>),
    Programs(Box<crate::program_lifecycle::ProgramResult>),
    UnitLibrary(Box<crate::unit_lifecycle::UnitLibrary>),
    BalanceRisk(Box<crate::balance_risk::BalanceRiskResult>),
    Kpi(serde_json::Value),
    Accounting(Box<crate::accounting_lifecycle::AccountingResult>),
    HedgeRisk(Box<crate::hedge_lifecycle::HedgeRisk>),
    HedgeDeck(Box<crate::hedge_lifecycle::HedgeDeck>),
    DepositRisk(Box<crate::deposit_lifecycle::DepositRisk>),
    DepositStress(Box<crate::deposit_lifecycle::DepositStress>),
    DepositDeck(Box<crate::deposit_lifecycle::DepositDeck>),
    Risk(RiskResult),
    Deck(Box<Deck>),
}
#[derive(Serialize)]
#[serde(untagged)]
enum Response {
    Success { ok: bool, result: Output },
    Error { ok: bool, error: String },
}
struct BoundedOutput(Vec<u8>);
impl Write for BoundedOutput {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        if bytes.len() > MAX_OUTPUT - self.0.len() {
            return Err(io::Error::other("term response exceeds 128 MiB"));
        }
        self.0.extend_from_slice(bytes);
        Ok(bytes.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}
fn execute(input: &[u8]) -> Result<Output, String> {
    if input.len() > MAX_INPUT {
        return Err("term request exceeds 64 MiB".into());
    }
    let request: Request = serde_json::from_slice(input).map_err(|e| e.to_string())?;
    let threads = match &request {
        Request::FloatingRate { threads, .. }
        | Request::DatedTerm { threads, .. }
        | Request::ObservedCreditCalibration { threads, .. }
        | Request::CreditModel { threads, .. }
        | Request::DatedCashflow { threads, .. }
        | Request::TreasuryBridge { threads, .. }
        | Request::Treasury { threads, .. }
        | Request::Cohorts { threads, .. }
        | Request::Controller { threads, .. }
        | Request::WhatIf { threads, .. }
        | Request::Graph { threads, .. }
        | Request::Coefficients { threads, .. }
        | Request::LedgerMap { threads, .. }
        | Request::Programs { threads, .. }
        | Request::UnitLibrary { threads, .. }
        | Request::BalanceRisk { threads, .. }
        | Request::Kpi { threads, .. }
        | Request::Accounting { threads, .. }
        | Request::HedgeRisk { threads, .. }
        | Request::HedgeDeck { threads, .. }
        | Request::Risk { threads, .. }
        | Request::Deck { threads, .. }
        | Request::DepositRisk { threads, .. }
        | Request::DepositStress { threads, .. }
        | Request::DepositDeck { threads, .. } => *threads,
    };
    if !(1..=256).contains(&threads) {
        return Err("invalid native thread count".into());
    }
    let pool = crate::pool(threads).map_err(|_| "native thread pool unavailable")?;
    pool.install(|| match request {
        Request::FloatingRate { request, .. } => {
            crate::model_lifecycle::floating_rate(&request).map(Output::Kpi)
        }
        Request::DatedTerm { request, .. } => {
            crate::model_lifecycle::dated_term(&request).map(Output::Kpi)
        }
        Request::ObservedCreditCalibration { request, .. } => {
            crate::model_lifecycle::fit_observed_credit(&request).map(Output::Kpi)
        }
        Request::CreditModel { request, .. } => {
            crate::model_lifecycle::credit(&request).map(Output::Kpi)
        }
        Request::DatedCashflow { request, .. } => {
            crate::model_lifecycle::cashflows(&request).map(Output::Kpi)
        }
        Request::TreasuryBridge { request, .. } => request.run().map(Output::Kpi),
        Request::Treasury { request, .. } => request.run().map(Output::Kpi),
        Request::Cohorts { request, .. } => request.run().map(Output::Kpi),
        Request::Controller { request, .. } => request.run().map(Output::Kpi),
        Request::WhatIf { request, .. } => request.run().map(Output::Kpi),
        Request::Graph { request, .. } => request.run().map(Output::Kpi),
        Request::Coefficients { request, .. } => request.run().map(Output::Coefficients),
        Request::LedgerMap { request, .. } => request.run().map(Output::Kpi),
        Request::Programs { request, .. } => request.run().map(|v| Output::Programs(Box::new(v))),
        Request::UnitLibrary { request, .. } => {
            request.run().map(|v| Output::UnitLibrary(Box::new(v)))
        }
        Request::BalanceRisk { request, .. } => {
            request.run().map(|v| Output::BalanceRisk(Box::new(v)))
        }
        Request::Kpi { request, .. } => request.run().map(Output::Kpi),
        Request::Accounting { request, .. } => {
            request.run().map(|v| Output::Accounting(Box::new(v)))
        }
        Request::HedgeRisk { request, .. } => request.run().map(|v| Output::HedgeRisk(Box::new(v))),
        Request::HedgeDeck { request, .. } => {
            crate::hedge_lifecycle::HedgeDeck::new(&request.swaps, request.asof, request.months)
                .map(|v| Output::HedgeDeck(Box::new(v)))
        }
        Request::DepositRisk { request, .. } => {
            request.risk().map(|v| Output::DepositRisk(Box::new(v)))
        }
        Request::DepositStress { request, .. } => {
            request.stress().map(|v| Output::DepositStress(Box::new(v)))
        }
        Request::DepositDeck { request, .. } => {
            crate::deposit_lifecycle::DepositDeck::new(&request.book, &request.assumptions)
                .map(|v| Output::DepositDeck(Box::new(v)))
        }
        Request::Risk { request, .. } => request.run().map(Output::Risk),
        Request::Deck { request, .. } => request.build().map(|deck| Output::Deck(Box::new(deck))),
    })
}
pub fn respond(input: &[u8]) -> (Vec<u8>, bool) {
    let result = std::panic::catch_unwind(|| execute(input))
        .unwrap_or_else(|_| Err("native term request failed validation".into()));
    let success = result.is_ok();
    let response = match result {
        Ok(result) => Response::Success { ok: true, result },
        Err(error) => Response::Error { ok: false, error },
    };
    let mut output = BoundedOutput(Vec::new());
    if let Err(error) = serde_json::to_writer(&mut output, &response) {
        return (
            serde_json::to_vec(&Response::Error {
                ok: false,
                error: error.to_string(),
            })
            .unwrap(),
            false,
        );
    }
    (output.0, success)
}
/// One batched raw-contract request. Returned allocation must be freed exactly once.
///
/// # Safety
/// input must point to length live bytes for the duration of this synchronous call.
#[no_mangle]
pub unsafe extern "C" fn portfolio_term_request(input: *const u8, length: usize) -> *mut c_char {
    let bytes = if input.is_null() || length > MAX_INPUT {
        serde_json::to_vec(&Response::Error {
            ok: false,
            error: "null or oversized term input".into(),
        })
        .unwrap()
    } else {
        // SAFETY: caller owns the admitted live input range for this call.
        respond(unsafe { std::slice::from_raw_parts(input, length) }).0
    };
    CString::new(bytes).unwrap().into_raw()
}
/// # Safety
/// pointer must be an unfreed allocation returned by portfolio_term_request.
#[no_mangle]
pub unsafe extern "C" fn portfolio_term_free(pointer: *mut c_char) {
    if !pointer.is_null() {
        // SAFETY: the caller returns this library's owned CString once.
        drop(unsafe { CString::from_raw(pointer) });
    }
}

/// Raw JSON input with contiguous numeric outputs, avoiding bulk result JSON.
/// Returns a small JSON status allocation, freed with portfolio_term_free.
///
/// # Safety
/// input must own length live bytes. outputs must point to four live descriptors
/// with exclusive, aligned f64 storage of the stated lengths. Output ranges must
/// not overlap each other, descriptors or input. No pointer survives this call.
#[no_mangle]
pub unsafe extern "C" fn portfolio_term_risk_into(
    input: *const u8,
    length: usize,
    outputs: *const crate::quant::Buffer,
    count: usize,
) -> *mut c_char {
    let result = std::panic::catch_unwind(|| -> Result<(), String> {
        if input.is_null() || length > MAX_INPUT || outputs.is_null() || count != 4 {
            return Err("invalid term input/output descriptors".into());
        }
        // SAFETY: caller provides four live descriptors and the admitted input.
        let descriptors = unsafe { std::slice::from_raw_parts(outputs, count) };
        if descriptors.iter().any(|b| {
            b.data.is_null()
                || !(b.data as usize).is_multiple_of(std::mem::align_of::<f64>())
                || b.len > 128 * 1024 * 1024
                || b.shape != [b.len, 1, 1]
        }) {
            return Err("invalid term numeric output storage".into());
        }
        let body = unsafe { std::slice::from_raw_parts(input, length) };
        let Output::Risk(result) = execute(body)? else {
            return Err("numeric outputs require term-risk-1".into());
        };
        let values = [result.oas, result.price, result.dv01, result.sensitivities];
        if descriptors
            .iter()
            .zip(&values)
            .any(|(b, v)| b.len != v.len())
        {
            return Err("term numeric output shape mismatch".into());
        }
        // Validate the whole result before publishing any caller-owned output.
        for (buffer, values) in descriptors.iter().zip(&values) {
            // SAFETY: validated length; caller guarantees disjoint owned storage.
            unsafe { std::ptr::copy_nonoverlapping(values.as_ptr(), buffer.data, values.len()) };
        }
        Ok(())
    })
    .unwrap_or_else(|_| Err("native term request failed validation".into()));
    let bytes = match result {
        Ok(()) => b"{\"ok\":true}".to_vec(),
        Err(error) => serde_json::to_vec(&Response::Error { ok: false, error }).unwrap(),
    };
    CString::new(bytes).unwrap().into_raw()
}
