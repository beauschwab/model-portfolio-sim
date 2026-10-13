//! Proposed Risk policy math: explicit physical comparative costs, no source or Ledger ownership.
pub fn positive_incremental_cost(
    current: f64,
    baseline: f64,
    annual_rate: f64,
    fraction: f64,
) -> Result<(f64, f64), String> {
    if !current.is_finite()
        || !baseline.is_finite()
        || current < 0.0
        || baseline < 0.0
        || !annual_rate.is_finite()
        || !(0.0..=1.0).contains(&annual_rate)
        || !fraction.is_finite()
        || fraction <= 0.0
        || fraction > 1.0
    {
        return Err("explicit nonnegative comparative cost domain required".into());
    }
    // Positive part belongs only to the declared cost methodology. Neither source
    // book balances nor cash, earnings or RWA are clipped.
    let incremental = (current - baseline).max(0.0);
    let amount = incremental * annual_rate * fraction;
    if !amount.is_finite() {
        return Err("comparative cost overflow".into());
    }
    Ok((incremental, amount))
}
pub fn option_spread(amount: f64, funding_basis: f64) -> Result<f64, String> {
    if !amount.is_finite() || !funding_basis.is_finite() || amount < 0.0 || funding_basis < 0.0 {
        return Err("nonnegative reviewed option-cost domain".into());
    }
    if funding_basis == 0.0 {
        return if amount == 0.0 {
            Ok(0.0)
        } else {
            Err("nonzero option at zero funding requires separately reviewed Risk dollar-option contract".into())
        };
    }
    let spread = amount / funding_basis;
    if !spread.is_finite() || spread > 1.0 {
        return Err("bounded derived option spread required".into());
    }
    Ok(spread)
}
