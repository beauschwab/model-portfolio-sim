//! Closing reports are derived from replayed postings, never simulated cash/equity.
use super::{types::Account, R};
use serde_json::{json, Value};
use std::collections::{BTreeMap, BTreeSet, HashMap};

type GlKey = (usize, usize, usize);
#[derive(Default)]
pub struct Reports {
    replay: HashMap<GlKey, f64>,
    attribution: BTreeMap<(String, String, String), [f64; 3]>,
}
impl Reports {
    pub fn posting(&mut self, account: usize, gl: usize, instrument: usize, value: f64) {
        *self.replay.entry((account, gl, instrument)).or_default() += value;
    }
    pub fn event(&mut self, scenario: &str, account: &str, event: &str, values: [f64; 3]) {
        let total = self
            .attribution
            .entry((scenario.into(), account.into(), event.into()))
            .or_default();
        for (sum, value) in total.iter_mut().zip(values) {
            *sum += value;
        }
    }
    pub fn close(
        &mut self,
        scenario: &str,
        accounts: &[Account],
        trial: &[(usize, usize, usize, f64)],
        dictionary: &[String],
    ) -> R<Vec<(&'static str, Value)>> {
        if trial.len() != self.replay.len() {
            return Err("replayed GL key set mismatch".into());
        }
        let mut seen = BTreeSet::new();
        for &(account, gl, instrument, value) in trial {
            let key = (account, gl, instrument);
            let got = self.replay.get(&key).ok_or("missing replayed GL key")?;
            if !seen.insert(key)
                || !value.is_finite()
                || !got.is_finite()
                || (got - value).abs() > 1e-9_f64.max(1e-12 * got.abs().max(value.abs()))
            {
                return Err("replayed closing GL mismatch".into());
            }
        }
        let funding: BTreeSet<_> = self
            .replay
            .keys()
            .filter(|k| dictionary[k.1] == "funding_principal")
            .map(|k| (&k.0, &k.2))
            .collect();
        let mut totals: HashMap<&str, [f64; 7]> =
            accounts.iter().map(|a| (a.id.as_str(), [0.; 7])).collect();
        // Preserve the reference's sorted GL reduction order.
        let mut keys: Vec<_> = self.replay.keys().collect();
        keys.sort_by(|a, b| {
            (&dictionary[a.0], &dictionary[a.1], &dictionary[a.2]).cmp(&(
                &dictionary[b.0],
                &dictionary[b.1],
                &dictionary[b.2],
            ))
        });
        for key in keys {
            let value = self.replay[key];
            let t = totals
                .get_mut(dictionary[key.0].as_str())
                .ok_or("unknown replay account")?;
            let gl = dictionary[key.1].as_str();
            if gl == "cash" {
                t[3] += value;
            }
            if gl == "restricted_cash" {
                t[4] += value;
            }
            if gl.starts_with("pnl:")
                || matches!(gl, "opening_equity" | "oci" | "equity_distributions")
            {
                t[2] -= value;
            } else if matches!(
                gl,
                "funding_principal"
                    | "secured_funding"
                    | "received_margin"
                    | "intercompany_payable"
            ) || (matches!(gl, "book_adjustment" | "accrued_interest")
                && funding.contains(&(&key.0, &key.2)))
                || (gl == "derivative_value" && value < 0.)
            {
                t[1] -= value;
            } else {
                t[0] += value;
            }
            if gl == "intercompany_receivable" {
                t[5] += value;
            }
            if gl == "intercompany_payable" {
                t[6] -= value;
            }
        }
        let mut output = Vec::new();
        let mut consolidated: Vec<(&str, [f64; 5])> = Vec::new();
        for a in accounts {
            let t = totals[a.id.as_str()];
            let error = t[0] - t[1] - t[2];
            if !t.iter().all(|v| v.is_finite())
                || error.abs() > 1e-8 * t[0].abs().max(t[1].abs()).max(1.)
            {
                return Err("journal-derived financial statements do not balance".into());
            }
            output.push(("closing_statements", json!({"scenario":scenario,"account":a.id,"currency":a.currency,
                "assets":t[0],"liabilities":t[1],"equity":t[2],"cash":t[3],"restricted_cash":t[4],
                "intercompany_assets":t[5],"intercompany_liabilities":t[6],"reconciliation_error":error})));
            let index = consolidated
                .iter()
                .position(|(c, _)| *c == a.currency)
                .unwrap_or_else(|| {
                    consolidated.push((&a.currency, [0.; 5]));
                    consolidated.len() - 1
                });
            let total = &mut consolidated[index].1;
            for i in 0..5 {
                total[i] += t[i];
            }
            total[0] -= t[5];
            total[1] -= t[6];
        }
        for (currency, t) in consolidated {
            output.push((
                "consolidated",
                json!({"scenario":scenario,"currency":currency,"assets":t[0],
                "liabilities":t[1],"equity":t[2],"cash":t[3],"restricted_cash":t[4]}),
            ));
        }
        self.replay.clear();
        Ok(output)
    }
    pub fn attribution(&self, scenarios: &[super::types::Scenario]) -> Vec<Value> {
        let keys: BTreeSet<_> = self.attribution.keys().map(|(_, a, e)| (a, e)).collect();
        let mut rows = Vec::new();
        for scenario in scenarios {
            for &(account, event) in &keys {
                let get = |name: &str| {
                    self.attribution
                        .get(&(name.into(), account.clone(), event.clone()))
                        .copied()
                        .unwrap_or_default()
                };
                let base = get("baseline");
                let stress = get(&scenario.name);
                rows.push(json!({"scenario":scenario.name,"account":account,"event":event,
                    "cash_delta":stress[0]-base[0],"earnings_delta":stress[1]-base[1],"aoci_delta":stress[2]-base[2]}));
            }
        }
        rows
    }
}
