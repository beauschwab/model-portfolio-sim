//! Shared exact commercial economic cash extraction, independent of graph orchestration.
use portfolio_model_core::credit_path_income as carry;
use serde::Serialize;
fn check() -> Result<(), String> {
    portfolio_compute_control::checkpoint()
}
fn money(x: f64) -> Result<f64, String> {
    crate::products::term::contracts::amount(x)
}
#[derive(Clone, Serialize)]
pub struct EconomicCash {
    pub event_id: String,
    pub kind: String,
    pub year: f64,
    pub principal: f64,
    pub interest: f64,
    pub fees: f64,
}
impl EconomicCash {
    pub fn total(&self) -> Result<f64, String> {
        money(self.principal + self.interest + self.fees)
    }
}
pub fn economic_cash(c: &carry::ResultData) -> Result<Vec<EconomicCash>, String> {
    let mut out = Vec::new();
    for path in &c.credit().result.paths {
        for e in &path.events {
            check()?;
            if e.kind == "transition" && e.cash == 0. {
                continue;
            }
            let (principal, interest, fees) =
                if e.kind == "draw" || (e.kind == "transition" && e.cash < 0.) {
                    (e.cash, 0., 0.)
                } else if e.cash != 0. || e.principal != 0. || e.interest != 0. {
                    (
                        e.principal,
                        e.interest,
                        money(e.cash - e.principal - e.interest)?,
                    )
                } else {
                    continue;
                };
            out.push(EconomicCash {
                event_id: e.event_id.clone(),
                kind: e.kind.clone(),
                year: e.year,
                principal,
                interest,
                fees,
            });
        }
        if let Some(default) = &path.default {
            for (index, e) in default.recoveries.iter().enumerate() {
                check()?;
                out.push(EconomicCash {
                    event_id: format!("recovery:{index}"),
                    kind: "recovery".into(),
                    year: e.year,
                    principal: e.principal,
                    interest: e.interest,
                    fees: 0.,
                });
            }
        }
    }
    out.sort_by(|a, b| a.year.total_cmp(&b.year));
    for c in &out {
        c.total()?;
    }
    Ok(out)
}
