use super::*;
fn paired_terms(r: &MortgageRequest) -> Result<serde_json::Value, String> {
    let mut v = serde_json::to_value(r).map_err(|e| e.to_string())?;
    let object = v.as_object_mut().ok_or("paired native raw object")?;
    for field in [
        "path",
        "borrower_collection_fraction",
        "loss_given_default",
        "complete_recovery_tails",
    ] {
        object.remove(field);
    }
    let identity = v["identity"]
        .as_object_mut()
        .ok_or("paired native identity object")?;
    for field in ["run_id", "assumption_id"] {
        identity.remove(field);
    }
    if let Some(resets) = v["rate"]["resets"].as_array_mut() {
        // Calendar, reference-index terms and contract caps are immutable. The
        // complete actual supplied fixing/projection is retained in each raw.
        for reset in resets {
            reset
                .as_object_mut()
                .ok_or("paired reset object")?
                .remove("input");
        }
    }
    Ok(v)
}
fn encoded_len(
    r: &impl Serialize,
    check: &mut dyn FnMut() -> Result<(), String>,
) -> Result<usize, String> {
    struct Count<'a> {
        n: usize,
        check: &'a mut dyn FnMut() -> Result<(), String>,
    }
    impl Write for Count<'_> {
        fn write(&mut self, b: &[u8]) -> std::io::Result<usize> {
            (self.check)().map_err(std::io::Error::other)?;
            self.n = self
                .n
                .checked_add(b.len())
                .filter(|n| *n <= 1024 * 1024)
                .ok_or_else(|| std::io::Error::other("sequential raw byte admission exceeded"))?;
            Ok(b.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }
    let mut count = Count { n: 0, check };
    serde_json::to_writer(&mut count, r).map_err(|e| e.to_string())?;
    Ok(count.n)
}
fn escaped_metadata(v: &serde_json::Value) -> Result<usize, String> {
    match v {
        serde_json::Value::String(s) => Ok(serde_json::to_vec(s).map_err(|e| e.to_string())?.len()),
        serde_json::Value::Array(a) => a.iter().try_fold(0usize, |sum, v| {
            sum.checked_add(escaped_metadata(v)?)
                .ok_or_else(|| "sequential metadata overflow".into())
        }),
        serde_json::Value::Object(o) => o.values().try_fold(0usize, |sum, v| {
            sum.checked_add(escaped_metadata(v)?)
                .ok_or_else(|| "sequential metadata overflow".into())
        }),
        _ => Ok(0),
    }
}
fn identity(i: &Identity) -> Result<(), String> {
    for s in [&i.id, &i.revision] {
        if s.trim().is_empty() || s.len() > 256 || s.chars().any(char::is_control) {
            return Err("bounded explicit sequential review/source identity required".into());
        }
    }
    Ok(())
}
fn calendar(r: &MortgageRequest) -> BTreeSet<i32> {
    let end = r.as_of_ordinal + r.horizon_days as i32;
    let mut days: BTreeSet<_> = r
        .payment_dates
        .iter()
        .copied()
        .chain(r.path.iter().map(|p| p.start_ordinal))
        .filter(|&d| d > r.as_of_ordinal && d <= end)
        .chain([end])
        .collect();
    if let mortgage_state::MortgageRate::Arm { resets, .. } = &r.rate {
        days.extend(
            resets
                .iter()
                .map(|x| x.ordinal)
                .filter(|&d| d > r.as_of_ordinal && d <= end),
        );
    }
    days
}
pub(super) fn admit_with_check(
    r: &Request,
    check: &mut dyn FnMut() -> Result<(), String>,
) -> Result<Admission, String> {
    check()?;
    let encoded = encoded_len(r, check)?;
    mortgage_state::validate_request(&r.contract)?;
    mortgage_state::validate_request(&r.original_contract)?;
    let c = &r.contract;
    let a = &r.acquisition;
    let p = &r.policy;
    let i = &p.interest;
    let original = &r.original_contract;
    if r.schema != SCHEMA
        || c.identity.measure != CashflowMeasure::Physical
        || c.monetary_unit != "currency_units"
        || c.horizon_days > 1080
        || !matches!(
            c.ownership,
            MortgageOwnership::WholeLoan {
                remittance_lag_days: 0,
                ..
            }
        )
        || c.opening_borrower_principal != c.opening_investor_principal
        || c.opening_borrower_interest != [0.; 3]
        || c.opening_investor_interest != [0.; 3]
        || c.opening_contractual_interest != 0.
        || a.purchase_ordinal != c.as_of_ordinal
        || a.actual_cost <= 0.
        || a.accounting_basis != "supplied_non_pcd_gross_cost-1"
        || !a.original_annual_effective_rate.is_finite()
        || !(-0.99..=10.).contains(&a.original_annual_effective_rate)
        || r.economic_source.revision != c.identity.source_revision
        || !near(a.actual_cost, a.opening_gross_carrying.iter().sum())
        || c.payment_dates
            .last()
            .is_none_or(|&d| d > c.as_of_ordinal + c.horizon_days as i32)
        || paired_terms(c)? != paired_terms(original)?
        || original.identity.measure != CashflowMeasure::Physical
        || r.original_economic_source.revision != original.identity.source_revision
        || r.pairing.original_run_id != original.identity.run_id
        || r.pairing.current_run_id != c.identity.run_id
        || r.pairing.policy != "supplied_physical_original_contract_unchanged-1"
    {
        return Err(
            "complete Physical equal-principal whole-loan zero-remittance acquired scope required"
                .into(),
        );
    }
    for id in [
        &a.identity,
        &a.original_yield_anchor,
        &a.income_review,
        &r.economic_source,
        &p.identity,
        &p.default_principal_book_review,
        &p.economic_principal_disposal_review,
        &i.review,
        &r.original_economic_source,
        &r.pairing.review,
    ] {
        identity(id)?;
    }
    identity(&Identity {
        id: r.path_id.clone(),
        revision: "draft".into(),
    })?;
    if p.original_income != "stop_at_exit-1"
        || p.remittance_float != "reject_nonzero_remittance_lag-1"
        || p.live_cash != "actual_native_cash_reduces_carry-1"
        || p.exit_basis != "signed_native_exit_cash_minus_exact_carry-1"
        || p.default_basis != "dispose_signed_basis_at_default-1"
        || p.default_principal_book != "chargeoff_native_loss_at_default-1"
        || p.tail != "reject_unclosed_gross_rights-1"
        || i.accounting_accrued != AccruedPolicy::ReverseIncomeAtDefault
        || i.economic_remainder != RemainderOwnership::ReviewedEconomicDisposal
        || i.economic_disposal_review.is_none()
        || i.disposal_lag_days > 1080
        || p.economic_principal_disposal_lag_days > 1080
    {
        return Err(
            "explicit supported book, economic disposal, float and full-tail policies required"
                .into(),
        );
    }
    identity(i.economic_disposal_review.as_ref().unwrap())?;
    amount(a.actual_cost)?;
    for (origin, &g) in a.opening_gross_carrying.iter().enumerate() {
        amount(g)?;
        if g < 0. || (c.opening_investor_principal[origin] == 0. && g != 0.) {
            return Err(
                "opening G must belong to an actual nonzero acquired principal origin".into(),
            );
        }
    }
    for origin in 0..3 {
        check()?;
        let f = i.collection_fraction[origin];
        let tail = &i.complete_collection_tails[origin];
        if !f.is_finite()
            || !(0.0..=1.0).contains(&f)
            || tail.len() > 32
            || (f == 0. && !tail.is_empty())
            || (f > 0. && tail.is_empty())
        {
            return Err("explicit complete bounded default-interest collections required".into());
        }
        let mut previous = None;
        let mut sum = 0.;
        for t in tail {
            if !t.weight.is_finite()
                || !(0.0..=1.0).contains(&t.weight)
                || t.lag_days > i.disposal_lag_days
                || previous.is_some_and(|x| t.lag_days <= x)
            {
                return Err("interest collection/disposal ordering invalid".into());
            }
            previous = Some(t.lag_days);
            sum += t.weight;
        }
        if !tail.is_empty() && (sum - 1.).abs() > 1e-12 {
            return Err("complete interest weights must sum to one without normalization".into());
        }
        if c.complete_recovery_tails[origin]
            .iter()
            .chain(&original.complete_recovery_tails[origin])
            .any(|t| t.lag_days > p.economic_principal_disposal_lag_days)
        {
            return Err(
                "principal economic disposal must follow complete native recoveries".into(),
            );
        }
    }
    let count = calendar(c).len();
    let original_count = calendar(original).len();
    let metadata = escaped_metadata(&serde_json::to_value(r).map_err(|e| e.to_string())?)?;
    let interest_rows = i
        .complete_collection_tails
        .iter()
        .map(Vec::len)
        .sum::<usize>();
    let rows_for = |contract: &MortgageRequest, n: usize| -> Result<usize, String> {
        let recovery = contract
            .complete_recovery_tails
            .iter()
            .map(Vec::len)
            .sum::<usize>();
        recovery
            .checked_mul(8)
            .and_then(|x| interest_rows.checked_mul(4).and_then(|y| x.checked_add(y)))
            .and_then(|x| x.checked_add(128))
            .and_then(|x| x.checked_mul(n))
            .ok_or_else(|| "sequential row bound overflow".into())
    };
    let rows = rows_for(c, count)?
        .checked_add(rows_for(original, original_count)?)
        .ok_or("sequential rows overflow")?;
    let intervals = count
        .checked_add(original_count)
        .ok_or("sequential interval bound overflow")?;
    let bytes = rows
        .checked_mul(
            metadata
                .checked_add(4096)
                .ok_or("sequential bytes overflow")?,
        )
        .and_then(|x| {
            intervals
                .checked_mul(metadata.checked_add(65536)?)
                .and_then(|y| x.checked_add(y))
        })
        .and_then(|x| encoded.checked_mul(4).and_then(|y| x.checked_add(y)))
        .ok_or("sequential retained bytes overflow")?;
    let work = count
        .checked_mul(140 * 14usize.pow(3) + 4096)
        .and_then(|x| {
            original_count
                .checked_mul(4096)
                .and_then(|y| x.checked_add(y))
        })
        .and_then(|x| x.checked_add(4_000_000))
        .and_then(|x| x.checked_add(crate::products::mortgages::receipts::source_code_bytes()))
        .ok_or("sequential work overflow")?;
    let growth =
        (a.original_annual_effective_rate.ln_1p().max(0.) * f64::from(c.horizon_days) / 365.).exp();
    if count > MAX_INTERVALS
        || original_count > MAX_INTERVALS
        || encoded > 1024 * 1024
        || bytes > MAX_BYTES
        || work > 9_000_000
        || a.actual_cost * growth > crate::validation::limits::MAX_MONETARY_AMOUNT
    {
        return Err("sequential interval/work/escaped retained byte admission exceeded".into());
    }
    let last_source = c.as_of_ordinal + c.horizon_days as i32;
    date(last_source, p.economic_principal_disposal_lag_days)?;
    date(last_source, i.disposal_lag_days)?;
    check()?;
    for contract in [c, original] {
        if contract.identity.position_id.len() + contract.identity.run_id.len() + 32 > 256 {
            return Err(
                "captured generator source identity exceeds bounded shared compiler domain".into(),
            );
        }
    }
    Ok(Admission {
        interval_count: count,
        original_interval_count: original_count,
        retained_row_bound: rows,
        retained_byte_bound: bytes,
        work_bound: work,
        generator_code_hash_bytes: crate::products::mortgages::receipts::source_code_bytes(),
    })
}
