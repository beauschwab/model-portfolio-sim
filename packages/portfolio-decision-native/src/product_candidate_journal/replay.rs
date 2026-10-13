use super::*;
use portfolio_ledger_native::types::Spec;
use portfolio_risk_native::product_candidate_source::Path;
use std::collections::BTreeMap;
type Balances = BTreeMap<(String, String, String, String), f64>;
fn money(x: f64) -> Result<f64, String> {
    if x.is_finite() && x.abs() <= product_strategy::MAX_ALLOCATION_AMOUNT {
        Ok(x)
    } else {
        Err("candidate journal finite monetary amount required".into())
    }
}
fn near(a: f64, b: f64) -> Result<(), String> {
    if !a.is_finite() || !b.is_finite() || (a - b).abs() > 1e-9 + 1e-12 * b.abs() {
        Err("candidate journal source/closing reconciliation failed".into())
    } else {
        Ok(())
    }
}
fn post(gl: &str, value: f64) -> Posting {
    Posting {
        gl_account: gl.into(),
        value,
    }
}
fn principal(side: f64) -> &'static str {
    if side > 0. {
        "asset_principal"
    } else {
        "funding_principal"
    }
}
fn basis(side: f64) -> &'static str {
    if side > 0. {
        "asset_book_adjustment"
    } else {
        "funding_book_adjustment"
    }
}
fn right(side: f64, accrued: f64) -> &'static str {
    if side > 0. {
        if accrued >= 0. {
            "dated_interest_receivable"
        } else {
            "dated_deferred_income"
        }
    } else if accrued >= 0. {
        "dated_interest_payable"
    } else {
        "dated_prepaid_expense"
    }
}
fn rights(side: f64, coupon_sign: f64, old: f64, new: f64) -> Vec<Posting> {
    vec![
        post(right(side * coupon_sign, coupon_sign * old), -side * old),
        post(right(side * coupon_sign, coupon_sign * new), side * new),
    ]
}
fn balance(b: &Balances, account: &str, position: &str, path: &str, gl: &str) -> f64 {
    b.get(&(account.into(), position.into(), path.into(), gl.into()))
        .copied()
        .unwrap_or(0.)
}
struct Event<'a> {
    path: &'a Path,
    owner: &'a Owner,
    mapping: &'a RunMapping,
    ordinal: i32,
    phase: &'static str,
    native_event_ids: Vec<String>,
    postings: Vec<Posting>,
}
fn append(
    r: &Request,
    b: &mut Balances,
    journals: &mut Vec<Journal>,
    e: Event<'_>,
) -> Result<(), String> {
    let mut net = 0.;
    let mut magnitude = 0.;
    for p in &e.postings {
        money(p.value)?;
        net += p.value;
        magnitude += p.value.abs();
    }
    near(net, 0.).or_else(|_| {
        if net.abs() <= 1e-9 + 1e-12 * magnitude {
            Ok::<(), String>(())
        } else {
            Err("candidate event journal is unbalanced".into())
        }
    })?;
    for p in &e.postings {
        let pooled = matches!(
            p.gl_account.as_str(),
            "cash" | "equity" | "pnl:effective_interest"
        );
        let key = (
            e.owner.account_id.clone(),
            if pooled {
                String::new()
            } else {
                e.owner.position_id.clone()
            },
            if pooled {
                String::new()
            } else {
                e.path.path_id.clone()
            },
            p.gl_account.clone(),
        );
        let old = b.entry(key).or_insert(0.);
        *old = money(*old + p.value)?;
    }
    journals.push(Journal {
        transaction_id: journals.len() + 1,
        source_scenario_id: e.path.scenario_id.clone(),
        ledger_scenario_id: e.mapping.ledger_scenario_id.clone(),
        severity: e.mapping.severity,
        ordinal: e.ordinal,
        phase: e.phase.into(),
        unit_id: e.path.unit_id.clone(),
        position_id: e.owner.position_id.clone(),
        account_id: e.owner.account_id.clone(),
        source: e.owner.source.clone(),
        path_id: e.path.path_id.clone(),
        source_weight: e.path.source_weight,
        allocation_factor: e.path.allocation_factor,
        native_source_index: e.path.native_source_index,
        native_event_ids: e.native_event_ids,
        policy: r.policy.identity.clone(),
        postings: e.postings,
    });
    Ok(())
}
pub(super) fn run(
    r: &Request,
    spec: &Spec,
    prepared: Prepared,
    capture: Capture,
    work: usize,
    full: i32,
) -> Result<ResultData, String> {
    let mut journals = Vec::new();
    let mut daily_positions = Vec::new();
    let mut daily_accounts = Vec::new();
    let mut trial = Vec::new();
    let mut reported_positions = Vec::new();
    let mut closing_positions = Vec::new();
    let mut reported_accounts = Vec::new();
    let mut closing_accounts = Vec::new();
    // Admit every complete physical path before emitting any journal. The sign
    // of original effective income never determines the contractual right.
    let signs: Vec<_> = capture
        .paths
        .iter()
        .map(|p| {
            let sign =
                portfolio_risk_native::product_candidate_source::contractual_interest_sign(p)?;
            if sign != p.interest_direction {
                return Err("candidate immutable contractual direction mismatch".into());
            }
            Ok(sign)
        })
        .collect::<Result<Vec<f64>, String>>()?;
    for scenario in &prepared.source().scenario_ids {
        portfolio_compute_control::checkpoint()?;
        let mapping = r
            .run_mapping
            .iter()
            .find(|m| m.source_scenario_id == *scenario)
            .ok_or("candidate run mapping missing")?;
        let mut b = Balances::new();
        for a in &spec.accounts {
            b.insert(
                (a.id.clone(), String::new(), String::new(), "cash".into()),
                a.cash,
            );
            b.insert(
                (a.id.clone(), String::new(), String::new(), "equity".into()),
                -a.equity,
            );
        }
        let paths: Vec<_> = capture
            .paths
            .iter()
            .filter(|p| p.scenario_id == *scenario)
            .collect();
        let coupon_signs: Vec<_> = capture
            .paths
            .iter()
            .zip(&signs)
            .filter(|(p, _)| p.scenario_id == *scenario)
            .map(|(_, sign)| *sign)
            .collect();
        let mut states: Vec<_> = paths
            .iter()
            .map(|p| {
                let o = &r.owners[p.unit_index];
                PositionState {
                    source_scenario_id: scenario.clone(),
                    ordinal: r.as_of_ordinal,
                    unit_id: p.unit_id.clone(),
                    position_id: o.position_id.clone(),
                    account_id: o.account_id.clone(),
                    path_id: p.path_id.clone(),
                    source_weight: p.source_weight,
                    allocated_principal: 0.,
                    accrued_interest: 0.,
                    book_adjustment: 0.,
                    gross_carrying_value: 0.,
                    effective_interest_to_date: 0.,
                    acquired: false,
                    full_end_ordinal: p.full_end_ordinal,
                }
            })
            .collect();
        for ordinal in r.as_of_ordinal..=full {
            portfolio_compute_control::checkpoint()?;
            for (pi, &path) in paths.iter().enumerate() {
                let owner = &r.owners[path.unit_index];
                let state = &mut states[pi];
                let factor = money(path.allocation_factor * path.source_weight)?;
                if ordinal == path.purchase_ordinal {
                    let face = money(path.unit_face * factor)?;
                    let cost =
                        money(path.daily_income_request.opening_gross_carrying_value * factor)?;
                    let adj = money(cost - face)?;
                    append(
                        r,
                        &mut b,
                        &mut journals,
                        Event {
                            path,
                            owner,
                            mapping,
                            ordinal,
                            phase: "acquisition",
                            native_event_ids: vec![format!("purchase:{}", path.unit_id)],
                            postings: vec![
                                post(principal(path.side), path.side * face),
                                post(basis(path.side), path.side * adj),
                                post("cash", -path.side * cost),
                            ],
                        },
                    )?;
                    state.allocated_principal = face;
                    state.book_adjustment = adj;
                    state.gross_carrying_value = cost;
                    state.acquired = true;
                } else if ordinal > path.purchase_ordinal && ordinal <= path.full_end_ordinal {
                    let row =
                        &path.daily_income.rows[(ordinal - path.purchase_ordinal - 1) as usize];
                    let earned = money(row.contractual_interest_earned * factor)?;
                    let effective = money(row.effective_interest * factor)?;
                    let amort = money(row.basis_amortization * factor)?;
                    let new_accrued = money(state.accrued_interest + earned)?;
                    let mut postings = rights(
                        path.side,
                        coupon_signs[pi],
                        state.accrued_interest,
                        new_accrued,
                    );
                    postings.push(post(basis(path.side), path.side * amort));
                    postings.push(post("pnl:effective_interest", -path.side * effective));
                    let mut ids: Vec<_> = path
                        .recognition
                        .iter()
                        .filter(|s| s.start_ordinal < ordinal && s.end_ordinal >= ordinal)
                        .map(|s| s.origin_id.clone())
                        .collect();
                    ids.push(row.event_id.clone());
                    append(
                        r,
                        &mut b,
                        &mut journals,
                        Event {
                            path,
                            owner,
                            mapping,
                            ordinal,
                            phase: "recognition",
                            native_event_ids: ids,
                            postings,
                        },
                    )?;
                    state.accrued_interest = new_accrued;
                    state.book_adjustment = money(state.book_adjustment + amort)?;
                    state.effective_interest_to_date =
                        money(state.effective_interest_to_date + effective)?;
                    let cash: Vec<_> = path.cash.iter().filter(|c| c.ordinal == ordinal).collect();
                    if !cash.is_empty() {
                        let mut pc = 0.;
                        let mut ic = 0.;
                        for c in &cash {
                            pc = money(pc + c.principal * factor)?;
                            ic = money(ic + c.interest * factor)?;
                        }
                        let new_accrued = money(state.accrued_interest - ic)?;
                        let mut postings = vec![
                            post("cash", path.side * money(pc + ic)?),
                            post(principal(path.side), -path.side * pc),
                        ];
                        postings.extend(rights(
                            path.side,
                            coupon_signs[pi],
                            state.accrued_interest,
                            new_accrued,
                        ));
                        append(
                            r,
                            &mut b,
                            &mut journals,
                            Event {
                                path,
                                owner,
                                mapping,
                                ordinal,
                                phase: "settlement",
                                native_event_ids: cash.iter().map(|c| c.event_id.clone()).collect(),
                                postings,
                            },
                        )?;
                        state.allocated_principal = money(state.allocated_principal - pc)?;
                        state.accrued_interest = new_accrued;
                    }
                    state.gross_carrying_value = money(
                        state.allocated_principal + state.accrued_interest + state.book_adjustment,
                    )?;
                    near(state.allocated_principal, row.closing_principal * factor)?;
                    near(
                        state.accrued_interest,
                        row.closing_accrued_interest * factor,
                    )?;
                    near(state.book_adjustment, row.closing_book_adjustment * factor)?;
                    near(
                        state.gross_carrying_value,
                        row.closing_gross_carrying_value * factor,
                    )?;
                }
                if ordinal <= path.full_end_ordinal {
                    state.ordinal = ordinal;
                    near(
                        balance(
                            &b,
                            &owner.account_id,
                            &owner.position_id,
                            &path.path_id,
                            principal(path.side),
                        ),
                        path.side * state.allocated_principal,
                    )?;
                    near(
                        balance(
                            &b,
                            &owner.account_id,
                            &owner.position_id,
                            &path.path_id,
                            basis(path.side),
                        ),
                        path.side * state.book_adjustment,
                    )?;
                    let rights_total = [
                        "dated_interest_receivable",
                        "dated_interest_payable",
                        "dated_prepaid_expense",
                        "dated_deferred_income",
                    ]
                    .iter()
                    .map(|gl| balance(&b, &owner.account_id, &owner.position_id, &path.path_id, gl))
                    .sum();
                    near(rights_total, path.side * state.accrued_interest)?;
                    daily_positions.push(state.clone());
                    if ordinal == r.reporting_end_ordinal {
                        reported_positions.push(state.clone());
                    }
                }
            }
            for a in &spec.accounts {
                let cash = balance(&b, &a.id, "", "", "cash");
                let income = -balance(&b, &a.id, "", "", "pnl:effective_interest");
                if cash < a.cash_floor - 1e-9 {
                    return Err("candidate end-of-day account cash floor breached; no replacement funding inferred".into());
                }
                let state = AccountState {
                    source_scenario_id: scenario.clone(),
                    ordinal,
                    account_id: a.id.clone(),
                    cash,
                    cumulative_net_effective_income: income,
                    equity: money(a.equity + income)?,
                };
                let total: f64 = b
                    .iter()
                    .filter(|((account, _, _, _), _)| *account == a.id)
                    .map(|(_, v)| v)
                    .sum();
                near(total, 0.)?;
                daily_accounts.push(state.clone());
                if ordinal == r.reporting_end_ordinal {
                    reported_accounts.push(state.clone());
                }
                if ordinal == full {
                    closing_accounts.push(state);
                }
            }
        }
        for mut state in states {
            state.ordinal = full;
            near(state.allocated_principal, 0.)?;
            near(state.accrued_interest, 0.)?;
            near(state.gross_carrying_value, 0.)?;
            closing_positions.push(state);
        }
        for ((account_id, position_id, path_id, gl_account), balance) in b {
            trial.push(TrialBalance {
                source_scenario_id: scenario.clone(),
                account_id,
                position_id,
                path_id,
                gl_account,
                balance,
            });
        }
    }
    // Source funding is replayed independently against account cash net movement
    // for every retained date, rather than using quoted tensor cash as truth.
    let selected = product_strategy::evaluate(&prepared, &r.allocation)?;
    for source in &selected.scenarios {
        let opening: f64 = spec.accounts.iter().map(|a| a.cash).sum();
        for c in &source.cash {
            let actual: f64 = daily_accounts
                .iter()
                .filter(|a| a.source_scenario_id == source.scenario_id && a.ordinal == c.ordinal)
                .map(|a| a.cash)
                .sum();
            near(actual - opening, c.cumulative_bank_cash)?;
        }
        let income: f64 = reported_accounts
            .iter()
            .filter(|a| a.source_scenario_id == source.scenario_id)
            .map(|a| a.cumulative_net_effective_income)
            .sum();
        near(income, source.incremental_external_nii)?;
    }
    Ok(ResultData {schema:SCHEMA,raw_request:r.clone(),normalized_specification:serde_json::to_value(spec).map_err(|e|e.to_string())?,native_preparation:prepared,source_capture:capture,
        journals,daily_positions,daily_accounts,reported_positions,complete_closing_positions:closing_positions,reported_accounts,complete_closing_accounts:closing_accounts,
        trial_balance:trial,full_end_ordinal:full,admitted_work:work,independent_persisted_verification:false,dynamic_mixed_acceptance:false,
        scope:"standalone acquisition-aware modeled physical expected candidate journal; native raw opening reconciliation and complete original-yield/cash tails; supplied opening identity is provenance only; no independent persisted verification, observed settlement, mixed existing-source composition, regulatory or empirical acceptance"})
}

#[cfg(test)]
mod contractual_right_tests {
    use super::rights;
    #[test]
    fn negative_coupon_right_and_early_cash_keep_signed_amounts() {
        for (side, sign, accrued, gl, amount) in [
            (-1., -1., -2., "dated_interest_receivable", 2.),
            (-1., -1., 2., "dated_deferred_income", -2.),
            (-1., 1., 2., "dated_interest_payable", -2.),
            (-1., 1., -2., "dated_prepaid_expense", 2.),
            (1., -1., -2., "dated_interest_payable", -2.),
            (1., -1., 2., "dated_prepaid_expense", 2.),
        ] {
            let p = rights(side, sign, 0., accrued);
            assert_eq!(p[1].gl_account, gl);
            assert_eq!(p[1].value, amount);
        }
        // Signed settlement first creates deferred income; later recognition
        // clears that liability without any receivable/prepaid substitution.
        let early = rights(-1., -1., 0., 2.);
        let earned = rights(-1., -1., 2., 0.);
        assert_eq!(early[1].gl_account, earned[0].gl_account);
        assert_eq!(early[1].value + earned[0].value, 0.);
    }
}
