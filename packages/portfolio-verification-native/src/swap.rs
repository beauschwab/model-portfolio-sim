//! Independent clean-value and due-cash equations for the raw swap contracts. Fixed and
//! indexed legs are summed from the supplied periods, observations and exact discount points;
//! nothing is interpolated, projected or calibrated here.
use crate::near;
use serde_json::Value;

fn year_fraction(start: i64, end: i64, convention: &str) -> Result<f64, String> {
    let days = (end - start) as f64;
    match convention {
        "act360" => Ok(days / 360.),
        "act365" | "act365_fixed" => Ok(days / 365.),
        other => Err(format!(
            "unsupported day count `{other}` for the independent swap equation"
        )),
    }
}
fn f(v: &Value, key: &str) -> Result<f64, String> {
    v.get(key)
        .and_then(Value::as_f64)
        .ok_or_else(|| format!("swap field `{key}` missing"))
}
fn i(v: &Value, key: &str) -> Result<i64, String> {
    v.get(key)
        .and_then(Value::as_i64)
        .ok_or_else(|| format!("swap field `{key}` missing"))
}
fn s<'a>(v: &'a Value, key: &str) -> Result<&'a str, String> {
    v.get(key)
        .and_then(Value::as_str)
        .ok_or_else(|| format!("swap field `{key}` missing"))
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Valuation {
    pub clean_value: f64,
    pub due_cash: f64,
}

/// Clean value after scheduled payments on the as-of date, and the contractual cash due that
/// day, signed from the entity's side (receipts positive).
pub fn value(swap: &Value) -> Result<Valuation, String> {
    let asof = i(swap, "asof_ordinal")?;
    let notional = f(swap, "notional")?;
    let fixed_rate = f(swap, "fixed_rate")?;
    let sign = match s(swap, "side")? {
        "payer" => -1.,
        "receiver" => 1.,
        other => return Err(format!("unsupported swap side `{other}`")),
    };
    let curve = swap.get("discount_curve").ok_or("discount curve missing")?;
    if i(curve, "asof_ordinal")? != asof {
        return Err("discount curve is not anchored at the swap as-of date".into());
    }
    let points = curve
        .get("points")
        .and_then(Value::as_array)
        .ok_or("discount points missing")?;
    let discount = |ordinal: i64| -> Result<f64, String> {
        points
            .iter()
            .find(|p| p.get("ordinal").and_then(Value::as_i64) == Some(ordinal))
            .map(|p| f(p, "discount_factor"))
            .transpose()?
            .filter(|d| *d > 0.)
            .ok_or_else(|| format!("no exact discount point for {ordinal}"))
    };
    let (mut clean, mut due) = (0., 0.);
    for p in swap
        .get("fixed_periods")
        .and_then(Value::as_array)
        .ok_or("fixed periods missing")?
    {
        let tau = year_fraction(
            i(p, "start_ordinal")?,
            i(p, "end_ordinal")?,
            s(p, "day_count")?,
        )?;
        let pay = i(p, "payment_ordinal")?;
        let amount = sign * notional * fixed_rate * tau;
        if pay > asof {
            clean += amount * discount(pay)?;
        } else if pay == asof {
            due += amount;
        }
    }
    for p in swap
        .get("floating_periods")
        .and_then(Value::as_array)
        .ok_or("floating periods missing")?
    {
        let coupon = p.get("coupon").ok_or("floating coupon missing")?;
        if s(coupon, "kind")? != "indexed"
            || s(coupon, "calculation")? != "term_simple"
            || s(coupon, "margin_treatment")? != "simple"
        {
            return Err(
                "the independent equation covers term-simple indexed coupons with a simple margin"
                    .into(),
            );
        }
        if f(p, "fee")? != 0. {
            return Err("swap period fees are outside the independent equation".into());
        }
        let observations = coupon
            .get("observations")
            .and_then(Value::as_array)
            .ok_or("observations missing")?;
        if observations.len() != 1 {
            return Err("one observation per term period is expected".into());
        }
        let rate = f(&observations[0], "rate")? + f(coupon, "margin")?;
        let tau = year_fraction(
            i(p, "start_ordinal")?,
            i(p, "end_ordinal")?,
            s(p, "day_count")?,
        )?;
        let pay = i(p, "payment_ordinal")?;
        let amount = -sign * notional * rate * tau;
        if pay > asof {
            clean += amount * discount(pay)?;
        } else if pay == asof {
            due += amount;
        }
    }
    Ok(Valuation {
        clean_value: clean,
        due_cash: due,
    })
}

/// Research-method bucket sensitivities: the clean-value change when every discount point after
/// the as-of date in a bucket (first bound at or above its day count, else the last) is scaled by
/// exp(-0.0001 * days / 365), one basis point of continuously compounded act/365 zero rate.
pub fn bucket_sensitivities(swap: &Value, upper: &[i64]) -> Result<Vec<f64>, String> {
    let base = value(swap)?.clean_value;
    let asof = i(swap, "asof_ordinal")?;
    let mut out = Vec::with_capacity(upper.len());
    for b in 0..upper.len() {
        let mut bumped = swap.clone();
        let points = bumped
            .get_mut("discount_curve")
            .and_then(|c| c.get_mut("points"))
            .and_then(Value::as_array_mut)
            .ok_or("discount points missing")?;
        for p in points.iter_mut() {
            let days = i(p, "ordinal")? - asof;
            let bucket = upper
                .iter()
                .position(|u| days <= *u)
                .unwrap_or(upper.len() - 1);
            if bucket == b && days > 0 {
                let d = f(p, "discount_factor")? * (-0.0001 * days as f64 / 365.).exp();
                p["discount_factor"] = Value::from(d);
            }
        }
        out.push(value(&bumped)?.clean_value - base);
    }
    Ok(out)
}

pub fn check(swap: &Value, clean_value: f64, due_cash: f64) -> Result<(), String> {
    let v = value(swap)?;
    if !near(v.clean_value, clean_value) || !near(v.due_cash, due_cash) {
        return Err(format!(
            "native swap valuation differs from the independent equation: clean {} vs {}, due {} vs {}",
            clean_value, v.clean_value, due_cash, v.due_cash
        ));
    }
    Ok(())
}
