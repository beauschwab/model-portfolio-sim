use super::commitment::{commitment, matched, undrawn};
use super::*;
pub fn evaluate(r: Request) -> Result<Value, String> {
    let (joined, charge_work) = admit(&r)?;
    let native = source::evaluate(&r.source)?;
    let phase_bound = native
        .sources
        .iter()
        .try_fold(0usize, |a, x| add(a, x.work_bound))?;
    let (acquisition_phases, phase_work) = acquisition_phase::derive(&r, &native)?;
    if phase_work > phase_bound {
        return Err("phase work bound".into());
    }
    let mut positions = vec![];
    let commitments = native
        .paths
        .iter()
        .map(|p| Ok((binding(p), commitment(p, &native)?)))
        .collect::<Result<BTreeMap<_, _>, String>>()?;
    let mut lineage = vec![];
    let paths = native
        .paths
        .iter()
        .map(|p| (binding(p), p))
        .collect::<BTreeMap<_, _>>();
    for p in &native.paths {
        portfolio_compute_control::checkpoint()?;
        let key = binding(p);
        let policy = r
            .charges
            .iter()
            .find(|c| c.binding == key)
            .ok_or("charge coverage")?;
        let benchmark = paths[&policy.option.benchmark];
        let contingent = paths[&policy.contingent.benchmark];
        matched(p, benchmark)?;
        matched(p, contingent)?;
        for (i, d) in p.preparation.days.iter().enumerate() {
            portfolio_compute_control::checkpoint()?;
            if !d.scaled_average_funding_balance.is_finite()
                || d.scaled_average_funding_balance < 0.0
            {
                return Err("signed funding is incompatible with existing nonnegative Treasury positions; explicit reviewed channel policy required".into());
            }
            let (inc, option) = charge_math::positive_incremental_cost(
                d.scaled_average_funding_balance,
                benchmark.preparation.days[i].scaled_average_funding_balance,
                policy.option.annual_rate,
                d.accrual_fraction,
            )?;
            let (und, contingent_amount) = charge_math::positive_incremental_cost(
                undrawn(p, &commitments[&key], i)?,
                undrawn(contingent, &commitments[&policy.contingent.benchmark], i)?,
                policy.contingent.annual_rate,
                d.accrual_fraction,
            )?;
            let basis = d.scaled_average_funding_balance * d.accrual_fraction;
            let spread = charge_math::option_spread(option, basis)?;
            positions.push(treasury::Position {
                id: format!(
                    "{}:{}:{}:{}:{}",
                    key.source_id, key.owner_index, key.run_index, key.path_index, d.ordinal
                ),
                loan_id: Some(p.position_id.clone()),
                cohort_id: None,
                entity: p.preparation.raw_request.entity.clone(),
                currency: p.preparation.raw_request.currency_unit.clone(),
                scenario: p.preparation.raw_request.scenario_id.clone(),
                period: {
                    let (y, m, day) = portfolio_model_core::cashflow::ymd_from_ordinal(d.ordinal)?;
                    format!("{y:04}-{m:02}-{day:02}")
                },
                curve_id: policy.curve_id.clone(),
                side: p.preparation.raw_request.side.clone(),
                average_balance: d.scaled_average_funding_balance,
                accrual_fraction: d.accrual_fraction,
                repricing_years: d.reference_years,
                funding_years: d.liquidity_years,
                external_interest: d.original_eir,
                fees: 0.0,
                operating_cost: policy.daily_operating_cost * p.preparation.scale,
                expected_loss: 0.0,
                allocated_capital: policy.allocated_capital * p.preparation.scale,
                cost_of_capital: policy.annual_cost_of_capital,
                option_spread: spread,
                contingent_liquidity_charge: contingent_amount,
            });
            lineage.push(DerivedCharge {
                binding: key.clone(),
                ordinal: d.ordinal,
                benchmark: policy.option.benchmark.clone(),
                contingent_benchmark: policy.contingent.benchmark.clone(),
                actual_cash_difference: d.bank_cash - benchmark.preparation.days[i].bank_cash,
                incremental_funding: inc,
                incremental_undrawn: und,
                option_amount: option,
                contingent_amount,
                retained_earnings: d.earnings_by_channel.clone(),
                native_opening_undrawn: commitments[&key][i]["undrawn"].as_f64().unwrap(),
                native_closing_undrawn: commitments[&key][i + 1]["undrawn"].as_f64().unwrap(),
                benchmark_opening_undrawn: commitments[&policy.contingent.benchmark][i]["undrawn"]
                    .as_f64()
                    .unwrap(),
                benchmark_closing_undrawn: commitments[&policy.contingent.benchmark][i + 1]
                    ["undrawn"]
                    .as_f64()
                    .unwrap(),
            });
        }
    }
    let treasury_request = treasury::Request {
        as_of: r.as_of.clone(),
        policy_id: r.policy_review.id.clone(),
        capital: vec![],
        curves: r.curves,
        positions,
        strategy_units: vec![],
        capital_deltas: vec![],
    };
    let treasury_result = treasury_request.run()?;
    let mut accounting_reconciliation = vec![];
    for phase in &acquisition_phases {
        let opening = phase["complete_opening_earnings"]
            .as_array()
            .unwrap()
            .iter()
            .map(|x| x["amount"].as_f64().unwrap())
            .sum::<f64>();
        accounting_reconciliation.push(json!({"binding":phase["binding"],"ordinal":phase["ordinal"],"phase":"opening_acquisition-1","source_earnings":opening,"operating_cost":0.0,"full_external_earnings":opening,"business_profit":opening,"treasury_profit":0.0,"consolidated_earnings":opening,"capital_cost":0.0}));
    }
    for (i, charge) in lineage.iter().enumerate() {
        let source_earnings = charge
            .retained_earnings
            .iter()
            .map(|x| x.amount)
            .sum::<f64>();
        let recurring = charge
            .retained_earnings
            .iter()
            .find(|x| x.channel == "original_eir")
            .unwrap()
            .amount;
        let t = &treasury_result["ftp_positions"][i];
        let p = &treasury_request.positions[i];
        let full = source_earnings - p.operating_cost;
        let business = t["business_profit"].as_f64().unwrap() + (source_earnings - recurring);
        let treasury = t["ftp_charge"].as_f64().unwrap();
        if (business + treasury - full).abs() > 1e-9 + 1e-12 * full.abs() {
            return Err("complete source earnings FTP elimination".into());
        }
        accounting_reconciliation.push(json!({"binding":charge.binding,"ordinal":charge.ordinal,"phase":"earned_day-1","source_earnings":source_earnings,"operating_cost":p.operating_cost,"full_external_earnings":full,"business_profit":business,"treasury_profit":treasury,"consolidated_earnings":business+treasury,"capital_cost":t["capital_cost"]}));
    }
    let capital = match r.capital {
        Some(c) => Some(mortgage_closing_capital::run(c)?),
        None => None,
    };
    portfolio_compute_control::checkpoint()?;
    let retained_capacity = add(
        add(native.retained_capacity_bound, 64 * 1024 * 1024)?,
        add(
            source::charge_states(&r.source)?
                .checked_mul(4096)
                .ok_or("charge capacity overflow")?,
            if capital.is_some() {
                64 * 1024 * 1024
            } else {
                0
            },
        )?,
    )?;
    Ok(
        json!({"schema":SCHEMA,"source_preparation":native,"selected_acquisition_phases":acquisition_phases,"source_commitment_receipts":commitments.iter().map(|(binding,points)|json!({"binding":binding,"points":points})).collect::<Vec<_>>(),"phase_work_bound":phase_bound,"phase_work":phase_work,"retained_capacity_bound":retained_capacity,"treasury_request":treasury_request,"treasury_result":treasury_result,"accounting_scope_reconciliation":accounting_reconciliation,"charges":lineage,"closing_capital":capital,"joined_work_bound":joined,"charge_work_bound":charge_work,"source_ftp_adoption":false,"independent_persisted_verification":false,"regulatory_approval":false,"empirical_validation":false,"interim_capital_eligible":false,"capital_ftp_eligible":false,"pricing_measure_option_valuation":false,"charge_methodology":"reviewed_physical_scenario_incremental_cost-1"}),
    )
}
