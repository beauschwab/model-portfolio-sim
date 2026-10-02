//! Actual-date scheduled recognition and settlement ownership for daily replay.
//! Credit/behavior producers own scenario amounts; this route never overlays PD,
//! deposit flight, survival scaling or synthetic maturity on these positions.

use crate::dated_store::{Boundary, DatedStore, DatedStoreRequest, Direction, StoredEvent};
use crate::types::Spec;
use portfolio_model_core::limits::MAX_MONETARY_AMOUNT;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet, HashMap};

pub const SCHEMA: &str = "dated-ledger-1";
pub const PROTOCOL: u32 = 3;
pub const RIGHTS_GL: [&str; 4] = [
    "dated_interest_receivable", "dated_interest_payable",
    "dated_prepaid_expense", "dated_deferred_income",
];

#[derive(Debug, Clone, Copy, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum FeePolicy { ImmediateEarnings }

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RecognitionPolicy {
    pub policy_id: String,
    pub policy_revision: String,
    /// This schema accepts producer-supplied calendar recognition segments only.
    pub recognition_model: String,
    pub fees: FeePolicy,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct DatedLedgerRequest {
    pub schema: String,
    pub monetary_unit: String,
    pub recognition_policy: RecognitionPolicy,
    pub specification: Value,
    pub dated: DatedStoreRequest,
}

pub struct CompiledDatedLedger {
    pub spec: Spec,
    pub store: DatedStore,
    pub monetary_unit: String,
    pub recognition_policy: RecognitionPolicy,
    pub owner_positions: Vec<usize>,
    pub position_owners: Vec<Option<usize>>,
    runs: HashMap<(String, u64), usize>,
    pub work_units: usize,
}

fn valid_text(s: &str) -> bool {
    !s.trim().is_empty() && s.len() <= 256 && !s.chars().any(char::is_control)
}
fn asset_kind(kind: &str) -> bool { matches!(kind, "loan" | "security" | "reverse_repo") }
fn near(a: f64, b: f64) -> bool { (a-b).abs() <= 1e-9_f64.max(1e-12*a.abs().max(b.abs())) }

impl CompiledDatedLedger {
    pub fn compile(request: DatedLedgerRequest, max_positions: usize, max_work: usize) -> Result<Self, String> {
        if request.schema != SCHEMA || request.monetary_unit != "currency_units"
            || !valid_text(&request.recognition_policy.policy_id)
            || !valid_text(&request.recognition_policy.policy_revision)
            || request.recognition_policy.recognition_model != "supplied_calendar_segments-1" {
            return Err("dated ledger requires its explicit schema, currency units and recognition policy".into());
        }
        let spec = crate::spec::normalize(request.specification, max_positions, max_work)?;
        let store = DatedStore::build(request.dated)?;
        if store.reporting_horizon_days() as usize != spec.horizon_days || store.owners().is_empty() {
            return Err("dated reporting horizon must equal ledger horizon and declare owners".into());
        }
        let mut expected = BTreeSet::from([("baseline".to_owned(), 1.0_f64.to_bits())]);
        for scenario in &spec.scenarios {
            expected.insert((scenario.name.clone(), 1.0_f64.to_bits()));
            for &severity in &spec.reverse_severities {
                // Positive and negative zero are the same economic severity.
                expected.insert((scenario.name.clone(), if severity == 0.0 {0.0_f64.to_bits()} else {severity.to_bits()}));
            }
        }
        let mut runs = HashMap::new();
        for (i, run) in store.runs().iter().enumerate() {
            let bits = if run.severity == 0.0 {0.0_f64.to_bits()} else {run.severity.to_bits()};
            if runs.insert((run.scenario_id.to_string(), bits), i).is_some() {
                return Err("duplicate economic dated execution run".into());
            }
        }
        if runs.keys().cloned().collect::<BTreeSet<_>>() != expected {
            return Err("dated inputs must cover exactly baseline, scenarios and reverse severities".into());
        }
        let positions: HashMap<_,_> = spec.positions.iter().enumerate().map(|(i,p)|(p.id.as_str(),i)).collect();
        let accounts: HashMap<_,_> = spec.accounts.iter().map(|a|(a.id.as_str(),a)).collect();
        let mut owner_positions = Vec::with_capacity(store.owners().len());
        let mut position_owners = vec![None; spec.positions.len()];
        for (oi, owner) in store.owners().iter().enumerate() {
            let &i = positions.get(owner.position_id.as_ref()).ok_or("dated owner names unknown ledger position")?;
            let p = &spec.positions[i];
            if p.account.as_str() != owner.account_id.as_ref()
                || accounts[p.account.as_str()].currency != owner.currency.as_ref()
                || (owner.direction == Direction::Receipt) != asset_kind(p.kind.as_str()) {
                return Err("dated owner account/currency/direction mismatch".into());
            }
            if p.start_day != 0 || p.allowance != 0. || p.commitment != 0. || p.annual_pd != 0.
                || p.migration_rate != 0. || p.draw_fraction != 0. || p.monthly_runoff != 0.
                || p.floating_beta != 0. || p.encumbered_fraction != 0.
                || !p.collateral_position.is_empty() || p.eligible_fraction != 0.
                || matches!(p.classification.as_str(), "afs" | "trading") {
                return Err("dated schedule owner needs explicit state/credit/mark/collateral integration for these legacy fields".into());
            }
            if spec.cashflows.iter().any(|f| f.position == p.id)
                || spec.policies.iter().any(|x| x.position == p.id.as_str())
                || spec.positions.iter().any(|x| x.collateral_position == p.id.as_str()) {
                return Err("dated ownership overlaps legacy cashflow, sale or collateral policy".into());
            }
            owner_positions.push(i); position_owners[i] = Some(oi);
        }
        let iterations = 1 + spec.scenarios.len() * (1 + spec.reverse_severities.len());
        let mut work_units = spec.positions.len().checked_mul(spec.horizon_days)
            .and_then(|n|n.checked_mul(iterations)).ok_or("dated work overflow")?;
        for event in store.events() {
            let repetitions = if event.run_index.is_none() {iterations} else {
                let run = &store.runs()[event.run_index.unwrap()];
                usize::from(run.scenario_id.as_ref() == "baseline" || run.severity == 1.)
                    + usize::from(run.scenario_id.as_ref() != "baseline" && spec.reverse_severities.contains(&run.severity))
            };
            let mut event_work = 2usize; // opening and closing rights
            for segment in &store.segments()[event.recognition_range.0..event.recognition_range.1] {
                let days = (segment.end_ordinal.min(store.reporting_end_ordinal())
                    - segment.start_ordinal.max(store.as_of_ordinal())).max(0) as usize;
                event_work = event_work.checked_add(days).ok_or("dated segment work overflow")?;
            }
            work_units = work_units.checked_add(event_work.checked_mul(repetitions).ok_or("dated run work overflow")?)
                .ok_or("dated total work overflow")?;
        }
        if work_units > max_work { return Err("combined dated position/recognition work exceeds admission".into()); }
        let compiled = Self { spec, store, monetary_unit: request.monetary_unit,
            recognition_policy: request.recognition_policy, owner_positions, position_owners, runs, work_units };
        for ri in 0..compiled.store.runs().len() {
            portfolio_compute_control::checkpoint()?;
            let state = DatedRunState::new(&compiled, ri)?;
            let mut totals = vec![0.; compiled.owner_positions.len()];
            for event in compiled.store.events().iter().filter(|e|e.run_index.is_none_or(|r|r==ri)) {
                totals[event.owner_index] += event.components.principal.abs();
                finite(totals[event.owner_index])?;
            }
            for (oi,&pi) in compiled.owner_positions.iter().enumerate() {
                let p=&compiled.spec.positions[pi];
                let signed=if asset_kind(p.kind.as_str()) {1.} else {-1.};
                if totals[oi] > p.balance && !near(totals[oi],p.balance) {
                    return Err("dated principal settlements exceed opening face; draws need explicit state events".into());
                }
                if !near(state.rights[oi].iter().sum(), signed*p.opening_accrued) {
                    return Err("opening dated earned rights do not reconcile supplied accrued anchor".into());
                }
            }
        }
        Ok(compiled)
    }
    pub fn run_index(&self, scenario: &str, severity: f64) -> Result<usize,String> {
        let bits=if severity==0. {0.0_f64.to_bits()} else {severity.to_bits()};
        self.runs.get(&(scenario.to_owned(),bits)).copied().ok_or("missing dated execution run")
    }
}

fn finite(value:f64)->Result<(),String>{
    if !value.is_finite() || value.abs()>MAX_MONETARY_AMOUNT {Err("dated ledger monetary admission exceeded".into())}else{Ok(())}
}
pub fn classify_right(value:f64,settled:bool)->[f64;4] {
    if settled { [0.,0.,value.max(0.),value.min(0.)] }
    else {[value.max(0.),value.min(0.),0.,0.]}
}

#[derive(Clone, Copy, Default)]
pub struct EventState { pub earned:f64, pub book:f64, pub settled:bool }
pub struct DatedRunState {
    pub run_index:usize,
    pub events:Vec<EventState>,
    /// Gross rights by owner: positive receivable/prepaid, negative payable/deferred.
    pub rights:Vec<[f64;4]>,
    active:BTreeSet<(usize,usize)>,
    pub day:u32,
}
impl DatedRunState {
    pub fn new(compiled:&CompiledDatedLedger,run_index:usize)->Result<Self,String>{
        let store=&compiled.store; let run=&store.runs()[run_index];
        let mut state=Self{run_index,events:vec![EventState::default();store.events().len()],
            rights:vec![[0.;4];store.owners().len()],active:BTreeSet::new(),day:0};
        for event in store.events().iter().filter(|e|e.run_index.is_none_or(|r|r==run_index)) {
            let rights=store.rights_at(&run.run_id,event.index,store.as_of_ordinal(),Boundary::BeforeEvents)?;
            state.events[event.index]=EventState{earned:rights.earned_interest_to_date,
                book:rights.book_amortization_to_date,settled:false};
            let values=classify_right(rights.signed_interest_right,false);
            for (total,value) in state.rights[event.owner_index].iter_mut().zip(values){*total+=value;finite(*total)?;}
            for si in event.recognition_range.0..event.recognition_range.1 {
                let segment=&store.segments()[si];
                if segment.start_ordinal<=store.as_of_ordinal() && segment.end_ordinal>store.as_of_ordinal(){state.active.insert((event.index,si));}
            }
        }
        Ok(state)
    }
    pub fn replace_event(&mut self,event:&StoredEvent,new:EventState)->Result<[f64;4],String>{
        let old=self.events[event.index];
        let value=|s:EventState|classify_right(s.earned-if s.settled{event.components.cash_interest}else{0.},s.settled);
        let before=value(old);let after=value(new);let mut delta=[0.;4];
        for k in 0..4 {delta[k]=after[k]-before[k];self.rights[event.owner_index][k]+=delta[k];finite(self.rights[event.owner_index][k])?;}
        finite(new.earned)?;finite(new.book)?;self.events[event.index]=new;Ok(delta)
    }
    /// Advance active supplied segments in stable source event order; no scan of all events per day.
    pub fn recognition(&mut self,store:&DatedStore,day:u32)->Result<Vec<(usize,f64,f64)>,String>{
        if day!=self.day+1 || day>store.reporting_horizon_days(){return Err("dated recognition cursor must advance one admitted day".into());}
        let start=store.as_of_ordinal()+day as i32-1;let end=start+1;
        let mut deltas=BTreeMap::<usize,(f64,f64)>::new();
        for &(ei,si) in &self.active {
            let s=&store.segments()[si];
            let fraction=|d:i32|f64::from(d-s.start_ordinal)/f64::from(s.end_ordinal-s.start_ordinal);
            let value=deltas.entry(ei).or_default();
            value.0+=s.accrued_interest*fraction(end)-s.accrued_interest*fraction(start);
            value.1+=s.book_amortization*fraction(end)-s.book_amortization*fraction(start);
        }
        let run=&store.runs()[self.run_index];
        for boundary in store.accrual_boundaries_on(&run.run_id,end)? {
            match boundary.kind {
                crate::dated_store::AccrualBoundaryKind::Start=>{self.active.insert((boundary.event_index,boundary.segment_index));},
                crate::dated_store::AccrualBoundaryKind::End=>{self.active.remove(&(boundary.event_index,boundary.segment_index));},
            }
        }
        self.day=day;
        Ok(deltas.into_iter().map(|(i,(a,b))|(i,a,b)).collect())
    }
}
