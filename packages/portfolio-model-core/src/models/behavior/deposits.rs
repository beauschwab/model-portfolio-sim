//! Exact-observation deposit retention, closure and migration calibration.
//!
//! This is a behavioral evidence stage. It estimates cause-specific physical
//! intensities from known segment spells and never replaces the supplied daily
//! deposit policy or infers paid-rate response from balances alone.

use crate::cashflows::dated::CashflowMeasure;
use crate::models::credit::transition::MAX_ROW_HAZARD;
use crate::models::observation::SECONDS_PER_YEAR;
pub use crate::models::observation::{DataKind, ObservationMode, ObservationTime};
use crate::validation::limits::MAX_DATED_METADATA_BYTES;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};

pub const SCHEMA: &str = "observed-deposit-calibration-1";
pub const MAX_SPELLS: usize = 200_000;
pub const MAX_SEGMENTS: usize = 32;
pub const MAX_ACCOUNTS: usize = 100_000;

fn seconds(time: ObservationTime) -> Result<i64, String> {
    time.seconds()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SamplingUnit {
    /// Withdrawal is a complete account exit. Partial withdrawals are unsupported.
    AccountFullExit,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum EstimateOrigin {
    ObservedMle {},
    ObservedNoEvents {},
    SuppliedAssumption {
        assumption_id: String,
        rationale: String,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum SpellEnding {
    Migration { destination: String },
    Withdrawal {},
    Closure {},
    RightCensored {},
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ObservedSpell {
    pub spell_id: String,
    pub account_id: String,
    pub segment_id: String,
    pub start: ObservationTime,
    pub end: ObservationTime,
    pub ending: SpellEnding,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum UnobservedRowPolicy {
    Reject {},
    SuppliedRow {
        withdrawal_per_year: f64,
        closure_per_year: f64,
        migration_per_year: Vec<f64>,
        assumption_id: String,
        rationale: String,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Request {
    pub schema: String,
    pub source_id: String,
    pub source_revision: String,
    pub population_id: String,
    pub model_policy_id: String,
    pub training_split_id: String,
    pub holdout_split_id: String,
    pub observation_mode: ObservationMode,
    pub sampling_unit: SamplingUnit,
    pub data_kind: DataKind,
    pub train_start: ObservationTime,
    pub train_end: ObservationTime,
    pub holdout_end: ObservationTime,
    pub segment_ids: Vec<String>,
    pub unobserved_row_policy: Vec<UnobservedRowPolicy>,
    pub spells: Vec<ObservedSpell>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SegmentHazard {
    pub segment_id: String,
    pub withdrawal_per_year: f64,
    pub closure_per_year: f64,
    pub migration_per_year: Vec<f64>,
    /// `None` means zero observed events or a supplied row, not zero risk.
    pub standard_error_withdrawal: Option<f64>,
    pub standard_error_closure: Option<f64>,
    pub standard_error_migration: Vec<Option<f64>>,
    pub estimate_origin: EstimateOrigin,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WindowDiagnostics {
    pub exposure_years_by_segment: Vec<f64>,
    pub migration_counts: Vec<Vec<u64>>,
    pub withdrawal_counts: Vec<u64>,
    pub closure_counts: Vec<u64>,
    pub expected_migration_counts: Vec<Vec<f64>>,
    pub expected_withdrawal_counts: Vec<f64>,
    pub expected_closure_counts: Vec<f64>,
    pub log_likelihood: Option<f64>,
    pub impossible_event_count: u64,
    pub right_censored_spell_count: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DepositCalibrationResult {
    pub schema: String,
    pub measure: CashflowMeasure,
    pub source_id: String,
    pub source_revision: String,
    pub population_id: String,
    pub model_policy_id: String,
    pub training_split_id: String,
    pub holdout_split_id: String,
    pub observation_mode: ObservationMode,
    pub sampling_unit: SamplingUnit,
    pub data_kind: DataKind,
    pub segment_ids: Vec<String>,
    pub train_start: ObservationTime,
    pub train_end: ObservationTime,
    pub holdout_end: ObservationTime,
    pub account_count: usize,
    pub spell_count: usize,
    pub segment_hazards: Vec<SegmentHazard>,
    pub training: WindowDiagnostics,
    pub holdout: WindowDiagnostics,
    pub adoption_eligible: bool,
}

fn text(value: &str, max: usize, bytes: &mut usize) -> Result<(), String> {
    if value.trim().is_empty() || value.len() > max || value.chars().any(char::is_control) {
        return Err("deposit calibration identities must be bounded control-free text".into());
    }
    *bytes += value.len();
    if *bytes > MAX_DATED_METADATA_BYTES {
        return Err("deposit calibration metadata budget exceeded".into());
    }
    Ok(())
}

fn validate_row(row: &SegmentHazard, index: usize, n: usize) -> Result<(), String> {
    if row.migration_per_year.len() != n
        || row.migration_per_year[index] != 0.0
        || row
            .migration_per_year
            .iter()
            .any(|value| !value.is_finite() || *value < 0.0)
        || !row.withdrawal_per_year.is_finite()
        || !row.closure_per_year.is_finite()
        || row.withdrawal_per_year < 0.0
        || row.closure_per_year < 0.0
        || row.withdrawal_per_year
            + row.closure_per_year
            + row.migration_per_year.iter().sum::<f64>()
            > MAX_ROW_HAZARD
    {
        return Err("deposit hazard row has invalid causes or exceeds total intensity".into());
    }
    Ok(())
}

#[derive(Default)]
struct Evidence {
    exposure_seconds: Vec<u64>,
    migration_counts: Vec<Vec<u64>>,
    withdrawal_counts: Vec<u64>,
    closure_counts: Vec<u64>,
    right_censored_spell_count: u64,
}

impl Evidence {
    fn new(n: usize) -> Self {
        Self {
            exposure_seconds: vec![0; n],
            migration_counts: vec![vec![0; n]; n],
            withdrawal_counts: vec![0; n],
            closure_counts: vec![0; n],
            right_censored_spell_count: 0,
        }
    }

    fn record(
        &mut self,
        spell: &ObservedSpell,
        index: &HashMap<String, usize>,
        start: i64,
        end: i64,
    ) -> Result<(), String> {
        let spell_start = seconds(spell.start)?;
        let spell_end = seconds(spell.end)?;
        let origin = *index
            .get(&spell.segment_id)
            .ok_or("unknown deposit segment")?;
        self.exposure_seconds[origin] +=
            (spell_end.min(end) - spell_start.max(start)).max(0) as u64;
        if spell_end > start && spell_end <= end {
            match &spell.ending {
                SpellEnding::Migration { destination } => {
                    let destination = *index
                        .get(destination)
                        .ok_or("unknown migration destination")?;
                    self.migration_counts[origin][destination] += 1;
                }
                SpellEnding::Withdrawal {} => self.withdrawal_counts[origin] += 1,
                SpellEnding::Closure {} => self.closure_counts[origin] += 1,
                SpellEnding::RightCensored {} => self.right_censored_spell_count += 1,
            }
        }
        Ok(())
    }

    fn diagnostics(&self, hazards: &[SegmentHazard]) -> WindowDiagnostics {
        let exposure = self
            .exposure_seconds
            .iter()
            .map(|value| *value as f64 / SECONDS_PER_YEAR)
            .collect::<Vec<_>>();
        let mut expected_migration =
            vec![vec![0.0; self.exposure_seconds.len()]; self.exposure_seconds.len()];
        let mut expected_withdrawal = vec![0.0; exposure.len()];
        let mut expected_closure = vec![0.0; exposure.len()];
        let mut log_likelihood = 0.0;
        let mut impossible = 0;
        for origin in 0..exposure.len() {
            let row = &hazards[origin];
            let years = exposure[origin];
            expected_withdrawal[origin] = row.withdrawal_per_year * years;
            expected_closure[origin] = row.closure_per_year * years;
            log_likelihood -= (row.withdrawal_per_year
                + row.closure_per_year
                + row.migration_per_year.iter().sum::<f64>())
                * years;
            for (destination, expected_value) in expected_migration[origin].iter_mut().enumerate() {
                let hazard = row.migration_per_year[destination];
                *expected_value = hazard * years;
                let count = self.migration_counts[origin][destination];
                if count > 0 {
                    if hazard == 0.0 {
                        impossible += count;
                    } else {
                        log_likelihood += count as f64 * hazard.ln();
                    }
                }
            }
            for (count, hazard) in [
                (self.withdrawal_counts[origin], row.withdrawal_per_year),
                (self.closure_counts[origin], row.closure_per_year),
            ] {
                if count > 0 {
                    if hazard == 0.0 {
                        impossible += count;
                    } else {
                        log_likelihood += count as f64 * hazard.ln();
                    }
                }
            }
        }
        WindowDiagnostics {
            exposure_years_by_segment: exposure,
            migration_counts: self.migration_counts.clone(),
            withdrawal_counts: self.withdrawal_counts.clone(),
            closure_counts: self.closure_counts.clone(),
            expected_migration_counts: expected_migration,
            expected_withdrawal_counts: expected_withdrawal,
            expected_closure_counts: expected_closure,
            log_likelihood: (impossible == 0).then_some(log_likelihood),
            impossible_event_count: impossible,
            right_censored_spell_count: self.right_censored_spell_count,
        }
    }
}

pub fn fit(request: &Request) -> Result<DepositCalibrationResult, String> {
    fit_with_check(request, || Ok(()))
}

pub fn fit_with_check(
    request: &Request,
    mut check: impl FnMut() -> Result<(), String>,
) -> Result<DepositCalibrationResult, String> {
    check()?;
    if request.schema != SCHEMA {
        return Err("unsupported observed deposit calibration schema".into());
    }
    if request.spells.is_empty() || request.spells.len() > MAX_SPELLS {
        return Err("deposit calibration spell admission exceeded".into());
    }
    if request.segment_ids.is_empty() || request.segment_ids.len() > MAX_SEGMENTS {
        return Err("deposit calibration segment admission exceeded".into());
    }
    if request.unobserved_row_policy.len() != request.segment_ids.len() {
        return Err("deposit unobserved policy must match segment count".into());
    }
    let train_start = seconds(request.train_start)?;
    let train_end = seconds(request.train_end)?;
    let holdout_end = seconds(request.holdout_end)?;
    if train_start >= train_end
        || train_end >= holdout_end
        || (holdout_end - train_start) as f64 > 100.0 * SECONDS_PER_YEAR
    {
        return Err("deposit calibration windows must be chronological and bounded".into());
    }
    let mut bytes = 0;
    for value in [
        &request.source_id,
        &request.source_revision,
        &request.population_id,
        &request.model_policy_id,
        &request.training_split_id,
        &request.holdout_split_id,
    ] {
        text(value, 256, &mut bytes)?;
    }
    if request.training_split_id == request.holdout_split_id {
        return Err("deposit training and holdout identities must differ".into());
    }
    let mut segment_index = HashMap::new();
    for (index, segment) in request.segment_ids.iter().enumerate() {
        text(segment, 256, &mut bytes)?;
        if segment_index.insert(segment.clone(), index).is_some() {
            return Err("deposit segment identities must be unique".into());
        }
    }
    for (index, policy) in request.unobserved_row_policy.iter().enumerate() {
        if let UnobservedRowPolicy::SuppliedRow {
            withdrawal_per_year,
            closure_per_year,
            migration_per_year,
            assumption_id,
            rationale,
        } = policy
        {
            let row = SegmentHazard {
                segment_id: request.segment_ids[index].clone(),
                withdrawal_per_year: *withdrawal_per_year,
                closure_per_year: *closure_per_year,
                migration_per_year: migration_per_year.clone(),
                standard_error_withdrawal: None,
                standard_error_closure: None,
                standard_error_migration: vec![None; request.segment_ids.len()],
                estimate_origin: EstimateOrigin::SuppliedAssumption {
                    assumption_id: assumption_id.clone(),
                    rationale: rationale.clone(),
                },
            };
            validate_row(&row, index, request.segment_ids.len())?;
            text(assumption_id, 256, &mut bytes)?;
            text(rationale, 1024, &mut bytes)?;
        }
    }
    let mut ids = HashSet::new();
    let mut sorted = Vec::with_capacity(request.spells.len());
    for spell in &request.spells {
        check()?;
        text(&spell.spell_id, 256, &mut bytes)?;
        text(&spell.account_id, 256, &mut bytes)?;
        text(&spell.segment_id, 256, &mut bytes)?;
        let start = seconds(spell.start)?;
        let end = seconds(spell.end)?;
        if start < train_start
            || end > holdout_end
            || start >= end
            || !segment_index.contains_key(&spell.segment_id)
        {
            return Err("deposit spells require bounded positive known-segment exposure".into());
        }
        if !ids.insert(&spell.spell_id) {
            return Err("duplicate deposit spell identity".into());
        }
        if let SpellEnding::Migration { destination } = &spell.ending {
            if destination == &spell.segment_id || !segment_index.contains_key(destination) {
                return Err("deposit migration must target a distinct known segment".into());
            }
        }
        sorted.push(spell);
    }
    check()?;
    sorted.sort_by(|left, right| {
        left.account_id
            .cmp(&right.account_id)
            .then(left.start.cmp(&right.start))
            .then(left.spell_id.cmp(&right.spell_id))
    });
    let mut accounts = 0;
    for (index, spell) in sorted.iter().enumerate() {
        check()?;
        if matches!(spell.ending, SpellEnding::Migration { .. })
            && seconds(spell.end)? < holdout_end
            && (index + 1 == sorted.len() || sorted[index + 1].account_id != spell.account_id)
        {
            return Err(
                "deposit migration requires destination continuation through observation end"
                    .into(),
            );
        }
        if index == 0 || spell.account_id != sorted[index - 1].account_id {
            accounts += 1;
            if accounts > MAX_ACCOUNTS {
                return Err("deposit calibration account admission exceeded".into());
            }
        } else {
            let prior = sorted[index - 1];
            if prior.end != spell.start {
                return Err("deposit account spells must be contiguous".into());
            }
            match &prior.ending {
                SpellEnding::Migration { destination } if destination == &spell.segment_id => {}
                _ => return Err("deposit account cannot resume after a terminal spell".into()),
            }
        }
    }
    let mut training = Evidence::new(request.segment_ids.len());
    let mut holdout = Evidence::new(request.segment_ids.len());
    for spell in sorted {
        check()?;
        training.record(spell, &segment_index, train_start, train_end)?;
        holdout.record(spell, &segment_index, train_end, holdout_end)?;
    }
    if training.exposure_seconds.iter().sum::<u64>() == 0
        || holdout.exposure_seconds.iter().sum::<u64>() == 0
    {
        return Err("deposit calibration requires training and holdout exposure".into());
    }
    let n = request.segment_ids.len();
    let mut hazards = Vec::with_capacity(n);
    for origin in 0..n {
        check()?;
        let seconds = training.exposure_seconds[origin];
        if seconds == 0 {
            match &request.unobserved_row_policy[origin] {
                UnobservedRowPolicy::Reject {} => {
                    return Err(format!(
                        "deposit segment {} has no training exposure",
                        request.segment_ids[origin]
                    ));
                }
                UnobservedRowPolicy::SuppliedRow {
                    withdrawal_per_year,
                    closure_per_year,
                    migration_per_year,
                    assumption_id,
                    rationale,
                } => hazards.push(SegmentHazard {
                    segment_id: request.segment_ids[origin].clone(),
                    withdrawal_per_year: *withdrawal_per_year,
                    closure_per_year: *closure_per_year,
                    migration_per_year: migration_per_year.clone(),
                    standard_error_withdrawal: None,
                    standard_error_closure: None,
                    standard_error_migration: vec![None; n],
                    estimate_origin: EstimateOrigin::SuppliedAssumption {
                        assumption_id: assumption_id.clone(),
                        rationale: rationale.clone(),
                    },
                }),
            }
        } else {
            let years = seconds as f64 / SECONDS_PER_YEAR;
            let withdrawal = training.withdrawal_counts[origin] as f64 / years;
            let closure = training.closure_counts[origin] as f64 / years;
            let migration = training.migration_counts[origin]
                .iter()
                .enumerate()
                .map(|(destination, count)| {
                    if destination == origin {
                        0.0
                    } else {
                        *count as f64 / years
                    }
                })
                .collect::<Vec<_>>();
            let row = SegmentHazard {
                segment_id: request.segment_ids[origin].clone(),
                withdrawal_per_year: withdrawal,
                closure_per_year: closure,
                migration_per_year: migration,
                standard_error_withdrawal: (training.withdrawal_counts[origin] > 0)
                    .then(|| (training.withdrawal_counts[origin] as f64).sqrt() / years),
                standard_error_closure: (training.closure_counts[origin] > 0)
                    .then(|| (training.closure_counts[origin] as f64).sqrt() / years),
                standard_error_migration: training.migration_counts[origin]
                    .iter()
                    .enumerate()
                    .map(|(destination, count)| {
                        if destination == origin {
                            Some(0.0)
                        } else {
                            (*count > 0).then(|| (*count as f64).sqrt() / years)
                        }
                    })
                    .collect(),
                estimate_origin: if training.withdrawal_counts[origin]
                    + training.closure_counts[origin]
                    + training.migration_counts[origin].iter().sum::<u64>()
                    == 0
                {
                    EstimateOrigin::ObservedNoEvents {}
                } else {
                    EstimateOrigin::ObservedMle {}
                },
            };
            validate_row(&row, origin, n)?;
            hazards.push(row);
        }
    }
    check()?;
    Ok(DepositCalibrationResult {
        schema: SCHEMA.into(),
        measure: CashflowMeasure::Physical,
        source_id: request.source_id.clone(),
        source_revision: request.source_revision.clone(),
        population_id: request.population_id.clone(),
        model_policy_id: request.model_policy_id.clone(),
        training_split_id: request.training_split_id.clone(),
        holdout_split_id: request.holdout_split_id.clone(),
        observation_mode: request.observation_mode,
        sampling_unit: request.sampling_unit,
        data_kind: request.data_kind,
        segment_ids: request.segment_ids.clone(),
        train_start: request.train_start,
        train_end: request.train_end,
        holdout_end: request.holdout_end,
        account_count: accounts,
        spell_count: request.spells.len(),
        training: training.diagnostics(&hazards),
        holdout: holdout.diagnostics(&hazards),
        segment_hazards: hazards,
        adoption_eligible: false,
    })
}
