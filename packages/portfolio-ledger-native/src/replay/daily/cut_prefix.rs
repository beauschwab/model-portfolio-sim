//! Diagnostic typed source census only. No cut admission or execution uses this
//! module. Exhaustive native destructuring prevents silent field loss; selected
//! unsupported reviewed states refuse. Money is observed, never calculated.
use super::cut_shape;
use portfolio_model_core::{
    commercial_coupon_accounting::{self as book, AtomicGroup, CouponLot, OwnedPosting, Snapshot},
    credit_accounting::{AccountCode, Identity, PostingIntent},
    credit_path_income::CanonicalCouponLegalBalance,
    nonintegral_fee as fee,
};
use serde::Serialize;
type R<T> = Result<T, String>;
fn tick(work: usize) -> R<()> {
    portfolio_compute_control::checkpoint()?;
    cut_shape::charge(work)
}

#[derive(Clone, Copy, Debug, Default, Serialize, PartialEq, Eq)]
pub struct TypedPrefixShape {
    pub encoded_bytes_upper: usize,
    pub cells: usize,
    pub declared_field_visits: usize,
    pub emitted_fields: usize,
    pub string_utf8_bytes: usize,
}
fn add(a: usize, b: usize) -> R<usize> {
    a.checked_add(b)
        .ok_or("typed prefix census overflow".into())
}
impl TypedPrefixShape {
    fn checked_add(self, other: Self) -> R<Self> {
        Ok(Self {
            encoded_bytes_upper: add(self.encoded_bytes_upper, other.encoded_bytes_upper)?,
            cells: add(self.cells, other.cells)?,
            declared_field_visits: add(self.declared_field_visits, other.declared_field_visits)?,
            emitted_fields: add(self.emitted_fields, other.emitted_fields)?,
            string_utf8_bytes: add(self.string_utf8_bytes, other.string_utf8_bytes)?,
        })
    }
    pub fn census_work_upper(self) -> R<usize> {
        add(
            add(self.cells, self.declared_field_visits)?,
            self.string_utf8_bytes,
        )
    }
    /// Includes omitted-field discriminant visits as well as all visible values.
    pub fn comparison_work_upper(self, other: Self) -> R<usize> {
        let visits = add(
            add(self.cells, self.declared_field_visits)?,
            add(other.cells, other.declared_field_visits)?,
        )?;
        add(
            visits
                .checked_mul(4)
                .ok_or("typed prefix comparison overflow")?,
            add(self.string_utf8_bytes, other.string_utf8_bytes)?,
        )
    }
}

#[derive(Default)]
struct Counter(TypedPrefixShape);
impl Counter {
    fn node(&mut self, bytes: usize) -> R<()> {
        tick(1)?;
        self.0.cells = add(self.0.cells, 1)?;
        self.bytes(bytes)
    }
    fn bytes(&mut self, bytes: usize) -> R<()> {
        self.0.encoded_bytes_upper = add(self.0.encoded_bytes_upper, bytes)?;
        Ok(())
    }
    fn text(&mut self, value: &str) -> R<()> {
        if value.len() > 1024 {
            return Err("typed prefix text layout currently supports at most1024UTF8bytes".into());
        }
        // Charge before iterating UTF8 bytes, including escaped controls.
        tick(value.len())?;
        self.0.string_utf8_bytes = add(self.0.string_utf8_bytes, value.len())?;
        self.bytes(2)?;
        for byte in value.bytes() {
            self.bytes(match byte {
                b'"' | b'\\' | b'\n' | b'\r' | b'\t' | 8 | 12 => 2,
                0..=31 => 6,
                _ => 1,
            })?;
        }
        Ok(())
    }
    fn guard(&mut self) -> R<()> {
        tick(1)?;
        self.0.declared_field_visits = add(self.0.declared_field_visits, 1)?;
        Ok(())
    }
    fn field(&mut self, name: &str) -> R<()> {
        if self.0.emitted_fields == usize::MAX {
            return Err("typed prefix field overflow".into());
        }
        self.0.emitted_fields += 1;
        self.text(name)?;
        self.bytes(1) // colon; object commas are counted by the caller.
    }
}

trait Typed: Serialize {
    fn visit(&self, c: &mut Counter) -> R<()>;
    fn same(&self, other: &Self) -> R<bool>;
    #[cfg(test)]
    fn layout_value(&self) -> R<serde_json::Value> {
        serde_json::to_value(self).map_err(|e| e.to_string())
    }
}
impl Typed for f64 {
    fn visit(&self, c: &mut Counter) -> R<()> {
        if !self.is_finite() {
            return Err("typed prefix finite f64 required".into());
        }
        c.node(32)
    }
    fn same(&self, other: &Self) -> R<bool> {
        tick(1)?;
        if !self.is_finite() || !other.is_finite() {
            return Err("typed prefix finite f64 required".into());
        }
        Ok(self.to_bits() == other.to_bits())
    }
}
macro_rules! integer {
    ($($t:ty),+) => { $(impl Typed for $t {
        fn visit(&self, c:&mut Counter)->R<()> { c.node(32) }
        fn same(&self, other:&Self)->R<bool> { tick(1)?; Ok(self==other) }
    })+ };
}
integer!(usize, u64, u32, i32, i64);
impl Typed for bool {
    fn visit(&self, c: &mut Counter) -> R<()> {
        c.node(5)
    }
    fn same(&self, other: &Self) -> R<bool> {
        tick(1)?;
        Ok(self == other)
    }
}
impl Typed for str {
    fn visit(&self, c: &mut Counter) -> R<()> {
        c.node(0)?;
        c.text(self)
    }
    fn same(&self, other: &Self) -> R<bool> {
        if self.len() > 1024 || other.len() > 1024 {
            return Err("typed prefix text layout currently supports at most1024UTF8bytes".into());
        }
        tick(add(1, self.len().max(other.len()))?)?;
        Ok(self == other)
    }
}
impl Typed for String {
    fn visit(&self, c: &mut Counter) -> R<()> {
        self.as_str().visit(c)
    }
    fn same(&self, other: &Self) -> R<bool> {
        self.as_str().same(other.as_str())
    }
}
impl<T: Typed> Typed for Option<T> {
    #[cfg(test)]
    fn layout_value(&self) -> R<serde_json::Value> {
        match self {
            Some(v) => v.layout_value(),
            None => Ok(serde_json::Value::Null),
        }
    }
    fn visit(&self, c: &mut Counter) -> R<()> {
        match self {
            Some(v) => v.visit(c),
            None => c.node(4),
        }
    }
    fn same(&self, other: &Self) -> R<bool> {
        tick(1)?;
        match (self, other) {
            (Some(a), Some(b)) => a.same(b),
            (None, None) => Ok(true),
            _ => Ok(false),
        }
    }
}
impl<T: Typed> Typed for Vec<T> {
    #[cfg(test)]
    fn layout_value(&self) -> R<serde_json::Value> {
        self.iter()
            .map(Typed::layout_value)
            .collect::<R<Vec<_>>>()
            .map(serde_json::Value::Array)
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
}
macro_rules! visible {
    ($v:expr, always) => {
        true
    };
    ($v:expr, some) => {
        $v.is_some()
    };
    ($v:expr, nonempty) => {
        !$v.is_empty()
    };
    ($v:expr, nonzero) => {
        $v != 0.
    };
    ($v:expr, trueonly) => {
        $v
    };
}
macro_rules! typed_struct {
    ($t:ty, {$($f:ident:$visibility:ident),+ $(,)?})=>{
        impl Typed for $t {
            #[cfg(test)]
            fn layout_value(&self)->R<serde_json::Value> {
                let Self { $($f:_,)+ }=self;
                let mut fields=serde_json::Map::new();
                $(if visible!(self.$f,$visibility) {fields.insert(stringify!($f).into(),self.$f.layout_value()?);})+
                Ok(serde_json::Value::Object(fields))
            }
            fn visit(&self,c:&mut Counter)->R<()> {
                // Intentionally no `..`: a new native field fails compilation.
                let Self { $($f:_,)+ }=self;
                c.node(2)?;
                let mut count=0usize;
                $(c.guard()?;
                  if visible!(self.$f,$visibility) {
                    if count>0 { c.bytes(1)?; }
                    count=add(count,1)?;
                    c.field(stringify!($f))?;
                    self.$f.visit(c)?;
                  })+
                let _=count;
                Ok(())
            }
            fn same(&self,other:&Self)->R<bool> {
                let Self { $($f:_,)+ }=self;
                let Self { $($f:_,)+ }=other;
                tick(1)?;
                $(tick(1)?;
                  let a=visible!(self.$f,$visibility);
                  let b=visible!(other.$f,$visibility);
                  if a!=b {return Ok(false);}
                  if a && !self.$f.same(&other.$f)? {return Ok(false);})+
                Ok(true)
            }
        }
    };
}
macro_rules! typed_enum {
    ($t:ty, {$($variant:ident=>$label:literal),+ $(,)?})=>{
        impl Typed for $t {
            #[cfg(test)]
            fn layout_value(&self)->R<serde_json::Value> {Ok(serde_json::Value::String(match self {$(Self::$variant=>$label,)+}.into()))}
            fn visit(&self,c:&mut Counter)->R<()> {
                let text=match self { $(Self::$variant=>$label,)+ };
                text.visit(c)
            }
            fn same(&self,other:&Self)->R<bool> {
                // Exhaustive match is an evolution guard even for future enums.
                let a=match self { $(Self::$variant=>$label,)+ };
                let b=match other { $(Self::$variant=>$label,)+ };
                a.same(b)
            }
        }
    };
}
typed_enum!(AccountCode, {Cash=>"cash",LivePrincipal=>"live_principal",DefaultPrincipal=>"default_principal",
    LiveAccruedInterest=>"live_accrued_interest",DefaultAccruedInterest=>"default_accrued_interest",
    LiveBookAdjustment=>"live_book_adjustment",DefaultBookAdjustment=>"default_book_adjustment",
    Allowance=>"allowance",InterestIncome=>"interest_income",CreditLossExpense=>"credit_loss_expense",
    BasisRecognitionExpense=>"basis_recognition_expense",RecoveryIncome=>"recovery_income"});
typed_enum!(fee::Account, {Cash=>"cash",FeeReceivable=>"fee_receivable",FeePayable=>"fee_payable",
    DeferredFeeIncome=>"deferred_fee_income",PrepaidFeeExpense=>"prepaid_fee_expense",FeeIncome=>"fee_income",
    FeeWriteoffExpense=>"fee_writeoff_expense",FeeRecoveryIncome=>"fee_recovery_income",AcquiredFeeAsset=>"acquired_fee_asset",
    AcquiredFeeLiability=>"acquired_fee_liability",AcquiredFeeAssetValuationAdjustment=>"acquired_fee_asset_valuation_adjustment",
    AcquiredFeeLiabilityValuationAdjustment=>"acquired_fee_liability_valuation_adjustment",FeeAcquisitionExpense=>"fee_acquisition_expense"});
typed_enum!(fee::Action, {Acquire=>"acquire",AcquireTransferredStock=>"acquire_transferred_stock",ReleaseAcquiredCost=>"release_acquired_cost",
    Earn=>"earn",Cash=>"cash",RefundPrepaid=>"refund_prepaid",RefundEarned=>"refund_earned",WaiveUnpaidEarned=>"waive_unpaid_earned",
    CancelUnearned=>"cancel_unearned",RecoverWaived=>"recover_waived"});
typed_enum!(fee::Reason, {Service=>"service",Settlement=>"settlement",Default=>"default",Exercise=>"exercise",Maturity=>"maturity",OtherReviewed=>"other_reviewed"});
typed_struct!(Identity,{id:always,revision:always});
typed_struct!(PostingIntent,{account:always,amount:always});
typed_struct!(OwnedPosting,{coupon_id:always,earning_origin_index:always,posting:always});
typed_struct!(CouponLot,{coupon_id:always,source_event_id:always,source_id:always,source_revision:always,native_audit_index:always,native_cash_index:always,
    earning_origin_index:always,contractual_earned:always,legal_cash_collected:always,legal_remaining:always,recognized_earned:always,
    recognized_cash_applied:always,recognized_remaining:always,default_claim:always});
typed_struct!(CanonicalCouponLegalBalance,{coupon_id:always,native_audit_index:always,native_cash_index:always,opening_entitlement_principal:always,
    cumulative_factor:always,contractual_earned:always,actual_interest_collected:always,legal_remaining:always});
impl Typed for book::reviews::ReviewedSnapshot {
    fn visit(&self, _: &mut Counter) -> R<()> {
        let Self {
            legal_principal: _,
            model_gross_carry: _,
            allowance: _,
            claims: _,
            legal_waivers: _,
        } = self;
        Err("typed prefix reviewed-credit state layout is not yet independently mapped".into())
    }
    fn same(&self, _: &Self) -> R<bool> {
        let mut c = Counter::default();
        self.visit(&mut c)?;
        Ok(false)
    }
}
typed_struct!(Snapshot,{path_key:always,year:always,principal:always,recognized_interest:always,legal_interest:always,
    native_legal_reduction_difference:always,book_adjustment:always,gross_carry:always,in_default:always,reviewed_credit_state:some,
    lots:always,pending_coupon_obligation_indices:nonempty,canonical_legal_coupons:nonempty});
typed_struct!(AtomicGroup,{path_key:always,year:always,phase:always,policy:always,source_event_id:always,effective_interest:always,
    basis_realization:always,cash:always,postings:always,next:always});
typed_struct!(fee::Posting,{account:always,amount:always});
typed_struct!(fee::Event,{event_id:always,fee_id:always,source:always,review:always,ordinal:always,sequence:always,action:always,reason:always,amount:always});
typed_struct!(fee::State,{acquired_fee_asset_valuation_adjustment:nonzero,acquired_fee_liability_valuation_adjustment:nonzero,
    transferred_net_stock:nonzero,acquired_stock_selected:trueonly,acquired_cost:nonzero,released_acquired_cost:nonzero,acquired_fee_asset:nonzero,
    acquired_fee_liability:nonzero,earned:always,refunded_earned:nonzero,cancelled_unearned:always,waived_earned:always,recovered_waived:always,
    net_service_cash:always,fee_receivable:always,fee_payable:always,deferred_fee_income:always,prepaid_fee_expense:always,unearned_entitlement:always});
typed_struct!(fee::Group,{event:always,policy:always,postings:always,next:always});
#[derive(Serialize)]
struct BindingIdentityView<'a> {
    scenario: &'a str,
    position_id: &'a str,
    path_key: u64,
    origin_day: usize,
    source_class: &'static str,
    source_weight: f64,
    allocated_face: f64,
    native_unit_face: f64,
}
typed_struct!(BindingIdentityView<'_>,{scenario:always,position_id:always,path_key:always,origin_day:always,source_class:always,
    source_weight:always,allocated_face:always,native_unit_face:always});
#[derive(Serialize)]
struct FeeIdentityView<'a> {
    loan_binding: usize,
    native_position_id: &'a str,
}
typed_struct!(FeeIdentityView<'_>,{loan_binding:always,native_position_id:always});

fn census<T: Typed>(value: &T) -> R<TypedPrefixShape> {
    if cut_shape::spent().is_none() {
        return Err("typed prefix census requires a remaining-work ticket".into());
    }
    let mut c = Counter::default();
    value.visit(&mut c)?;
    Ok(c.0)
}
pub fn commercial_loan_prefix_shape(group: &AtomicGroup) -> R<TypedPrefixShape> {
    census(group)
}
pub fn commercial_fee_prefix_shape(group: &fee::Group) -> R<TypedPrefixShape> {
    census(group)
}
pub fn commercial_loan_prefix_equal(a: &AtomicGroup, b: &AtomicGroup) -> R<bool> {
    if cut_shape::spent().is_none() {
        return Err("typed prefix comparison requires a remaining-work ticket".into());
    }
    a.same(b)
}
pub fn commercial_fee_prefix_equal(a: &fee::Group, b: &fee::Group) -> R<bool> {
    if cut_shape::spent().is_none() {
        return Err("typed prefix comparison requires a remaining-work ticket".into());
    }
    a.same(b)
}

/// Source-stage and search dimensions only. This is not a complete Sim work or
/// allocator admission proof: generic close-day/output/native owners are separate.
#[derive(Default, Debug, Serialize)]
pub struct BorrowedCutSourceInventory {
    pub loan_bindings: usize,
    pub fee_bindings: usize,
    pub loan_groups: usize,
    pub fee_groups: usize,
    pub loan_stage_rows: usize,
    pub fee_stage_rows: usize,
    pub compiled_work_formula: usize,
    pub compiled_component_bytes_formula: usize,
    pub loan_postings: usize,
    pub fee_postings: usize,
    pub loan_lot_occurrences: usize,
    pub maximum_lots_per_group: usize,
    pub loan_stock_key_upper: usize,
    pub maximum_coupon_key_bytes: usize,
    pub loan_two_stock_search_steps_upper: usize,
    pub loan_reconcile_stock_search_steps_upper: usize,
    pub loan_stock_clone_entries_upper: usize,
    pub loan_legal_clone_entries_upper: usize,
    pub fee_stock_key_upper: usize,
    pub fee_stock_clone_entries_upper: usize,
    pub source_group_shape: TypedPrefixShape,
    pub fee_native_year_shape: TypedPrefixShape,
    pub maximum_snapshot_encoded_bytes_upper: usize,
}
fn mul(a: usize, b: usize) -> R<usize> {
    a.checked_mul(b)
        .ok_or("typed prefix census overflow".into())
}

fn borrowed_sources(
    c: &crate::owners::commercial::intents::Compiled<'_>,
    horizon: usize,
) -> R<BorrowedCutSourceInventory> {
    use std::borrow::Cow;
    let mut result = BorrowedCutSourceInventory {
        loan_bindings: c.bindings.len(),
        fee_bindings: c.fees.len(),
        ..BorrowedCutSourceInventory::default()
    };
    for b in &c.bindings {
        tick(1)?;
        if b.accounting.compact_bytes().is_some() {
            return Err("typed prefix inventory supports borrowed finite owners only".into());
        }
        let mut postings = 0usize;
        let mut lots = 0usize;
        let mut groups = 0usize;
        for index in 0..b.accounting.group_count() {
            tick(1)?;
            let native = b.accounting.group(index);
            let Cow::Borrowed(g) = native else {
                return Err(
                    "typed prefix inventory refuses materializing accounting recipes".into(),
                );
            };
            if g.path_key != b.path_key {
                return Err(
                    "typed prefix inventory requires one complete path per borrowed owner".into(),
                );
            }
            result.source_group_shape = result.source_group_shape.checked_add(census(g)?)?;
            let snapshot = census(&g.next)?;
            result.maximum_snapshot_encoded_bytes_upper = result
                .maximum_snapshot_encoded_bytes_upper
                .max(snapshot.encoded_bytes_upper);
            groups = add(groups, 1)?;
            postings = add(postings, g.postings.len())?;
            lots = add(lots, g.next.lots.len())?;
            result.maximum_lots_per_group = result.maximum_lots_per_group.max(g.next.lots.len());
            result.loan_stage_rows = add(
                result.loan_stage_rows,
                add(g.postings.len(), add(mul(g.next.lots.len(), 2)?, 1)?)?,
            )?;
            for p in &g.postings {
                tick(1)?;
                result.maximum_coupon_key_bytes = result
                    .maximum_coupon_key_bytes
                    .max(p.coupon_id.as_ref().map_or(0, String::len));
            }
            for lot in &g.next.lots {
                tick(1)?;
                result.maximum_coupon_key_bytes =
                    result.maximum_coupon_key_bytes.max(lot.coupon_id.len());
            }
        }
        result.loan_groups = add(result.loan_groups, groups)?;
        result.loan_postings = add(result.loan_postings, postings)?;
        result.loan_lot_occurrences = add(result.loan_lot_occurrences, lots)?;
        // No value/weight pruning. Every historical posting could create a key;
        // every lot occurrence could create a legal key. Two linear searches per
        // posting, four full principal/basis scans and two searches per lot.
        result.loan_stock_key_upper = add(result.loan_stock_key_upper, postings)?;
        result.loan_two_stock_search_steps_upper = add(
            result.loan_two_stock_search_steps_upper,
            mul(mul(2, postings)?, postings)?,
        )?;
        result.loan_reconcile_stock_search_steps_upper = add(
            result.loan_reconcile_stock_search_steps_upper,
            mul(add(mul(4, groups)?, mul(2, lots)?)?, postings)?,
        )?;
        result.loan_stock_clone_entries_upper = add(
            result.loan_stock_clone_entries_upper,
            mul(groups, postings)?,
        )?;
        result.loan_legal_clone_entries_upper =
            add(result.loan_legal_clone_entries_upper, mul(groups, lots)?)?;
    }
    for f in &c.fees {
        tick(1)?;
        if f.source.compact_bytes().is_some() {
            return Err("typed prefix inventory supports borrowed finite fee owners only".into());
        }
        if f.native_years.len() != f.source.group_count() {
            return Err("typed prefix fee clock coverage differs".into());
        }
        let keys = mul(13, f.source.fee_count())?;
        result.fee_stock_key_upper = add(result.fee_stock_key_upper, keys)?;
        result.fee_stock_clone_entries_upper = add(
            result.fee_stock_clone_entries_upper,
            mul(keys, f.source.group_count())?,
        )?;
        for index in 0..f.source.group_count() {
            tick(1)?;
            result.fee_native_year_shape = result
                .fee_native_year_shape
                .checked_add(census(&f.native_years[index])?)?;
            let native = f.source.group(index);
            let Cow::Borrowed(g) = native else {
                return Err("typed prefix inventory refuses materializing fee recipes".into());
            };
            result.source_group_shape = result.source_group_shape.checked_add(census(g)?)?;
            let snapshot = census(&g.next)?;
            result.maximum_snapshot_encoded_bytes_upper = result
                .maximum_snapshot_encoded_bytes_upper
                .max(snapshot.encoded_bytes_upper);
            result.fee_groups = add(result.fee_groups, 1)?;
            result.fee_postings = add(result.fee_postings, g.postings.len())?;
            result.fee_stage_rows = add(result.fee_stage_rows, add(g.postings.len(), 2)?)?;
        }
    }
    let rows = add(result.loan_stage_rows, result.fee_stage_rows)?;
    result.compiled_work_formula = mul(mul(rows, add(horizon, 1)?)?, 16)?;
    result.compiled_component_bytes_formula = mul(rows, 4096)?;
    Ok(result)
}

#[derive(Debug, Serialize)]
pub struct TypedCutPrefixInventory {
    pub specification: super::cut_spec::TypedCutSpecificationInventory,
    pub completed: BorrowedCutSourceInventory,
    pub revised: BorrowedCutSourceInventory,
    pub prefix_events_per_side: usize,
    pub prefix_loan_events_per_side: usize,
    pub prefix_fee_events_per_side: usize,
    pub prefix_pair_shape: TypedPrefixShape,
    pub scheduled_layout_bytes: usize,
    pub ordered_event_workspace_bytes_upper: usize,
    pub ordering_comparison_steps_upper: usize,
    pub accessor_lookup_work_upper: usize,
    pub scenario_clone_bytes_upper: usize,
    pub accessor_string_workspace_bytes_upper: usize,
    pub typed_equality_work_upper: usize,
    pub binding_identity_pair_shape: TypedPrefixShape,
    pub binding_identity_equality_work_upper: usize,
    pub future_binding_identity_shape: TypedPrefixShape,
    pub fee_year_equality_work_upper: usize,
    pub actual_census_ticket_work: usize,
    pub immutable_specification_verified: bool,
    pub complete_sim_bound_available: bool,
    pub raw_source_capacity_bound_available: bool,
}
#[derive(Default)]
struct BindingInventory {
    pair: TypedPrefixShape,
    equality_work_upper: usize,
    future: TypedPrefixShape,
}
fn binding_view<'a>(
    b: &crate::owners::commercial::intents::Binding<'a>,
) -> BindingIdentityView<'a> {
    use crate::owners::commercial::intents::SourceClass;
    BindingIdentityView {
        scenario: b.scenario,
        position_id: b.position_id,
        path_key: b.path_key,
        origin_day: b.origin_day,
        source_class: match b.source_class {
            SourceClass::CommercialLoan => "commercial_loan-1",
            SourceClass::IssuerAcSecurity => "issuer_ac_security-1",
        },
        source_weight: b.source_weight,
        allocated_face: b.allocated_face,
        native_unit_face: b.native_unit_face,
    }
}
fn same_bindings(
    a: &crate::owners::commercial::intents::Compiled<'_>,
    b: &crate::owners::commercial::intents::Compiled<'_>,
    cut: usize,
) -> R<BindingInventory> {
    use crate::owners::commercial::intents::{Binding, FeeBinding};
    if a.bindings.len() > b.bindings.len() || a.fees.len() > b.fees.len() {
        return Err("typed prefix surviving ownership coverage differs".into());
    }
    let mut inventory = BindingInventory::default();
    for (a, b) in a.bindings.iter().zip(&b.bindings) {
        let Binding {
            source_class: _,
            scenario: _,
            position_id: _,
            path_key: _,
            origin_day: _,
            source_weight: _,
            allocated_face: _,
            native_unit_face: _,
            accounting: _,
        } = a;
        let Binding {
            source_class: _,
            scenario: _,
            position_id: _,
            path_key: _,
            origin_day: _,
            source_weight: _,
            allocated_face: _,
            native_unit_face: _,
            accounting: _,
        } = b;
        tick(1)?;
        let va = binding_view(a);
        let vb = binding_view(b);
        let sa = census(&va)?;
        let sb = census(&vb)?;
        inventory.pair = inventory.pair.checked_add(sa)?.checked_add(sb)?;
        inventory.equality_work_upper =
            add(inventory.equality_work_upper, sa.comparison_work_upper(sb)?)?;
        if !va.same(&vb)? {
            return Err("typed prefix exact surviving binding identity differs".into());
        }
    }
    for b in &b.bindings[a.bindings.len()..] {
        tick(1)?;
        if b.origin_day <= cut {
            return Err("typed prefix appended source is not future-only".into());
        }
        inventory.future = inventory.future.checked_add(census(&binding_view(b))?)?;
    }
    for (a, b) in a.fees.iter().zip(&b.fees) {
        let FeeBinding {
            loan_binding: _,
            native_position_id: _,
            source: _,
            native_years: _,
        } = a;
        let FeeBinding {
            loan_binding: _,
            native_position_id: _,
            source: _,
            native_years: _,
        } = b;
        tick(1)?;
        let va = FeeIdentityView {
            loan_binding: a.loan_binding,
            native_position_id: a.native_position_id,
        };
        let vb = FeeIdentityView {
            loan_binding: b.loan_binding,
            native_position_id: b.native_position_id,
        };
        let sa = census(&va)?;
        let sb = census(&vb)?;
        inventory.pair = inventory.pair.checked_add(sa)?.checked_add(sb)?;
        inventory.equality_work_upper =
            add(inventory.equality_work_upper, sa.comparison_work_upper(sb)?)?;
        if !va.same(&vb)? {
            return Err("typed prefix exact fee parent identity differs".into());
        }
    }
    for f in &b.fees[a.fees.len()..] {
        inventory.future = inventory.future.checked_add(census(&FeeIdentityView {
            loan_binding: f.loan_binding,
            native_position_id: f.native_position_id,
        })?)?;
    }
    Ok(inventory)
}
/// Observe the actual ordered Loan/Fee union with exact source times and groups.
/// Not called by any existing cut route; deliberately grants no admission.
pub fn commercial_cut_typed_prefix_inventory(
    completed: &crate::transport::types::Spec,
    revised: &crate::transport::types::Spec,
    original: &crate::owners::commercial::intents::Compiled<'_>,
    future: &crate::owners::commercial::intents::Compiled<'_>,
    cut: usize,
) -> R<TypedCutPrefixInventory> {
    use crate::owners::commercial::intents::Scheduled;
    if cut_shape::spent().is_none() {
        return Err("typed prefix census requires a remaining-work ticket".into());
    }
    if usize::BITS != 64 {
        return Err("typed prefix layout currently requires an audited64bit target".into());
    }
    if completed.horizon_days != revised.horizon_days || cut >= completed.horizon_days {
        return Err("typed prefix complete calendar differs".into());
    }
    if !completed.reverse_severities.is_empty() || !revised.reverse_severities.is_empty() {
        return Err("typed prefix reverse sources unsupported".into());
    }
    let before = cut_shape::spent().ok_or("typed prefix work ticket missing")?;
    let mut specification =
        super::cut_spec::commercial_cut_typed_specification_inventory(completed, revised, cut)?;
    if original.position_owners.len() != completed.positions.len()
        || future.position_owners.len() != revised.positions.len()
    {
        return Err("typed prefix compiled position coverage differs".into());
    }
    for (a, b) in original.position_owners.iter().zip(&future.position_owners) {
        tick(1)?;
        if a != b {
            return Err("typed prefix existing position ownership differs".into());
        }
    }
    for owner in &future.position_owners[original.position_owners.len()..] {
        tick(1)?;
        if !owner {
            return Err(
                "typed prefix appended position requires exclusive compiled ownership".into(),
            );
        }
    }
    specification.compiled_ownership_verified = true;
    let bindings = same_bindings(original, future, cut)?;
    let a = borrowed_sources(original, completed.horizon_days)?;
    let b = borrowed_sources(future, revised.horizon_days)?;
    let groups = add(
        add(a.loan_groups, a.fee_groups)?,
        add(b.loan_groups, b.fee_groups)?,
    )?;
    let mut maximum_scenario_bytes = "baseline".len();
    for name in completed
        .scenarios
        .iter()
        .chain(&revised.scenarios)
        .map(|s| s.name.as_str())
        .chain(
            original
                .bindings
                .iter()
                .chain(&future.bindings)
                .map(|b| b.scenario),
        )
    {
        tick(1)?;
        maximum_scenario_bytes = maximum_scenario_bytes.max(name.len());
    }
    let accessor_strings = mul(
        2,
        add(
            std::mem::size_of::<String>(),
            mul(2, maximum_scenario_bytes)?,
        )?,
    )?;
    // Both returned ordered Vecs and sort scratch coexist. This explicit upper
    // is separate from all source/Sink/Sim capacities, which remain unproved.
    let layout = std::mem::size_of::<Scheduled>();
    if layout > 3 * std::mem::size_of::<usize>() {
        return Err("typed prefix scheduled layout changed".into());
    }
    let workspace = add(
        add(
            mul(mul(groups, 6)?, layout)?,
            mul(2, std::mem::size_of::<Vec<Scheduled>>())?,
        )?,
        accessor_strings,
    )?;
    if workspace > 32 << 20 {
        return Err("typed prefix ordered workspace exceeds component32MiB before growth".into());
    }
    let mut result = TypedCutPrefixInventory {
        specification,
        completed: a,
        revised: b,
        prefix_events_per_side: 0,
        prefix_loan_events_per_side: 0,
        prefix_fee_events_per_side: 0,
        prefix_pair_shape: TypedPrefixShape::default(),
        scheduled_layout_bytes: layout,
        ordered_event_workspace_bytes_upper: workspace,
        ordering_comparison_steps_upper: 0,
        typed_equality_work_upper: 0,
        actual_census_ticket_work: 0,
        accessor_lookup_work_upper: 0,
        scenario_clone_bytes_upper: 0,
        accessor_string_workspace_bytes_upper: accessor_strings,
        binding_identity_pair_shape: bindings.pair,
        binding_identity_equality_work_upper: bindings.equality_work_upper,
        future_binding_identity_shape: bindings.future,
        fee_year_equality_work_upper: 0,
        immutable_specification_verified: true,
        complete_sim_bound_available: false,
        raw_source_capacity_bound_available: false,
    };
    for scenario in
        std::iter::once("baseline").chain(completed.scenarios.iter().map(|s| s.name.as_str()))
    {
        for day in 0..=cut {
            // Precharge a conservative quadratic ordering bound BEFORE the
            // existing accessor allocates/sorts either complete scheduled union.
            let sorting = mul(mul(groups, groups)?, 32)?;
            // Retain the historical conservative charge until the complete
            // source/Compiled/Sim census is independently proved. Immutable
            // metadata indices now avoid the old per-day Strings and sort.
            let clone_bytes = mul(4, scenario.len())?;
            let lookup = mul(
                mul(4, add(groups, 1)?)?,
                add(mul(2, maximum_scenario_bytes)?, 2)?,
            )?;
            tick(add(add(add(1, sorting)?, clone_bytes)?, lookup)?)?;
            result.ordering_comparison_steps_upper =
                add(result.ordering_comparison_steps_upper, sorting)?;
            result.accessor_lookup_work_upper = add(result.accessor_lookup_work_upper, lookup)?;
            result.scenario_clone_bytes_upper =
                add(result.scenario_clone_bytes_upper, clone_bytes)?;
            let left = original.ordered_events(scenario, day);
            let right = future.ordered_events(scenario, day);
            if left.len() != right.len() {
                return Err("typed prefix ordered union coverage differs".into());
            }
            for (left, right) in left.zip(right) {
                tick(1)?;
                let (sa, sb) =
                    match (left, right) {
                        (Scheduled::Loan(x, i), Scheduled::Loan(y, j)) if x == y && i == j => {
                            let a = original.bindings[x].accounting.group(i);
                            let b = future.bindings[y].accounting.group(j);
                            let sa = commercial_loan_prefix_shape(&a)?;
                            let sb = commercial_loan_prefix_shape(&b)?;
                            if !commercial_loan_prefix_equal(&a, &b)? {
                                return Err("typed prefix exact loan group differs".into());
                            }
                            result.prefix_loan_events_per_side =
                                add(result.prefix_loan_events_per_side, 1)?;
                            (sa, sb)
                        }
                        (Scheduled::Fee(x, i), Scheduled::Fee(y, j)) if x == y && i == j => {
                            if !original.fees[x].native_years[i]
                                .same(&future.fees[y].native_years[j])?
                            {
                                return Err("typed prefix fee year bits differ".into());
                            }
                            result.fee_year_equality_work_upper =
                                add(result.fee_year_equality_work_upper, 8)?;
                            let a = original.fees[x].source.group(i);
                            let b = future.fees[y].source.group(j);
                            let sa = commercial_fee_prefix_shape(&a)?;
                            let sb = commercial_fee_prefix_shape(&b)?;
                            if !commercial_fee_prefix_equal(&a, &b)? {
                                return Err("typed prefix exact fee group differs".into());
                            }
                            result.prefix_fee_events_per_side =
                                add(result.prefix_fee_events_per_side, 1)?;
                            (sa, sb)
                        }
                        _ => return Err(
                            "typed prefix Loan/Fee discriminator or binding/group order differs"
                                .into(),
                        ),
                    };
                result.prefix_pair_shape =
                    result.prefix_pair_shape.checked_add(sa)?.checked_add(sb)?;
                result.typed_equality_work_upper = add(
                    result.typed_equality_work_upper,
                    sa.comparison_work_upper(sb)?,
                )?;
                result.prefix_events_per_side = add(result.prefix_events_per_side, 1)?;
            }
        }
    }
    result.actual_census_ticket_work = cut_shape::spent()
        .ok_or("typed prefix work ticket missing")?
        .checked_sub(before)
        .ok_or("typed prefix work ticket regressed")?;
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn immutable_specification_composes_with_exact_compiled_position_coverage() {
        use crate::{owners::commercial::intents::Compiled, transport::types::Spec};
        let spec: Spec = serde_json::from_value(serde_json::json!({
            "horizon_days": 30,
            "accounts": [{"id":"bank", "entity":"bank", "currency":"USD", "cash":100., "equity":100.}],
            "scenarios": [{"name":"stress"}],
            "positions": [{"id":"old", "account":"bank", "kind":"loan", "balance":0.}]
        })).unwrap();
        // Structural controls only: no financial source or permission is made.
        let mut old = Compiled::new(&spec, vec![]).unwrap();
        let mut future = Compiled::new(&spec, vec![]).unwrap();
        let result = cut_shape::with_budget(100_000, || {
            commercial_cut_typed_prefix_inventory(&spec, &spec, &old, &future, 1)
        })
        .unwrap();
        assert!(result.immutable_specification_verified);
        assert!(result.specification.compiled_ownership_verified);
        assert!(
            !result.complete_sim_bound_available && !result.raw_source_capacity_bound_available
        );
        future.position_owners[0] = true;
        assert!(cut_shape::with_budget(100_000, || {
            commercial_cut_typed_prefix_inventory(&spec, &spec, &old, &future, 1)
        })
        .unwrap_err()
        .contains("existing position ownership differs"));
        future.position_owners[0] = false;
        old.position_owners.clear();
        assert!(cut_shape::with_budget(100_000, || {
            commercial_cut_typed_prefix_inventory(&spec, &spec, &old, &future, 1)
        })
        .unwrap_err()
        .contains("compiled position coverage differs"));
        let old = Compiled::new(&spec, vec![]).unwrap();
        let mut revised = spec.clone();
        let mut appended = revised.positions[0].clone();
        appended.id = "new".into();
        revised.positions.push(appended);
        let future = Compiled::new(&revised, vec![]).unwrap();
        assert!(cut_shape::with_budget(100_000, || {
            commercial_cut_typed_prefix_inventory(&spec, &revised, &old, &future, 1)
        })
        .unwrap_err()
        .contains("appended position requires exclusive compiled ownership"));
        revised.accounts[0].cash += 1.;
        assert!(cut_shape::with_budget(100_000, || {
            commercial_cut_typed_prefix_inventory(&spec, &revised, &old, &future, 1)
        })
        .unwrap_err()
        .contains("immutable specification"));
    }
    fn group() -> AtomicGroup {
        AtomicGroup {
            path_key: 1,
            year: 0.,
            phase: "reviewed_opening_snapshot",
            policy: Identity {
                id: "id".into(),
                revision: "r".into(),
            },
            source_event_id: None,
            effective_interest: 0.,
            basis_realization: 0.,
            cash: -100.,
            postings: vec![OwnedPosting {
                coupon_id: None,
                earning_origin_index: None,
                posting: PostingIntent {
                    account: AccountCode::Cash,
                    amount: -100.,
                },
            }],
            next: Snapshot {
                path_key: 1,
                year: 0.,
                principal: 100.,
                recognized_interest: 0.,
                legal_interest: 0.,
                native_legal_reduction_difference: 0.,
                book_adjustment: 0.,
                gross_carry: 100.,
                in_default: false,
                reviewed_credit_state: None,
                lots: vec![],
                pending_coupon_obligation_indices: vec![],
                canonical_legal_coupons: vec![],
            },
        }
    }
    fn fee_group() -> fee::Group {
        fee::Group {
            event: fee::Event {
                event_id: "event".into(),
                fee_id: "fee".into(),
                source: Identity {
                    id: "id".into(),
                    revision: "r".into(),
                },
                review: Identity {
                    id: "review".into(),
                    revision: "r".into(),
                },
                ordinal: 730000,
                sequence: 0,
                action: fee::Action::Earn,
                reason: fee::Reason::Service,
                amount: 0.,
            },
            policy: Identity {
                id: "policy".into(),
                revision: "r".into(),
            },
            postings: vec![],
            next: fee::State::default(),
        }
    }
    fn layout_matches<T: Typed>(value: &T) {
        let actual = serde_json::to_vec(value).unwrap();
        let from_layout = serde_json::to_vec(&value.layout_value().unwrap()).unwrap();
        // Native structs emit declaration order; Value maps sort keys. Compare
        // exact canonical Values so number categories and signed zero survive.
        let native: serde_json::Value = serde_json::from_slice(&actual).unwrap();
        assert_eq!(serde_json::to_vec(&native).unwrap(), from_layout);
    }
    #[test]
    fn typed_shape_covers_serde_and_charges_omitted_field_discriminants() {
        cut_shape::with_budget(100_000, || {
            {
                let loan = group();
                let before = cut_shape::spent().unwrap();
                let s = commercial_loan_prefix_shape(&loan)?;
                assert_eq!(cut_shape::spent().unwrap() - before, s.census_work_upper()?);
                let expected = cut_shape::shape(&loan)?;
                assert!(s.encoded_bytes_upper >= expected.encoded_bytes_upper);
                assert_eq!(add(s.cells, s.emitted_fields)?, expected.cells_upper);
            }
            let fee = fee_group();
            let s = commercial_fee_prefix_shape(&fee)?;
            let expected = cut_shape::shape(&fee)?;
            assert!(s.encoded_bytes_upper >= expected.encoded_bytes_upper);
            assert_eq!(add(s.cells, s.emitted_fields)?, expected.cells_upper);
            assert!(s.declared_field_visits > s.emitted_fields);
            Ok(())
        })
        .unwrap();
    }
    #[test]
    fn signed_zero_adjacent_clock_and_gross_state_are_exact() {
        cut_shape::with_budget(100_000, || {
            let a = group();
            let mut b = a.clone();
            b.cash = -0.;
            let mut c = b.clone();
            c.cash = 0.;
            assert!(!commercial_loan_prefix_equal(&b, &c)?);
            b = a.clone();
            b.year = f64::from_bits(1);
            assert!(!commercial_loan_prefix_equal(&a, &b)?);
            b = a.clone();
            b.next.legal_interest = 1.;
            assert!(!commercial_loan_prefix_equal(&a, &b)?);
            assert!(commercial_loan_prefix_equal(&a, &a)?);
            let f = fee_group();
            let mut g = f.clone();
            g.next.acquired_fee_asset = -0.;
            // The historical serializer omits both zero signs in this field.
            assert!(commercial_fee_prefix_equal(&f, &g)?);
            g.event.amount = -0.;
            assert!(!commercial_fee_prefix_equal(&f, &g)?);
            Ok(())
        })
        .unwrap();
    }
    #[test]
    fn escape_numeric_overflow_and_ticket_refuse_before_unreserved_scan() {
        assert!(commercial_loan_prefix_shape(&group()).is_err());
        assert!(cut_shape::with_budget(1, || commercial_loan_prefix_shape(&group())).is_err());
        cut_shape::with_budget(100_000, || {
            let mut a = group();
            a.policy.id = "\"\\\n\u{0001}\u{975e}".into();
            let s = commercial_loan_prefix_shape(&a)?;
            let expected = cut_shape::shape(&a)?;
            assert!(s.encoded_bytes_upper >= expected.encoded_bytes_upper);
            a.cash = f64::NAN;
            assert!(commercial_loan_prefix_shape(&a).is_err());
            Ok(())
        })
        .unwrap();
        let huge = TypedPrefixShape {
            cells: usize::MAX,
            ..TypedPrefixShape::default()
        };
        assert!(huge.comparison_work_upper(huge).is_err());
    }
    #[test]
    fn comparison_actual_ticket_is_bounded_by_complete_independent_shape() {
        cut_shape::with_budget(100_000, || {
            let a = group();
            let mut b = a.clone();
            b.next.principal = 99.;
            let sa = commercial_loan_prefix_shape(&a)?;
            let sb = commercial_loan_prefix_shape(&b)?;
            let before = cut_shape::spent().unwrap();
            assert!(!commercial_loan_prefix_equal(&a, &b)?);
            assert!(cut_shape::spent().unwrap() - before <= sa.comparison_work_upper(sb)?);
            Ok(())
        })
        .unwrap();
    }
    #[test]
    fn complete_optional_lot_canonical_and_enum_layouts_match_real_serde() {
        let mut g = group();
        g.source_event_id = Some("source".into());
        g.next.pending_coupon_obligation_indices = vec![0, 2];
        g.next.canonical_legal_coupons = vec![CanonicalCouponLegalBalance {
            coupon_id: "coupon".into(),
            native_audit_index: 2,
            native_cash_index: 3,
            opening_entitlement_principal: 100.,
            cumulative_factor: 0.25,
            contractual_earned: 25.,
            actual_interest_collected: 2.,
            legal_remaining: 23.,
        }];
        g.next.lots = vec![CouponLot {
            coupon_id: "coupon".into(),
            source_event_id: "event".into(),
            source_id: "source".into(),
            source_revision: "rev".into(),
            native_audit_index: 2,
            native_cash_index: 3,
            earning_origin_index: 1,
            contractual_earned: 25.,
            legal_cash_collected: 2.,
            legal_remaining: 23.,
            recognized_earned: 25.,
            recognized_cash_applied: 2.,
            recognized_remaining: 23.,
            default_claim: true,
        }];
        g.postings = vec![
            AccountCode::Cash,
            AccountCode::LivePrincipal,
            AccountCode::DefaultPrincipal,
            AccountCode::LiveAccruedInterest,
            AccountCode::DefaultAccruedInterest,
            AccountCode::LiveBookAdjustment,
            AccountCode::DefaultBookAdjustment,
            AccountCode::Allowance,
            AccountCode::InterestIncome,
            AccountCode::CreditLossExpense,
            AccountCode::BasisRecognitionExpense,
            AccountCode::RecoveryIncome,
        ]
        .into_iter()
        .map(|account| OwnedPosting {
            coupon_id: Some("coupon".into()),
            earning_origin_index: Some(1),
            posting: PostingIntent {
                account,
                amount: -0.,
            },
        })
        .collect();
        layout_matches(&g);
        let mut f = fee_group();
        f.next = fee::State {
            acquired_fee_asset_valuation_adjustment: 1.,
            acquired_fee_liability_valuation_adjustment: -1.,
            transferred_net_stock: 2.,
            acquired_stock_selected: true,
            acquired_cost: 3.,
            released_acquired_cost: 4.,
            acquired_fee_asset: 5.,
            acquired_fee_liability: -5.,
            earned: 6.,
            refunded_earned: 7.,
            cancelled_unearned: 8.,
            waived_earned: 9.,
            recovered_waived: 10.,
            net_service_cash: 11.,
            fee_receivable: 12.,
            fee_payable: -12.,
            deferred_fee_income: -13.,
            prepaid_fee_expense: 13.,
            unearned_entitlement: 14.,
        };
        f.postings = vec![
            fee::Account::Cash,
            fee::Account::FeeReceivable,
            fee::Account::FeePayable,
            fee::Account::DeferredFeeIncome,
            fee::Account::PrepaidFeeExpense,
            fee::Account::FeeIncome,
            fee::Account::FeeWriteoffExpense,
            fee::Account::FeeRecoveryIncome,
            fee::Account::AcquiredFeeAsset,
            fee::Account::AcquiredFeeLiability,
            fee::Account::AcquiredFeeAssetValuationAdjustment,
            fee::Account::AcquiredFeeLiabilityValuationAdjustment,
            fee::Account::FeeAcquisitionExpense,
        ]
        .into_iter()
        .map(|account| fee::Posting {
            account,
            amount: -0.,
        })
        .collect();
        for action in [
            fee::Action::Acquire,
            fee::Action::AcquireTransferredStock,
            fee::Action::ReleaseAcquiredCost,
            fee::Action::Earn,
            fee::Action::Cash,
            fee::Action::RefundPrepaid,
            fee::Action::RefundEarned,
            fee::Action::WaiveUnpaidEarned,
            fee::Action::CancelUnearned,
            fee::Action::RecoverWaived,
        ] {
            for reason in [
                fee::Reason::Service,
                fee::Reason::Settlement,
                fee::Reason::Default,
                fee::Reason::Exercise,
                fee::Reason::Maturity,
                fee::Reason::OtherReviewed,
            ] {
                f.event.action = action;
                f.event.reason = reason;
                layout_matches(&f);
            }
        }
        cut_shape::with_budget(1_000_000, || {
            let s = commercial_loan_prefix_shape(&g)?;
            assert!(s.encoded_bytes_upper >= cut_shape::shape(&g)?.encoded_bytes_upper);
            let s = commercial_fee_prefix_shape(&f)?;
            assert!(s.encoded_bytes_upper >= cut_shape::shape(&f)?.encoded_bytes_upper);
            Ok(())
        })
        .unwrap();
        g.next.reviewed_credit_state = Some(book::reviews::ReviewedSnapshot {
            legal_principal: 100.,
            model_gross_carry: 100.,
            allowance: 0.,
            claims: vec![],
            legal_waivers: vec![],
        });
        assert!(
            cut_shape::with_budget(100_000, || commercial_loan_prefix_shape(&g))
                .unwrap_err()
                .contains("not yet independently mapped")
        );
    }
    #[test]
    fn typed_helpers_honor_cancel_after_scope_entry_and_restore_ticket() {
        let control = portfolio_compute_control::Control::new(10_000).unwrap();
        let result = portfolio_compute_control::scope(Some(control.clone()), || {
            cut_shape::with_budget(100_000, || {
                commercial_loan_prefix_shape(&group())?;
                control.cancel();
                commercial_loan_prefix_shape(&group())
            })
        });
        assert_eq!(result.unwrap_err(), "COMPUTE_CANCELLED");
        assert!(cut_shape::spent().is_none());
        assert!(portfolio_compute_control::checkpoint().is_ok());
    }
}
