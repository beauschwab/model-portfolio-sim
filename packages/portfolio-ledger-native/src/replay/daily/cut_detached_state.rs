//! Exhaustive movement of actual native state across source-owner lifetimes.
//! No Serialize, Deserialize, Clone, public fields or caller state constructor.
use super::*;

macro_rules! owned_state {
    ($($name:ident : $kind:ty),+ $(,)?) => {
        pub(super) struct DetachedState { $( $name: $kind, )+ }
        impl DetachedState {
            pub(super) fn detach(sim: Sim<'_>) -> R<Self> {
                if sim.streamed_flows.is_some() || sim.dated.is_some()
                    || sim.consumers.is_some() || sim.mortgages.is_some()
                    || sim.deposit_revisions.is_some() || sim.source_principals.is_some()
                    || sim.derivatives.is_some() || sim.lifecycle.is_some()
                    || sim.dated_state.is_some() || sim.consumer_state.is_some()
                    || sim.mortgage_state.is_some() || sim.derivative_state.is_some()
                    || sim.lifecycle_state.is_some() || sim.commercials.is_none()
                    || sim.commercial_state.is_none()
                { return Err("sequential cut requires exclusive native commercial ownership".into()); }
                if !sim.netting.is_empty() || !sim.claims.is_empty()
                    || sim.claims_by_position.iter().any(|v| !v.is_empty())
                    || !sim.claim_maturities.is_empty()
                    || sim.opening_links.iter().any(|v| !v.is_empty())
                    || !sim.recovery_claims.is_empty() || !sim.recoveries.is_empty()
                    || !sim.intercompany.is_empty() || !sim.flows.is_empty()
                    || !sim.scheduled_ids.is_empty() || !sim.source_portfolio_cursors.is_empty()
                    || !sim.margin_targets.is_empty() || !sim.pending.is_empty()
                    || !sim.used.is_empty() || !sim.scheduled.is_empty()
                { return Err("sequential cut retained a generic overlay outside its capacity inventory".into()); }
                // No `..`: every new Sim field requires an ownership review.
                let Sim { spec: _, sink: _, streamed_flows: _, dated: _,
                    consumers: _, mortgages: _, deposit_revisions: _, source_principals: _,
                    derivatives: _, lifecycle: _, commercials: _, $($name,)+ } = sim;
                Ok(Self { $($name,)+ })
            }
            pub(super) fn attach<'a>(self, spec: &'a Spec,
                source: &'a Compiled<'a>, sink: &'a mut Sink) -> Sim<'a>
            {
                let Self { $($name,)+ } = self;
                Sim { spec, sink, streamed_flows: None, dated: None,
                    consumers: None, mortgages: None, deposit_revisions: None, source_principals: None,
                    derivatives: None, lifecycle: None, commercials: Some(source), $($name,)+ }
            }
        }
    };
}

owned_state! {
    scenario: Scenario,
    severity: f64,
    detail: bool,
    accounts: Vec<Account>,
    positions: Vec<Position>,
    netting: Vec<NettingSet>,
    aid: HashMap<String, usize>,
    pid: HashMap<String, usize>,
    market: Vec<f64>,
    marks: Vec<f64>,
    accrued: Vec<f64>,
    commitments: Vec<f64>,
    schedule_balance: Vec<f64>,
    aoci: Vec<f64>,
    hedge_aoci: Vec<f64>,
    extra_assets: Vec<f64>,
    extra_debt: Vec<f64>,
    funding_interest: Vec<f64>,
    restricted: Vec<f64>,
    opening_equity: Vec<f64>,
    minimum_cash: Vec<f64>,
    htm_sold: Vec<f64>,
    gl: HashMap<Key, f64>,
    earnings: HashMap<Key, f64>,
    sequence: usize,
    aggregate: Vec<(usize, String, [f64; 3])>,
    aggregate_index: HashMap<(usize, String), usize>,
    claims: Vec<Claim>,
    claims_by_position: Vec<Vec<usize>>,
    claim_maturities: HashMap<usize, Vec<usize>>,
    opening_links: Vec<Vec<usize>>,
    recovery_claims: HashMap<(usize, String), f64>,
    recoveries: HashMap<usize, Vec<(usize, String, f64)>>,
    intercompany: HashMap<Key, f64>,
    flows: HashMap<usize, Vec<Cashflow>>,
    scheduled_ids: HashSet<String>,
    dated_state: Option<crate::replay::dated::DatedRunState>,
    consumer_state: Option<crate::owners::consumer::Runtime>,
    mortgage_state: Option<crate::owners::mortgages::pending::Runtime>,
    source_atom_cursor: usize,
    source_portfolio_cursors: Vec<usize>,
    derivative_state: Option<crate::owners::derivatives::Runtime>,
    lifecycle_state: Option<crate::owners::lifecycle::Runtime>,
    commercial_state: Option<crate::owners::commercial::intents::Runtime>,
    margin_targets: HashMap<usize, Vec<(usize, f64)>>,
    pending: HashMap<usize, Vec<usize>>,
    used: Vec<f64>,
    scheduled: HashSet<usize>,
    first: HashMap<(usize, String), usize>,
    max_error: f64,
    htm_opening: Vec<f64>,
    htm_nonexception: Vec<f64>,
    htm_reclassified: Vec<Option<usize>>,
    liquidity_worst: Vec<[usize; 2]>,
}
