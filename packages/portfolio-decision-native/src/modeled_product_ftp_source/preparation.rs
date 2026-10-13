use super::*;

fn funding(s: &ftp::State, p: &ftp::Policy) -> Result<f64, String> {
    if p.channels.len() != 18 {
        return fail("complete ordered funding channel policy required");
    }
    Ok(s.channels
        .iter()
        .zip(&p.channels)
        .map(|(a, p)| a.amount * p.coefficient as f64)
        .sum())
}

pub(super) fn prepare_path_in_tier(
    mut d: PathData,
    v: &Review,
    portfolio: bool,
) -> Result<(ftp::ResultData, Vec<ftp::Amount>, f64), String> {
    if d.currency != v.currency {
        return fail("review currency must match native source");
    }
    if d.states.len() != usize::try_from(d.end - d.start + 1).map_err(|_| "reversed source")?
        || d.end - d.start
            > if portfolio {
                ftp::PORTFOLIO_MAX_DAYS as i32
            } else {
                64
            }
    {
        return fail("complete actual daily source <=64 days required");
    }
    let mut changes = Vec::new();
    for (seq, pair) in d.states.windows(2).enumerate() {
        portfolio_compute_control::checkpoint()?;
        changes.push(ftp::Change {
            sequence: (seq + 1) as u32,
            ordinal: pair[1].ordinal,
            source_event_id: pair[1].source_checkpoint_id.clone(),
            kind: "reviewed_other_reallocation".into(),
            amount: funding(&pair[1], &v.policy)? - funding(&pair[0], &v.policy)?,
            cash_event_id: None,
            review: v.channel_change_review.clone(),
        });
    }
    let opening_source_cash = d.cash.iter().filter(|x| x.0 == d.start).map(|x| x.2).sum();
    let cash = d
        .cash
        .drain(..)
        .filter(|x| x.0 > d.start)
        .map(|(ordinal, event_id, bank_cash)| ftp::Cash {
            event_id,
            ordinal,
            kind: "source_atomic_cash".into(),
            bank_cash,
            funding_change_sequence: None,
            exclusion_review: Some(v.cash_exclusion_review.clone()),
        })
        .collect();
    let mut opening = ftp::INCOME_LANES
        .iter()
        .map(|s| ftp::Amount {
            channel: (*s).into(),
            amount: 0.,
        })
        .collect::<Vec<_>>();
    let mut earned = Vec::new();
    let mut used = BTreeSet::new();
    for (ordinal, event_id, source_lane, bank_earnings) in d.earned {
        let lane = if source_lane == "original_eir" {
            "original_eir".to_string()
        } else {
            used.insert(source_lane.clone());
            v.other_income
                .get(&source_lane)
                .ok_or(format!("reviewed P&L mapping missing: {source_lane}"))?
                .clone()
        };
        if ordinal == d.start {
            opening
                .iter_mut()
                .find(|a| a.channel == lane)
                .ok_or("income lane")?
                .amount += bank_earnings;
        } else {
            earned.push(ftp::Earned {
                event_id,
                ordinal,
                channel: lane,
                bank_earnings,
                review: v.income_review.clone(),
            });
        }
    }
    if v.other_income.keys().any(|k| !used.contains(k)) {
        return fail("unused P&L mapping is ambiguous");
    }
    let r = ftp::Request {
        schema: if portfolio {
            ftp::PORTFOLIO_SCHEMA
        } else {
            ftp::SCHEMA
        }
        .into(),
        measure: "physical".into(),
        source: d.source,
        original_yield_anchor: d.anchor,
        path_id: d.path,
        entity: v.entity.clone(),
        currency_unit: d.currency,
        scenario_id: d.scenario,
        side: d.side,
        start_ordinal: d.start,
        full_end_ordinal: d.end,
        source_weight: d.weight,
        unit_face: d.face,
        allocated_face: d.allocated,
        opening_funding_balance: funding(&d.states[0], &v.policy)?,
        policy: v.policy.clone(),
        states: d.states,
        cash,
        changes,
        earned,
        resets: v.resets.clone(),
    };
    let prepared = if portfolio {
        ftp::prepare_portfolio_with_check(&r, &mut || portfolio_compute_control::checkpoint())?
    } else {
        ftp::prepare_with_check(&r, &mut || portfolio_compute_control::checkpoint())?
    };
    Ok((prepared, opening, opening_source_cash))
}
