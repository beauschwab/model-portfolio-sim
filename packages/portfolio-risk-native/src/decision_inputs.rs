//! Dirty-row extraction and assumption application for the owned decision actor.
use crate::accounting_lifecycle::{AccountingRequest, DepositBook, MortgageBook, TermBook};
use crate::incremental::PriceRequest;
use crate::mortgage_input::OwnedMortgage;
use crate::term_deck::DeckRequest;
use serde_json::Value;
use std::collections::BTreeMap;

pub type Selection = BTreeMap<String, Vec<usize>>;

pub fn accounting(source: &AccountingRequest, selected: &Selection) -> AccountingRequest {
    let mbs = source.mbs.as_ref().and_then(|b| {
        selected.get("mbs").map(|ix| {
            let r = &b.request;
            MortgageBook {
                ids: ix.iter().map(|&i| b.ids[i].clone()).collect(),
                book_yields: b
                    .book_yields
                    .as_ref()
                    .map(|v| ix.iter().map(|&i| v[i]).collect()),
                request: OwnedMortgage {
                    tenors: r.tenors.clone(),
                    swap_rates: r.swap_rates.clone(),
                    vol_quotes: r.vol_quotes.clone(),
                    cc_history: r.cc_history.clone(),
                    ps_history: r.ps_history.clone(),
                    book: ix
                        .iter()
                        .flat_map(|&i| r.book[i * 13..(i + 1) * 13].iter().copied())
                        .collect(),
                    original_hpi: if r.original_hpi.is_empty() {
                        vec![]
                    } else {
                        ix.iter().map(|&i| r.original_hpi[i]).collect()
                    },
                    seed: r.seed.clone(),
                    fixed_oas: vec![],
                    config: r.config.clone(),
                    prepay: r.prepay.clone(),
                },
            }
        })
    });
    let terms = source
        .terms
        .iter()
        .filter_map(|b| {
            selected.get(&b.key).map(|ix| TermBook {
                key: b.key.clone(),
                ids: ix.iter().map(|&i| b.ids[i].clone()).collect(),
                book_yields: b
                    .book_yields
                    .as_ref()
                    .map(|v| ix.iter().map(|&i| v[i]).collect()),
                deck: DeckRequest {
                    product: b.deck.product,
                    asof: b.deck.asof,
                    months: b.deck.months,
                    calendar: b.deck.calendar.clone(),
                    bdc: b.deck.bdc,
                    contracts: ix.iter().map(|&i| b.deck.contracts[i].clone()).collect(),
                },
            })
        })
        .collect();
    let deposits = source.deposits.as_ref().and_then(|b| {
        selected.get("deposits").map(|ix| DepositBook {
            ids: ix.iter().map(|&i| b.ids[i].clone()).collect(),
            book: ix.iter().map(|&i| b.book[i].clone()).collect(),
            assumptions: b.assumptions.clone(),
            history: b.history.clone(),
        })
    });
    AccountingRequest {
        market: source.market.clone(),
        asof: source.asof,
        horizon: source.horizon,
        mbs,
        terms,
        deposits,
        mm: None,
        hedges: None,
        withdrawal_parameters: source.withdrawal_parameters.clone(),
        anchors: BTreeMap::new(),
        deposit_initial_rate: None,
        forecast: None,
        draws: None,
        capture_anchor: false,
        capture_cashflows: false,
    }
}

/// Indices are taken from the actor's validated immutable ID index.
pub fn select(source: &PriceRequest, selected: &Selection) -> Result<PriceRequest, String> {
    let k = source.kpis.as_ref().ok_or("decision requires graph KPIs")?;
    let mut kpis = crate::kpi_lifecycle::KpiRequest {
        mode: k.mode.clone(),
        books: BTreeMap::new(),
        equity: None,
        asof: k.asof,
        weights: k.weights.clone(),
        nii: None,
        stress_aoci: vec![],
        shocks: k.shocks.clone(),
        dv01s: BTreeMap::new(),
        risk: None,
    };
    for (book, ix) in selected {
        let rows = k.books.get(book).ok_or("unknown selected book")?;
        if ix.iter().any(|&i| i >= rows.len()) {
            return Err("selected row out of bounds".into());
        }
        kpis.books
            .insert(book.clone(), ix.iter().map(|&i| rows[i].clone()).collect());
    }
    Ok(PriceRequest {
        base: accounting(&source.base, selected),
        current: accounting(&source.base, selected),
        spread_shift: 0.,
        overrides: BTreeMap::new(),
        include_analytics: true,
        include_key_rates: false,
        kpis: Some(kpis),
        auxiliary: None,
    })
}

pub fn validate_patch(book: &str, patch: &Value) -> Result<(), String> {
    for (field, v) in patch
        .as_object()
        .ok_or("assumption patch must be an object")?
    {
        let (lo, hi) = crate::whatif::domain(book, field).ok_or("unsupported assumption")?;
        if v.is_null() {
            continue;
        }
        let x = v.as_f64().ok_or("assumption must be numeric or null")?;
        if !x.is_finite() || x < lo || x > hi || field == "wam" && x.fract() != 0. {
            return Err(format!("invalid {book} assumption {field}"));
        }
    }
    Ok(())
}

pub fn apply(
    request: &mut AccountingRequest,
    patches: &BTreeMap<String, Value>,
) -> Result<(), String> {
    if let Some(b) = request.mbs.as_mut() {
        let fields = [
            "wac",
            "net_coupon",
            "wam",
            "age",
            "oltv",
            "factor",
            "fico",
            "avg_loan_size",
        ];
        let derived_hpi = b.request.original_hpi.is_empty();
        if derived_hpi
            && b.ids.iter().any(|id| {
                patches
                    .get(&format!("mbs:{id}"))
                    .is_some_and(|p| p.get("hpi_orig_ratio").is_some())
            })
        {
            b.request.original_hpi = b
                .request
                .book
                .chunks_exact(13)
                .map(|r| (1. + b.request.config.hpi[0]).powf(r[3] / 12.))
                .collect();
        }
        for (i, id) in b.ids.iter().enumerate() {
            if let Some(p) = patches.get(&format!("mbs:{id}")) {
                validate_patch("mbs", p)?;
                for (j, field) in fields.iter().enumerate() {
                    if let Some(x) = p[*field].as_f64() {
                        b.request.book[i * 13 + j] = x;
                    }
                }
                if let Some(x) = p["hpi_orig_ratio"].as_f64() {
                    b.request.original_hpi[i] = x;
                } else if derived_hpi && !b.request.original_hpi.is_empty() {
                    b.request.original_hpi[i] =
                        (1. + b.request.config.hpi[0]).powf(b.request.book[i * 13 + 3] / 12.);
                }
            }
        }
    }
    for b in &mut request.terms {
        for (id, row) in b.ids.iter().zip(&mut b.deck.contracts) {
            if let Some(p) = patches.get(&format!("{}:{id}", b.key)) {
                validate_patch(&b.key, p)?;
                for (field, target) in [
                    (
                        if b.key == "cds" {
                            "rate"
                        } else {
                            "coupon_or_spread"
                        },
                        &mut row.coupon,
                    ),
                    ("cap", &mut row.cap),
                    ("floor", &mut row.floor),
                    ("call_threshold", &mut row.call_threshold),
                    ("penalty_months", &mut row.penalty_months),
                    ("ew_mult", &mut row.ew_mult),
                ] {
                    if let Some(x) = p[field].as_f64() {
                        *target = x;
                    }
                }
                if row.floor > row.cap {
                    return Err("coupon floor cannot exceed cap".into());
                }
            }
        }
    }
    if let Some(b) = request.deposits.as_mut() {
        for (id, row) in b.ids.iter().zip(&mut b.book) {
            if let Some(p) = patches.get(&format!("deposits:{id}")) {
                validate_patch("deposits", p)?;
                for (field, target) in [
                    ("rate_paid", &mut row.rate_paid),
                    ("age_months", &mut row.age_months),
                    ("avg_account_size", &mut row.avg_account_size),
                    ("svc_cost", &mut row.svc_cost),
                ] {
                    if let Some(x) = p[field].as_f64() {
                        *target = x;
                    }
                }
                for (field, target) in [
                    ("attrition_base", &mut row.attrition_base),
                    ("attrition_amp", &mut row.attrition_amp),
                    ("attrition_slope", &mut row.attrition_slope),
                    ("attrition_gap", &mut row.attrition_gap),
                ] {
                    if let Some(x) = p[field].as_f64() {
                        *target = Some(x);
                    }
                }
            }
        }
    }
    Ok(())
}
