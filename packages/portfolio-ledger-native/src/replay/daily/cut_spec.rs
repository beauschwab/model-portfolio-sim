//! Diagnostic immutable raw-specification census, never a production permit.
//! Every native field is exhaustively mapped. Only borrowed views and fixed-size
//! counters are retained; no Value tree, copied position list or encoded buffer.
//! Compiled owners, source clocks, binding/fee identities and ordered observed
//! groups must still be checked independently by the caller.
use super::{cut_prefix::TypedPrefixShape, cut_shape};
use crate::transport::types::{
    Account, Cashflow, NettingSet, Policy, Position, Scenario, SharedText, Spec,
};
use serde::Serialize;

type R<T> = Result<T, String>;
fn add(a: usize, b: usize) -> R<usize> {
    a.checked_add(b)
        .ok_or("typed cut specification census overflow".into())
}
fn tick(work: usize) -> R<()> {
    portfolio_compute_control::checkpoint()?;
    cut_shape::charge(work)
}
fn ticket() -> R<usize> {
    cut_shape::spent().ok_or("typed cut specification requires a remaining-work ticket".into())
}

#[derive(Default)]
struct Counter(TypedPrefixShape);
impl Counter {
    fn bytes(&mut self, bytes: usize) -> R<()> {
        self.0.encoded_bytes_upper = add(self.0.encoded_bytes_upper, bytes)?;
        Ok(())
    }
    fn node(&mut self, bytes: usize) -> R<()> {
        tick(1)?;
        self.0.cells = add(self.0.cells, 1)?;
        self.bytes(bytes)
    }
    fn text(&mut self, text: &str) -> R<()> {
        // Pay the entire scan before observing any string byte; cancellation is
        // checked between bounded chunks without extra uncharged traversal.
        tick(text.len())?;
        self.0.string_utf8_bytes = add(self.0.string_utf8_bytes, text.len())?;
        self.bytes(2)?;
        for chunk in text.as_bytes().chunks(1024) {
            portfolio_compute_control::checkpoint()?;
            for byte in chunk {
                self.bytes(match byte {
                    b'"' | b'\\' | b'\n' | b'\r' | b'\t' | 8 | 12 => 2,
                    0..=31 => 6,
                    _ => 1,
                })?;
            }
        }
        Ok(())
    }
    fn field(&mut self, name: &str) -> R<()> {
        tick(1)?;
        self.0.declared_field_visits = add(self.0.declared_field_visits, 1)?;
        self.0.emitted_fields = add(self.0.emitted_fields, 1)?;
        self.text(name)?;
        self.bytes(1)
    }
}

trait Typed: Serialize {
    fn visit(&self, c: &mut Counter) -> R<()>;
    fn same(&self, other: &Self) -> R<bool>;
    /// Requested heap of a fresh native Clone, excluding inline Self. Shared
    /// Arc text and borrowed views retain their existing owners without copies.
    fn cloned_heap(&self) -> R<usize>;
    #[cfg(test)]
    fn layout_value(&self) -> R<serde_json::Value> {
        serde_json::to_value(self).map_err(|e| e.to_string())
    }
}
impl<T: Typed + ?Sized> Typed for &T {
    fn cloned_heap(&self) -> R<usize> {
        tick(1)?;
        Ok(0)
    }
    fn visit(&self, c: &mut Counter) -> R<()> {
        (**self).visit(c)
    }
    fn same(&self, other: &Self) -> R<bool> {
        (**self).same(*other)
    }
    #[cfg(test)]
    fn layout_value(&self) -> R<serde_json::Value> {
        (**self).layout_value()
    }
}
impl Typed for f64 {
    fn cloned_heap(&self) -> R<usize> {
        tick(1)?;
        Ok(0)
    }
    fn visit(&self, c: &mut Counter) -> R<()> {
        // Charge even a rejected nonfinite observation before inspecting it.
        c.node(32)?;
        if !self.is_finite() {
            return Err("typed cut specification requires finite f64".into());
        }
        Ok(())
    }
    fn same(&self, other: &Self) -> R<bool> {
        tick(1)?;
        if !self.is_finite() || !other.is_finite() {
            return Err("typed cut specification requires finite f64".into());
        }
        Ok(self.to_bits() == other.to_bits())
    }
}
impl Typed for usize {
    fn cloned_heap(&self) -> R<usize> {
        tick(1)?;
        Ok(0)
    }
    fn visit(&self, c: &mut Counter) -> R<()> {
        c.node(32)
    }
    fn same(&self, other: &Self) -> R<bool> {
        tick(1)?;
        Ok(self == other)
    }
}
impl Typed for bool {
    fn cloned_heap(&self) -> R<usize> {
        tick(1)?;
        Ok(0)
    }
    fn visit(&self, c: &mut Counter) -> R<()> {
        c.node(5)
    }
    fn same(&self, other: &Self) -> R<bool> {
        tick(1)?;
        Ok(self == other)
    }
}
impl Typed for str {
    fn cloned_heap(&self) -> R<usize> {
        tick(1)?;
        Ok(0)
    }
    fn visit(&self, c: &mut Counter) -> R<()> {
        c.node(0)?;
        c.text(self)
    }
    fn same(&self, other: &Self) -> R<bool> {
        tick(add(1, self.len().max(other.len()))?)?;
        if self.len() != other.len() {
            return Ok(false);
        }
        for (a, b) in self
            .as_bytes()
            .chunks(1024)
            .zip(other.as_bytes().chunks(1024))
        {
            portfolio_compute_control::checkpoint()?;
            if a != b {
                return Ok(false);
            }
        }
        Ok(true)
    }
}
macro_rules! text {
    ($t:ty, $shared:expr) => {
        impl Typed for $t {
            fn cloned_heap(&self) -> R<usize> {
                tick(1)?;
                Ok(if $shared { 0 } else { self.as_str().len() })
            }
            fn visit(&self, c: &mut Counter) -> R<()> {
                self.as_str().visit(c)
            }
            fn same(&self, other: &Self) -> R<bool> {
                self.as_str().same(other.as_str())
            }
        }
    };
}
text!(String, false);
text!(SharedText, true);
impl<T: Typed> Typed for [T] {
    fn cloned_heap(&self) -> R<usize> {
        tick(1)?;
        self.iter()
            .try_fold(0, |bytes, value| add(bytes, value.cloned_heap()?))
    }
    fn visit(&self, c: &mut Counter) -> R<()> {
        c.node(add(2, self.len().saturating_sub(1))?)?;
        for v in self {
            v.visit(c)?;
        }
        Ok(())
    }
    fn same(&self, other: &Self) -> R<bool> {
        tick(1)?;
        if self.len() != other.len() {
            return Ok(false);
        }
        for (a, b) in self.iter().zip(other) {
            if !a.same(b)? {
                return Ok(false);
            }
        }
        Ok(true)
    }
    #[cfg(test)]
    fn layout_value(&self) -> R<serde_json::Value> {
        self.iter()
            .map(Typed::layout_value)
            .collect::<R<Vec<_>>>()
            .map(serde_json::Value::Array)
    }
}
impl<T: Typed> Typed for Vec<T> {
    fn cloned_heap(&self) -> R<usize> {
        let slots = self
            .len()
            .checked_mul(std::mem::size_of::<T>())
            .ok_or("typed specification clone overflow")?;
        add(slots, self.as_slice().cloned_heap()?)
    }
    fn visit(&self, c: &mut Counter) -> R<()> {
        self.as_slice().visit(c)
    }
    fn same(&self, other: &Self) -> R<bool> {
        self.as_slice().same(other.as_slice())
    }
    #[cfg(test)]
    fn layout_value(&self) -> R<serde_json::Value> {
        self.as_slice().layout_value()
    }
}
macro_rules! typed_struct {
    ($t:ty, {$($f:ident),+ $(,)?}) => {
        impl Typed for $t {
            fn cloned_heap(&self) -> R<usize> {
                let Self { $($f: _,)+ } = self;
                tick(1)?;
                let mut heap = 0usize;
                $(heap = add(heap, self.$f.cloned_heap()?)?;)+
                Ok(heap)
            }
            fn visit(&self, c: &mut Counter) -> R<()> {
                // No `..`: every added native field requires an explicit review.
                let Self { $($f: _,)+ } = self;
                c.node(2)?;
                let mut count = 0usize;
                $(if count > 0 { c.bytes(1)?; }
                  count = add(count, 1)?;
                  c.field(stringify!($f))?;
                  self.$f.visit(c)?;)+
                let _ = count;
                Ok(())
            }
            fn same(&self, other: &Self) -> R<bool> {
                let Self { $($f: _,)+ } = self;
                let Self { $($f: _,)+ } = other;
                tick(1)?;
                $(tick(1)?; if !self.$f.same(&other.$f)? { return Ok(false); })+
                Ok(true)
            }
            #[cfg(test)]
            fn layout_value(&self) -> R<serde_json::Value> {
                let Self { $($f: _,)+ } = self;
                let mut fields = serde_json::Map::new();
                $(fields.insert(stringify!($f).into(), self.$f.layout_value()?);)+
                Ok(serde_json::Value::Object(fields))
            }
        }
    };
}
typed_struct!(Account, {id, entity, currency, cash, equity, cash_floor, cet1_floor, leverage_floor, capital_deductions,
    include_aoci, annual_fees, annual_costs, tax_rate, annual_dividends, lcr_floor, nsfr_floor, htm_asset_limit,
    lcr_applicability, nsfr_applicability, htm_accounting, htm_taint_materiality});
typed_struct!(Position, {id, account, kind, balance, rate, payment_interval_days, floating_beta, maturity_day, classification,
    risk_weight, asf_weight, rsf_weight, lcr_outflow_weight, hqla_weight, duration, allowance, annual_pd, lgd, watch_fraction,
    watch_pd_multiplier, migration_rate, recovery_days, commitment, draw_fraction, monthly_runoff, uninsured_fraction,
    concentration, operational_fraction, digital_fraction, rollover, collateral_pool, eligible_fraction, encumbered_fraction,
    haircut, source_id, book_adjustment, opening_accrued, collateral_position, pledged_face, start_day, hqla_level,
    lcr_inflow_weight, opening_market_price});
typed_struct!(NettingSet, {id, account, counterparty, fair_value, posted_margin, received_margin, stress_loss,
    margin_threshold, margin_delay, annual_pd, lgd, wrong_way_multiplier, risk_weight});
typed_struct!(Policy, {id, account, kind, trigger_cash, limit, delay_days, position, destination, execution_cost,
    funding_rate, allow_htm_sale, htm_sale_limit, funding_tenor_days, sale_reason, approval_day});
typed_struct!(Scenario, {name, start_day, rate_shift, spread_shift, deposit_flight, pd_multiplier, migration_multiplier,
    lgd_add, draw_multiplier, rollover_loss, haircut_add, market_shock, outage_days});
typed_struct!(Cashflow, {position, day, principal, cash_interest, accrual_interest, book_amortization, scenario});
typed_struct!(Spec, {horizon_days, accounts, positions, netting_sets, policies, scenarios, reverse_severities, cashflows, ruleset});

/// Exact requested allocations of the retained normalized specification. The
/// original owners of shared Position strings remain a separate source phase.
/// No JSON shape multiplier or input spare-capacity assumption enters this term.
pub(super) fn specification_clone_bytes(spec: &Spec) -> R<usize> {
    add(std::mem::size_of::<Spec>(), spec.cloned_heap()?)
}

/// The same complete serialization as Spec, borrowing a chosen position list.
#[derive(Serialize)]
struct SpecView<'a> {
    horizon_days: usize,
    accounts: &'a [Account],
    positions: &'a [Position],
    netting_sets: &'a [NettingSet],
    policies: &'a [Policy],
    scenarios: &'a [Scenario],
    reverse_severities: &'a [f64],
    cashflows: &'a [Cashflow],
    ruleset: &'a str,
}
fn view<'a>(spec: &'a Spec, positions: &'a [Position]) -> SpecView<'a> {
    let Spec {
        horizon_days,
        accounts,
        positions: _,
        netting_sets,
        policies,
        scenarios,
        reverse_severities,
        cashflows,
        ruleset,
    } = spec;
    SpecView {
        horizon_days: *horizon_days,
        accounts,
        positions,
        netting_sets,
        policies,
        scenarios,
        reverse_severities,
        cashflows,
        ruleset,
    }
}
typed_struct!(SpecView<'_>, {horizon_days, accounts, positions, netting_sets, policies, scenarios, reverse_severities, cashflows, ruleset});

fn census<T: Typed + ?Sized>(value: &T) -> R<TypedPrefixShape> {
    ticket()?;
    let mut c = Counter::default();
    value.visit(&mut c)?;
    Ok(c.0)
}

#[derive(Debug, Serialize)]
pub struct TypedCutSpecificationInventory {
    pub completed_shape: TypedPrefixShape,
    pub revised_shape: TypedPrefixShape,
    pub completed_positions: usize,
    pub appended_positions: usize,
    pub immutable_comparison_work_upper: usize,
    pub actual_census_ticket_work: usize,
    /// Raw immutable fields and retained positions only, not compiled ownership.
    pub immutable_specification_verified: bool,
    pub compiled_ownership_verified: bool,
}

/// Diagnostic only. New positions must satisfy the existing raw AC zero-opening
/// conditions; exclusive compiled ownership and future source clocks are separate.
pub fn commercial_cut_typed_specification_inventory(
    old: &Spec,
    new: &Spec,
    cut: usize,
) -> R<TypedCutSpecificationInventory> {
    let before = ticket()?;
    tick(1)?;
    if cut >= old.horizon_days || new.positions.len() < old.positions.len() {
        return Err(
            "typed cut specification requires surviving positions and future calendar".into(),
        );
    }
    let completed_shape = census(old)?;
    let revised_shape = census(new)?;
    let retained = &new.positions[..old.positions.len()];
    if !view(old, &old.positions).same(&view(new, &old.positions))?
        || !old.positions.as_slice().same(retained)?
    {
        return Err("typed cut immutable specification or retained position differs".into());
    }
    for p in &new.positions[old.positions.len()..] {
        tick(8)?;
        if !p.classification.as_str().same("ac")?
            || [
                p.balance,
                p.book_adjustment,
                p.opening_accrued,
                p.allowance,
                p.commitment,
            ]
            .iter()
            .any(|x| *x != 0.)
            || !p.collateral_position.is_empty()
        {
            return Err("typed cut appended position requires AC zero opening money and no collateral position".into());
        }
    }
    // Full pair twice bounds header-with-original-positions plus retained-prefix
    // comparison. It is intentionally conservative, with no admission reduction.
    let pair_work = completed_shape.comparison_work_upper(revised_shape)?;
    let immutable_comparison_work_upper = add(pair_work, pair_work)?;
    Ok(TypedCutSpecificationInventory {
        completed_shape,
        revised_shape,
        completed_positions: old.positions.len(),
        appended_positions: new.positions.len() - old.positions.len(),
        immutable_comparison_work_upper,
        actual_census_ticket_work: ticket()?
            .checked_sub(before)
            .ok_or("typed cut specification ticket regressed")?,
        immutable_specification_verified: true,
        compiled_ownership_verified: false,
    })
}

#[cfg(test)]
mod tests;
