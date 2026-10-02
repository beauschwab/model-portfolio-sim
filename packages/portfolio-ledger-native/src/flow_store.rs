//! Bounded native schedule store populated from validated accounting batches.
//! Strings stay in the position dictionary; no full JSON cashflow document exists.
use crate::types::{Cashflow, Position, Spec};
use std::collections::HashSet;

#[derive(Clone, Copy)]
struct Flow {
    position: usize,
    values: [f64; 4],
}
pub struct FlowStore {
    days: Vec<Vec<Flow>>,
    totals: Vec<f64>,
    last: Vec<usize>,
    count: usize,
    capacity_rows: usize,
}
impl FlowStore {
    pub fn new(horizon: usize) -> Result<Self, String> {
        if !(1..=1080).contains(&horizon) {
            return Err("invalid schedule horizon".into());
        }
        Ok(Self {
            days: vec![vec![]; horizon + 1],
            totals: vec![],
            last: vec![],
            count: 0,
            capacity_rows: 0,
        })
    }
    pub fn push(&mut self, position: usize, day: usize, values: [f64; 4]) -> Result<(), String> {
        if position >= 62000
            || day == 0
            || day >= self.days.len()
            || values.iter().any(|v| !v.is_finite() || v.abs() > 1e15)
            || values[0] < 0.
        {
            return Err("invalid streamed cashflow".into());
        }
        if self.last.len() <= position {
            self.last.resize(position + 1, 0);
            self.totals.resize(position + 1, 0.);
        }
        if day <= self.last[position] {
            return Err("duplicate or unordered streamed cashflow".into());
        }
        // Geometric growth avoids quadratic copies in large monthly buckets.
        // Bound allocated capacity, not merely populated rows.
        let bucket = &mut self.days[day];
        if bucket.len() == bucket.capacity() {
            let limit = 128 * 1024 * 1024 / std::mem::size_of::<Flow>();
            let additional = bucket.capacity().max(256).min(limit - self.capacity_rows);
            if additional == 0 {
                return Err("native schedule memory budget exceeded".into());
            }
            let previous = bucket.capacity();
            bucket
                .try_reserve_exact(additional)
                .map_err(|_| "native schedule allocation failed")?;
            self.capacity_rows += bucket.capacity() - previous;
            if self.capacity_rows > limit {
                return Err("native schedule memory budget exceeded".into());
            }
        }
        self.days[day].push(Flow { position, values });
        self.last[position] = day;
        self.totals[position] += values[0];
        self.count += 1;
        Ok(())
    }
    pub fn validate(&self, spec: &Spec) -> Result<(), String> {
        if self.days.len() != spec.horizon_days + 1 || self.last.len() > spec.positions.len() {
            return Err("schedule specification mismatch".into());
        }
        let external: HashSet<_> = spec.cashflows.iter().map(|f| f.position.as_str()).collect();
        for (i, (&last, &total)) in self.last.iter().zip(&self.totals).enumerate() {
            if last == 0 {
                continue;
            }
            let p = &spec.positions[i];
            if external.contains(p.id.as_str()) || p.commitment != 0. || total > p.balance + 1e-7 {
                return Err("streamed schedule overlaps or exceeds principal".into());
            }
        }
        for (day, rows) in self.days.iter().enumerate() {
            if rows
                .iter()
                .any(|f| day < spec.positions[f.position].start_day)
            {
                return Err("cashflow precedes origination".into());
            }
        }
        Ok(())
    }
    pub fn scheduled<'a>(&'a self, positions: &'a [Position]) -> impl Iterator<Item = String> + 'a {
        self.last
            .iter()
            .enumerate()
            .filter(|(_, d)| **d > 0)
            .map(|(i, _)| positions[i].id.to_string())
    }
    pub fn day<'a>(
        &'a self,
        day: usize,
        positions: &'a [Position],
    ) -> impl Iterator<Item = Cashflow> + 'a {
        self.days[day].iter().map(move |f| Cashflow {
            position: positions[f.position].id.to_string(),
            day,
            scenario: "all".into(),
            principal: f.values[0],
            cash_interest: f.values[1],
            accrual_interest: f.values[2],
            book_amortization: f.values[3],
        })
    }
    pub fn count(&self) -> usize {
        self.count
    }
    pub fn bytes(&self) -> usize {
        self.capacity_rows * std::mem::size_of::<Flow>()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn capacity_tracks_geometric_growth_and_order_is_enforced() {
        let mut store = FlowStore::new(30).unwrap();
        for i in 0..1000 {
            store.push(i, 30, [1., 0., 0., 0.]).unwrap();
        }
        assert_eq!(store.count(), 1000);
        assert_eq!(
            store.bytes(),
            store
                .days
                .iter()
                .map(|v| v.capacity() * std::mem::size_of::<Flow>())
                .sum::<usize>()
        );
        assert!(store.bytes() >= 1000 * std::mem::size_of::<Flow>());
        assert!(store.push(0, 30, [1., 0., 0., 0.]).is_err());
        assert!(store.push(0, 29, [1., 0., 0., 0.]).is_err());
        assert!(store.push(1000, 0, [1., 0., 0., 0.]).is_err());
    }
}
