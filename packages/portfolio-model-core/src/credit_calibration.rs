//! Exact-observation CTMC hazard MLE. Interval-censored bank snapshots are unsupported.
use crate::cashflow::validate_ordinal;
use crate::credit::{CreditState, MAX_ROW_HAZARD};
use crate::limits::MAX_DATED_METADATA_BYTES;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

pub const SCHEMA: &str = "observed-credit-calibration-1";
pub const MAX_SPELLS: usize = 200_000;
pub const MAX_ACCOUNTS: usize = 100_000;
const SECONDS_PER_YEAR: f64 = 365.0 * 86_400.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ObservationTime {
    pub ordinal: i32,
    pub seconds_of_day: u32,
}

impl ObservationTime {
    fn seconds(self) -> Result<i64, String> {
        validate_ordinal(self.ordinal)?;
        if self.seconds_of_day >= 86_400 {
            return Err("seconds_of_day must be in 0..86399".into());
        }
        Ok(i64::from(self.ordinal) * 86_400 + i64::from(self.seconds_of_day))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ObservationMode {
    ExactTransitionTimes,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DataKind {
    Observed,
    Synthetic,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum SpellEnding {
    Transition { destination: CreditState },
    RightCensored {},
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ObservedSpell {
    pub spell_id: String,
    pub account_id: String,
    pub start: ObservationTime,
    pub end: ObservationTime,
    pub origin: CreditState,
    pub ending: SpellEnding,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum UnobservedRowPolicy {
    Reject {},
    /// Used only if this origin has no training exposure; never mixed into observed MLEs.
    SuppliedRow {
        transition_hazards_per_year: [f64; 5],
        assumption_id: String,
        rationale: String,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CreditCalibrationRequest {
    pub schema: String,
    pub source_id: String,
    pub source_revision: String,
    pub population_id: String,
    pub model_policy_id: String,
    pub training_split_id: String,
    pub holdout_split_id: String,
    pub observation_mode: ObservationMode,
    pub data_kind: DataKind,
    pub train_start: ObservationTime,
    pub train_end: ObservationTime,
    pub holdout_end: ObservationTime,
    pub unobserved_row_policy: [UnobservedRowPolicy; 3],
    pub spells: Vec<ObservedSpell>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum RowEstimateOrigin {
    ObservedMle {},
    ObservedNoEvents {},
    SuppliedAssumption {
        assumption_id: String,
        rationale: String,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WindowDiagnostics {
    pub exposure_years_by_origin: [f64; 3],
    pub event_counts: [[u64; 5]; 3],
    /// Compensator h_ij * observed live-origin exposure, not horizon event probabilities.
    pub expected_event_counts: [[f64; 5]; 3],
    /// Complete path log density in annual time units, excluding initial-state density.
    /// None means at least one observed transition has zero fitted intensity.
    pub log_likelihood: Option<f64>,
    pub impossible_event_count: u64,
    pub right_censored_spell_count: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreditCalibrationResult {
    pub schema: String,
    pub source_id: String,
    pub source_revision: String,
    pub population_id: String,
    pub model_policy_id: String,
    pub training_split_id: String,
    pub holdout_split_id: String,
    pub observation_mode: ObservationMode,
    pub data_kind: DataKind,
    pub train_start: ObservationTime,
    pub train_end: ObservationTime,
    pub holdout_end: ObservationTime,
    pub account_count: usize,
    pub spell_count: usize,
    pub transition_hazards_per_year: [[f64; 5]; 3],
    pub estimate_origins: [RowEstimateOrigin; 3],
    pub training: WindowDiagnostics,
    pub holdout: WindowDiagnostics,
}

fn state_index(state: CreditState) -> usize {
    match state {
        CreditState::Performing => 0,
        CreditState::Watch => 1,
        CreditState::Delinquent => 2,
        CreditState::Default => 3,
        CreditState::Prepaid => 4,
    }
}

fn text_field(value: &str, max: usize, bytes: &mut usize) -> Result<(), String> {
    if value.trim().is_empty() || value.len() > max || value.chars().any(char::is_control) {
        return Err("calibration identities must be nonempty bounded text without controls".into());
    }
    *bytes += value.len();
    if *bytes > MAX_DATED_METADATA_BYTES {
        return Err("calibration metadata budget exceeded".into());
    }
    Ok(())
}

fn validate_hazard_row(row: &[f64; 5], origin: usize) -> Result<(), String> {
    if row.iter().any(|v| !v.is_finite() || *v < 0.0)
        || row[origin] != 0.0
        || row.iter().sum::<f64>() > MAX_ROW_HAZARD
    {
        return Err(format!("hazard row requires zero diagonal, finite nonnegative rates and total <={MAX_ROW_HAZARD}/year"));
    }
    Ok(())
}

#[derive(Default)]
struct Evidence {
    exposure_seconds: [u64; 3],
    event_counts: [[u64; 5]; 3],
    right_censored_spell_count: u64,
}

impl Evidence {
    fn record(&mut self, spell: &ObservedSpell, start: i64, end: i64) -> Result<(), String> {
        let spell_start = spell.start.seconds()?;
        let spell_end = spell.end.seconds()?;
        let origin = state_index(spell.origin);
        let exposure = (spell_end.min(end) - spell_start.max(start)).max(0) as u64;
        self.exposure_seconds[origin] += exposure;
        // Windows have left-open/right-closed event endpoints; exposure is elapsed time.
        if spell_end > start && spell_end <= end {
            match spell.ending {
                SpellEnding::Transition { destination } => {
                    self.event_counts[origin][state_index(destination)] += 1;
                }
                SpellEnding::RightCensored {} => self.right_censored_spell_count += 1,
            }
        }
        Ok(())
    }

    fn diagnostics(&self, hazards: &[[f64; 5]; 3]) -> WindowDiagnostics {
        let exposure_years_by_origin = self.exposure_seconds.map(|v| v as f64 / SECONDS_PER_YEAR);
        let mut expected_event_counts = [[0.0; 5]; 3];
        let mut log_likelihood = 0.0;
        let mut impossible_event_count = 0;
        for origin in 0..3 {
            for destination in 0..5 {
                let hazard = hazards[origin][destination];
                let count = self.event_counts[origin][destination];
                let expected = hazard * exposure_years_by_origin[origin];
                expected_event_counts[origin][destination] = expected;
                log_likelihood -= expected;
                if count > 0 {
                    if hazard == 0.0 {
                        impossible_event_count += count;
                    } else {
                        log_likelihood += count as f64 * hazard.ln();
                    }
                }
            }
        }
        WindowDiagnostics {
            exposure_years_by_origin,
            event_counts: self.event_counts,
            expected_event_counts,
            log_likelihood: (impossible_event_count == 0).then_some(log_likelihood),
            impossible_event_count,
            right_censored_spell_count: self.right_censored_spell_count,
        }
    }
}

pub fn fit(request: &CreditCalibrationRequest) -> Result<CreditCalibrationResult, String> {
    if request.schema != SCHEMA {
        return Err("unsupported observed credit calibration schema".into());
    }
    if request.spells.is_empty() || request.spells.len() > MAX_SPELLS {
        return Err(format!("calibration requires 1..{MAX_SPELLS} spells"));
    }
    let train_start = request.train_start.seconds()?;
    let train_end = request.train_end.seconds()?;
    let holdout_end = request.holdout_end.seconds()?;
    if train_start >= train_end
        || train_end >= holdout_end
        || (holdout_end - train_start) as f64 > 100.0 * SECONDS_PER_YEAR
    {
        return Err(
            "chronological train/holdout windows must be positive and span <=100 ACT/365 years"
                .into(),
        );
    }
    let mut metadata_bytes = 0;
    for text in [
        &request.source_id,
        &request.source_revision,
        &request.population_id,
        &request.model_policy_id,
        &request.training_split_id,
        &request.holdout_split_id,
    ] {
        text_field(text, 256, &mut metadata_bytes)?;
    }
    if request.training_split_id == request.holdout_split_id {
        return Err("training and holdout split identities must differ".into());
    }
    for (origin, policy) in request.unobserved_row_policy.iter().enumerate() {
        if let UnobservedRowPolicy::SuppliedRow {
            transition_hazards_per_year,
            assumption_id,
            rationale,
        } = policy
        {
            validate_hazard_row(transition_hazards_per_year, origin)?;
            text_field(assumption_id, 256, &mut metadata_bytes)?;
            text_field(rationale, 1024, &mut metadata_bytes)?;
        }
    }
    let mut ids = HashSet::with_capacity(request.spells.len());
    let mut sorted = Vec::with_capacity(request.spells.len());
    for spell in &request.spells {
        text_field(&spell.spell_id, 256, &mut metadata_bytes)?;
        text_field(&spell.account_id, 256, &mut metadata_bytes)?;
        if !ids.insert(&spell.spell_id) {
            return Err("duplicate spell identity".into());
        }
        let start = spell.start.seconds()?;
        let end = spell.end.seconds()?;
        let origin = state_index(spell.origin);
        if start >= end || start < train_start || end > holdout_end || origin >= 3 {
            return Err(
                "spells require positive exact live-state exposure within declared windows".into(),
            );
        }
        if let SpellEnding::Transition { destination } = spell.ending {
            if state_index(destination) == origin {
                return Err("a recorded transition cannot retain the same state".into());
            }
        }
        sorted.push(spell);
    }
    sorted.sort_by(|a, b| {
        a.account_id
            .cmp(&b.account_id)
            .then(a.start.cmp(&b.start))
            .then(a.spell_id.cmp(&b.spell_id))
    });
    let mut accounts = 0;
    for (index, spell) in sorted.iter().enumerate() {
        if index == 0 || spell.account_id != sorted[index - 1].account_id {
            accounts += 1;
            if accounts > MAX_ACCOUNTS {
                return Err("calibration account limit exceeded".into());
            }
        } else {
            let prior = sorted[index - 1];
            if prior.end != spell.start {
                return Err("account paths must be contiguous without gaps or overlaps".into());
            }
            match prior.ending {
                SpellEnding::Transition { destination }
                    if state_index(destination) < 3 && destination == spell.origin => {},
                _ => return Err("account cannot resume after censoring/absorption or contradict transition state".into()),
            }
        }
    }
    let mut training = Evidence::default();
    let mut holdout = Evidence::default();
    for spell in sorted {
        training.record(spell, train_start, train_end)?;
        holdout.record(spell, train_end, holdout_end)?;
    }
    if training.exposure_seconds.iter().sum::<u64>() == 0
        || holdout.exposure_seconds.iter().sum::<u64>() == 0
    {
        return Err(
            "both training and chronological holdout require observed live exposure".into(),
        );
    }
    let mut hazards = [[0.0; 5]; 3];
    let mut origins = [
        RowEstimateOrigin::ObservedMle {},
        RowEstimateOrigin::ObservedMle {},
        RowEstimateOrigin::ObservedMle {},
    ];
    for origin in 0..3 {
        let seconds = training.exposure_seconds[origin];
        if seconds == 0 {
            match &request.unobserved_row_policy[origin] {
                UnobservedRowPolicy::Reject {} => return Err(format!("origin {origin} has no training exposure; explicit supplied assumption required")),
                UnobservedRowPolicy::SuppliedRow { transition_hazards_per_year, assumption_id, rationale } => {
                    hazards[origin] = *transition_hazards_per_year;
                    origins[origin] = RowEstimateOrigin::SuppliedAssumption {
                        assumption_id: assumption_id.clone(), rationale: rationale.clone(),
                    };
                }
            }
        } else {
            let years = seconds as f64 / SECONDS_PER_YEAR;
            for (destination, hazard) in hazards[origin].iter_mut().enumerate() {
                *hazard = training.event_counts[origin][destination] as f64 / years;
            }
            validate_hazard_row(&hazards[origin], origin)?; // Never clamp a fit to production limits.
            if training.event_counts[origin].iter().sum::<u64>() == 0 {
                origins[origin] = RowEstimateOrigin::ObservedNoEvents {};
            }
        }
    }
    Ok(CreditCalibrationResult {
        schema: SCHEMA.into(),
        source_id: request.source_id.clone(),
        source_revision: request.source_revision.clone(),
        population_id: request.population_id.clone(),
        model_policy_id: request.model_policy_id.clone(),
        training_split_id: request.training_split_id.clone(),
        holdout_split_id: request.holdout_split_id.clone(),
        observation_mode: request.observation_mode,
        data_kind: request.data_kind,
        train_start: request.train_start,
        train_end: request.train_end,
        holdout_end: request.holdout_end,
        account_count: accounts,
        spell_count: request.spells.len(),
        transition_hazards_per_year: hazards,
        estimate_origins: origins,
        training: training.diagnostics(&hazards),
        holdout: holdout.diagnostics(&hazards),
    })
}
