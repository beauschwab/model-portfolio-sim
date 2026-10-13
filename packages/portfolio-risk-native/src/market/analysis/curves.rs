//! Direct discount and index pseudo-discount factors, not a par-quote bootstrap.
use super::{label, range};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum CurveRole {
    Collateral { collateral_index: String },
    Index { index_id: String },
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CurveTail {
    Reject,
    FlatZero,
    FlatForward,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SuppliedCurve {
    pub curve_id: String,
    pub source_id: String,
    pub source_revision: String,
    pub currency: String,
    pub role: CurveRole,
    pub times_years: Vec<f64>,
    pub discount_factors: Vec<f64>,
    pub right_tail: CurveTail,
}
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct CurveValue {
    pub discount_factor: f64,
    pub extrapolated: bool,
}
impl SuppliedCurve {
    pub fn validate(&self) -> Result<(), String> {
        for s in [
            &self.curve_id,
            &self.source_id,
            &self.source_revision,
            &self.currency,
        ] {
            label(s, 256)?;
        }
        match &self.role {
            CurveRole::Collateral { collateral_index } => label(collateral_index, 256)?,
            CurveRole::Index { index_id } => label(index_id, 256)?,
        }
        if self.times_years.len() < 2
            || self.times_years.len() > 4096
            || self.times_years.len() != self.discount_factors.len()
            || self.times_years[0] != 0.
            || self.discount_factors[0] != 1.
        {
            return Err("curve requires exact (0,1) anchor and 2..4096 matching nodes".into());
        }
        for &t in &self.times_years {
            range(t, 0., 100., "curve time")?;
        }
        for &df in &self.discount_factors {
            range(df, 1e-100, 1e100, "discount factor")?;
        }
        if self.times_years.windows(2).any(|w| w[1] <= w[0]) {
            return Err("curve times must be strictly increasing".into());
        }
        Ok(())
    }
    pub fn value(&self, t: f64) -> Result<CurveValue, String> {
        self.validate()?;
        self.value_validated(t)
    }
    /// Crate callers must validate the immutable supplied curve before lookup.
    pub(crate) fn value_validated(&self, t: f64) -> Result<CurveValue, String> {
        range(t, 0., 100., "curve query time")?;
        let i = self.times_years.partition_point(|x| *x < t);
        if i < self.times_years.len() && self.times_years[i] == t {
            return Ok(CurveValue {
                discount_factor: self.discount_factors[i],
                extrapolated: false,
            });
        }
        let n = self.times_years.len();
        let last = n - 1;
        let (log, extra) = if i == n {
            let end_log = self.discount_factors[last].ln();
            let end = self.times_years[last];
            let log = match self.right_tail {
                CurveTail::Reject => return Err("curve query beyond declared right tail".into()),
                CurveTail::FlatZero => end_log * t / end,
                CurveTail::FlatForward => {
                    end_log
                        + (end_log - self.discount_factors[last - 1].ln()) * (t - end)
                            / (end - self.times_years[last - 1])
                }
            };
            (log, true)
        } else {
            let start = i - 1;
            let weight =
                (t - self.times_years[start]) / (self.times_years[i] - self.times_years[start]);
            (
                self.discount_factors[start].ln()
                    + (self.discount_factors[i].ln() - self.discount_factors[start].ln()) * weight,
                false,
            )
        };
        let df = log.exp();
        range(df, 1e-100, 1e100, "interpolated discount factor")?;
        Ok(CurveValue {
            discount_factor: df,
            extrapolated: extra,
        })
    }
    pub fn simple_forward(
        &self,
        start: f64,
        end: f64,
        accrual_fraction: f64,
    ) -> Result<(f64, bool), String> {
        self.validate()?;
        range(accrual_fraction, 1e-12, 100., "forward accrual fraction")?;
        if !end.is_finite() || end <= start {
            return Err("forward end must follow start".into());
        }
        let a = self.value_validated(start)?;
        let b = self.value_validated(end)?;
        let forward = (a.discount_factor.ln() - b.discount_factor.ln()).exp_m1() / accrual_fraction;
        range(forward, -100., 100., "implied simple forward")?;
        Ok((forward, a.extrapolated || b.extrapolated))
    }
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ForwardQuery {
    pub collateral_curve_id: String,
    pub index_curve_id: String,
    pub start_year: f64,
    pub end_year: f64,
    pub accrual_fraction: f64,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ForwardResult {
    pub collateral_curve_id: String,
    pub index_curve_id: String,
    pub collateral_discount_factor: f64,
    pub collateral_implied_forward: f64,
    pub index_forward: f64,
    pub index_basis: f64,
    pub collateral_tail_used: bool,
    pub index_tail_used: bool,
}
pub fn forward_query(curves: &[SuppliedCurve], q: &ForwardQuery) -> Result<ForwardResult, String> {
    let collateral = curves
        .iter()
        .find(|c| c.curve_id == q.collateral_curve_id)
        .ok_or("unknown collateral curve")?;
    let index = curves
        .iter()
        .find(|c| c.curve_id == q.index_curve_id)
        .ok_or("unknown index curve")?;
    if !matches!(collateral.role, CurveRole::Collateral { .. })
        || !matches!(index.role, CurveRole::Index { .. })
        || collateral.currency != index.currency
    {
        return Err(
            "forward query requires distinct collateral/index curve roles and matching currency"
                .into(),
        );
    }
    let (c, ct) = collateral.simple_forward(q.start_year, q.end_year, q.accrual_fraction)?;
    let (i, it) = index.simple_forward(q.start_year, q.end_year, q.accrual_fraction)?;
    Ok(ForwardResult {
        collateral_curve_id: q.collateral_curve_id.clone(),
        index_curve_id: q.index_curve_id.clone(),
        collateral_discount_factor: collateral.value(q.end_year)?.discount_factor,
        collateral_implied_forward: c,
        index_forward: i,
        index_basis: i - c,
        collateral_tail_used: ct,
        index_tail_used: it,
    })
}
