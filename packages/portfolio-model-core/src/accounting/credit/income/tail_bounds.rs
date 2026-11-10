//! Selected complete finite recovery/claim census. There is no deserializable
//! work allowance or monetary checkpoint. Historical wrappers never select it.
use super::*;

pub const POLICY: &str = "counted_finite_transfer_contract_factor_work-1";
#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
pub struct SourceShape {
    pub canonical_coupons: usize,
    pub pending_indices: usize,
    pub draw_tranches: usize,
}
#[derive(Debug, Clone, Copy, Default, Serialize, PartialEq, Eq)]
pub struct Counts {
    pub points: usize,
    pub contributions: usize,
    pub scheduled_source_cash: usize,
    pub terminal_source_cash: usize,
    pub recoveries: usize,
    pub coupon_recoveries: usize,
    pub terminal_path_events: usize,
}
#[derive(Debug, Clone, Copy)]
pub(super) enum Kind {
    Point,
    Contribution,
    ScheduledCash,
    TerminalCash,
    Recovery,
    CouponRecovery,
    TerminalEvent,
}
impl Counts {
    fn cell(&mut self, k: Kind) -> &mut usize {
        match k {
            Kind::Point => &mut self.points,
            Kind::Contribution => &mut self.contributions,
            Kind::ScheduledCash => &mut self.scheduled_source_cash,
            Kind::TerminalCash => &mut self.terminal_source_cash,
            Kind::Recovery => &mut self.recoveries,
            Kind::CouponRecovery => &mut self.coupon_recoveries,
            Kind::TerminalEvent => &mut self.terminal_path_events,
        }
    }
    fn times(mut self, n: usize) -> Result<Self, String> {
        for k in [
            Kind::Point,
            Kind::Contribution,
            Kind::ScheduledCash,
            Kind::TerminalCash,
            Kind::Recovery,
            Kind::CouponRecovery,
            Kind::TerminalEvent,
        ] {
            *self.cell(k) = mul(*self.cell(k), n)?;
        }
        Ok(self)
    }
}
#[derive(Debug, Clone, Serialize)]
pub struct Receipt {
    pub policy: &'static str,
    pub measure: crate::models::credit::transition::CreditMeasure,
    pub maximum_recoveries: usize,
    pub intervals: usize,
    pub boundaries: usize,
    pub maximum_jumps: usize,
    pub paths: usize,
    pub maximum_atoms: usize,
    pub maximum_terminal_steps: usize,
    pub source_cash_per_path: usize,
    pub gross_coupon_claims_per_path: usize,
    pub source_shape: SourceShape,
    pub per_path: Counts,
    pub row_upper: usize,
    pub request_bytes_upper: usize,
    pub binding_capacity_upper: usize,
    pub additional_work_upper: usize,
    pub additional_body_bytes_upper: usize,
    pub observed: Counts,
}
/// Exact native request/bound/C/L/measure binding. Only native callers can
/// derive this capability; a JSON request cannot supply it or override a cap.
pub struct NativeTailCensus {
    binding: Vec<u8>,
    bound: [usize; 6],
    receipt: Receipt,
}
fn structural(v: &serde_json::Value) -> Result<usize, String> {
    use serde_json::Value;
    match v {
        Value::Null => Ok(4),
        Value::Bool(_) => Ok(5),
        Value::Number(number) => {
            if number.is_i64() || number.is_u64() {
                return Ok(number.to_string().len());
            }
            let value = number.as_f64().ok_or("invalid finite metadata number")?;
            // Stable supplied scalars are not replaced by acquired money. These
            // small one-decimal values need at most sign/digits/dot/fraction;
            // all other f64s retain the full 32-byte general envelope.
            Ok(if value == 0. {
                4
            } else if (-10. ..=10.).contains(&value) && (value * 10.).round() / 10. == value {
                8
            } else {
                32
            })
        }
        Value::String(s) => crate::strict_json::serialized_size(s, MAX_RAW_BYTES),
        Value::Array(v) => v.iter().try_fold(2, |n, v| add(n, add(1, structural(v)?)?)),
        Value::Object(v) => v.iter().try_fold(2, |n, (k, v)| {
            add(
                n,
                add(
                    add(2, crate::strict_json::serialized_size(k, MAX_RAW_BYTES)?)?,
                    if k == "accrued_interest" || k == "book_adjustment" {
                        32
                    } else {
                        structural(v)?
                    },
                )?,
            )
        }),
    }
}
fn key(bound: &NativeFactorBound, cash: usize, lots: usize) -> [usize; 6] {
    [
        bound.maximum_atoms_per_query,
        bound.maximum_terminal_steps_per_path,
        bound.retained_source_bytes,
        bound.source_work,
        cash,
        lots,
    ]
}
impl NativeTailCensus {
    pub fn derive(
        request: &Request,
        bound: &NativeFactorBound,
        cash: usize,
        lots: usize,
        shape: SourceShape,
        check: &mut dyn FnMut() -> Result<(), String>,
    ) -> Result<Self, String> {
        check()?;
        crate::strict_json::serialized_size(request, MAX_RAW_BYTES)?;
        let p = &request.raw_credit_path;
        let mut recoveries = 0;
        for interval in &p.intervals {
            for tail in &interval.complete_recovery_tails {
                check()?;
                if tail.is_empty() || tail.len() > 32 {
                    return Err(
                        "selected finite tail requires complete1..32 recovery records".into(),
                    );
                }
                recoveries = recoveries.max(tail.len());
            }
        }
        if recoveries == 0
            || shape.canonical_coupons > lots
            || shape.pending_indices > bound.maximum_atoms_per_query
            || shape.draw_tranches > 4096
            || lots > 4096
            || bound.maximum_terminal_steps_per_path > 4096
        {
            return Err("selected finite source body exceeds sealed shape".into());
        }
        let n = p.paths.len();
        let i = p.intervals.len();
        let b = p.boundaries.len();
        let j = p.max_jumps_per_path;
        let s = add(add(i, b)?, add(j, 1)?)?;
        let base = add(add(add(s, b)?, j)?, 4)?;
        let t = bound.maximum_terminal_steps_per_path;
        let m = bound.maximum_atoms_per_query;
        let per_path = Counts {
            points: add(add(base, recoveries)?, mul(t, add(1, recoveries)?)?)?,
            contributions: add(add(mul(s, m)?, lots)?, t)?,
            scheduled_source_cash: cash,
            terminal_source_cash: mul(t, recoveries)?,
            recoveries,
            coupon_recoveries: mul(lots, recoveries)?,
            terminal_path_events: mul(t, recoveries)?,
        };
        let rows = mul(
            n,
            add(
                add(add(base, mul(2, recoveries)?)?, cash)?,
                add(
                    add(
                        mul(lots, add(1, recoveries)?)?,
                        mul(t, add(2, mul(2, recoveries)?)?)?,
                    )?,
                    mul(s, m)?,
                )?,
            )?,
        )?;
        let request_bytes = structural(&serde_json::to_value(request).map_err(|e| e.to_string())?)?;
        let binding_capacity = mul(add(request_bytes, 4096)?, 4)?;
        // Fixed units cover native struct/Vec growth, owned UTF8 labels and
        // allocator slack. Per-point nested bodies are charged independently.
        let point = add(
            512,
            add(
                mul(shape.canonical_coupons, 1024)?,
                add(
                    mul(shape.pending_indices, 32)?,
                    mul(shape.draw_tranches, 1024)?,
                )?,
            )?,
        )?;
        let point_unit = mul(point, 2)?.max(8192);
        let points = mul(n, per_path.points)?;
        let point_extra = mul(points, point_unit - 8192)?;
        let terminal_events = mul(t, add(1, recoveries)?)?;
        let temporary = add(
            add(mul(m, 4096)?, mul(lots, 4096)?)?,
            add(
                mul(shape.canonical_coupons, 4096)?,
                add(mul(t, 8192)?, mul(terminal_events, 256)?)?,
            )?,
        )?;
        let rebuild = if t > 0 {
            mul(add(points, 3)?, point_unit)?
        } else {
            0
        };
        let trace = mul(
            n,
            add(
                add(mul(s, 512)?, mul(add(b, j)?, 2048)?)?,
                add(mul(recoveries, 256)?, mul(mul(t, recoveries)?, 2048)?)?,
            )?,
        )?;
        let body = add(
            binding_capacity,
            add(point_extra, add(temporary, add(rebuild, trace)?)?)?,
        )?;
        // Canonical legal recomputation scans all retained source cash; the
        // default lot owner separately replays contribution/cash histories.
        let total_cash = mul(n, add(cash, mul(t, recoveries)?)?)?;
        let canonical = mul(mul(points, shape.canonical_coupons)?, add(total_cash, 64)?)?;
        let defaults = mul(
            n,
            add(
                mul(add(mul(s, m)?, lots)?, add(lots, 1)?)?,
                mul(cash, add(add(b, j)?, add(lots, 1)?)?)?,
            )?,
        )?;
        let sort_levels = (usize::BITS - terminal_events.max(1).leading_zeros()) as usize;
        let sort = mul(mul(n, terminal_events)?, mul(add(sort_levels, 1)?, 16)?)?;
        let filter_scans = mul(n, add(mul(n, per_path.contributions)?, total_cash)?)?;
        let additional_work = add(
            mul(request_bytes, 16)?,
            add(
                mul(add(add(canonical, defaults)?, filter_scans)?, 64)?,
                sort,
            )?,
        )?;
        let bound_key = key(bound, cash, lots);
        let binding =
            crate::strict_json::encode(&(request, bound_key, shape), MAX_RAW_BYTES + 4096)?;
        let receipt = Receipt {
            policy: POLICY,
            measure: p.measure,
            maximum_recoveries: recoveries,
            intervals: i,
            boundaries: b,
            maximum_jumps: j,
            paths: n,
            maximum_atoms: m,
            maximum_terminal_steps: t,
            source_cash_per_path: cash,
            gross_coupon_claims_per_path: lots,
            source_shape: shape,
            per_path,
            row_upper: rows,
            request_bytes_upper: request_bytes,
            binding_capacity_upper: binding_capacity,
            additional_work_upper: additional_work,
            additional_body_bytes_upper: body,
            observed: Counts::default(),
        };
        Ok(Self {
            binding,
            bound: bound_key,
            receipt,
        })
    }
    pub(super) fn verify(
        &self,
        r: &Request,
        b: &NativeFactorBound,
        c: usize,
        l: usize,
    ) -> Result<(), String> {
        if self.bound != key(b, c, l)
            || self.binding
                != crate::strict_json::encode(
                    &(r, self.bound, self.receipt.source_shape),
                    MAX_RAW_BYTES + 4096,
                )?
        {
            return Err("stale selected tail request/bound/cash/claim/measure authority".into());
        }
        Ok(())
    }
    pub fn point_upper(&self) -> Result<usize, String> {
        mul(self.receipt.paths, self.receipt.per_path.points)
    }
    pub(super) fn receipt(&self) -> &Receipt {
        &self.receipt
    }
    pub(super) fn into_meter(self) -> Result<Meter, String> {
        let upper = self.receipt.per_path.times(self.receipt.paths)?;
        Ok(Meter {
            receipt: self.receipt,
            upper,
            path: Counts::default(),
            usage: Counts::default(),
        })
    }
}
pub(super) struct Meter {
    receipt: Receipt,
    upper: Counts,
    path: Counts,
    usage: Counts,
}
impl Meter {
    pub(super) fn begin_path(&mut self) {
        self.path = Counts::default();
    }
    pub(super) fn bump(&mut self, k: Kind) -> Result<(), String> {
        let path = add(*self.path.cell(k), 1)?;
        let total = add(*self.usage.cell(k), 1)?;
        if path > *self.receipt.per_path.cell(k) || total > *self.upper.cell(k) {
            return Err("selected finite tail category exceeded before allocation".into());
        }
        *self.path.cell(k) = path;
        *self.usage.cell(k) = total;
        Ok(())
    }
    pub(super) fn maximum_recoveries(&self) -> usize {
        self.receipt.maximum_recoveries
    }
    pub(super) fn shape(&self) -> SourceShape {
        self.receipt.source_shape
    }
    pub(super) fn finish(mut self) -> Receipt {
        self.receipt.observed = self.usage;
        self.receipt
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn metadata_envelope_covers_exact_utf8_scalars_and_changed_buyer_money() {
        for tenth in -100..=100 {
            let v = serde_json::json!(f64::from(tenth) / 10.);
            assert!(
                structural(&v).unwrap()
                    >= crate::strict_json::serialized_size(&v, MAX_RAW_BYTES).unwrap()
            );
        }
        for value in [
            f64::MIN_POSITIVE,
            f64::MAX,
            -0.,
            0.30000000000000004,
            1e-30,
            -1e30,
        ] {
            let v = serde_json::json!(value);
            assert!(
                structural(&v).unwrap()
                    >= crate::strict_json::serialized_size(&v, MAX_RAW_BYTES).unwrap()
            );
        }
        let before = serde_json::json!({"id":"quoted\"\\\u{e9}", "accrued_interest":0., "book_adjustment":0.});
        let after = serde_json::json!({"id":"quoted\"\\\u{e9}", "accrued_interest":1e10, "book_adjustment":-1e10});
        assert_eq!(structural(&before).unwrap(), structural(&after).unwrap());
        assert!(
            structural(&before).unwrap()
                >= crate::strict_json::serialized_size(&after, MAX_RAW_BYTES).unwrap()
        );
    }
    #[test]
    fn every_category_refuses_before_mutation_at_zero_and_at_exact_limit() {
        let kinds = [
            Kind::Point,
            Kind::Contribution,
            Kind::ScheduledCash,
            Kind::TerminalCash,
            Kind::Recovery,
            Kind::CouponRecovery,
            Kind::TerminalEvent,
        ];
        let receipt = Receipt {
            policy: POLICY,
            measure: crate::models::credit::transition::CreditMeasure::Physical,
            maximum_recoveries: 1,
            intervals: 1,
            boundaries: 0,
            maximum_jumps: 1,
            paths: 1,
            maximum_atoms: 1,
            maximum_terminal_steps: 0,
            source_cash_per_path: 0,
            gross_coupon_claims_per_path: 0,
            source_shape: SourceShape {
                canonical_coupons: 0,
                pending_indices: 0,
                draw_tranches: 0,
            },
            per_path: Counts::default(),
            row_upper: 0,
            request_bytes_upper: 0,
            binding_capacity_upper: 0,
            additional_work_upper: 0,
            additional_body_bytes_upper: 0,
            observed: Counts::default(),
        };
        for k in kinds {
            let mut meter = Meter {
                receipt: receipt.clone(),
                upper: Counts::default(),
                path: Counts::default(),
                usage: Counts::default(),
            };
            assert!(meter.bump(k).is_err());
            assert_eq!(meter.usage, Counts::default());
            *meter.receipt.per_path.cell(k) = 1;
            *meter.upper.cell(k) = 1;
            meter.bump(k).unwrap();
            assert_eq!(*meter.usage.cell(k), 1);
            assert!(meter.bump(k).is_err());
            assert_eq!(*meter.usage.cell(k), 1);
            meter.begin_path();
            assert!(meter.bump(k).is_err());
            assert_eq!(*meter.usage.cell(k), 1);
        }
    }
}
