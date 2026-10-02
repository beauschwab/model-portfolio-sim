//! Explicit research weight tables and raw balance-sheet rows, aggregated natively.
use crate::balance_risk::BalanceRiskRequest;
use crate::lifecycle_market::finite;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::BTreeMap;

#[derive(Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Row {
    pub id: String,
    pub balance: f64,
    pub price: f64,
    pub maturity: Option<i32>,
    pub side: Option<String>,
    pub segment: Option<String>,
    pub channel: Option<String>,
}
type Books = BTreeMap<String, Vec<Row>>;
#[derive(Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Weights {
    pub lcr_runoff: BTreeMap<String, f64>,
    pub lcr_cd_runoff: BTreeMap<String, f64>,
    pub lcr_secured: BTreeMap<String, f64>,
    pub lcr_inflow: BTreeMap<String, f64>,
    pub nsfr_asf: BTreeMap<String, f64>,
    pub nsfr_rsf: BTreeMap<String, f64>,
    pub rwa: BTreeMap<String, f64>,
    pub l2_cap: f64,
    pub l2a_factor: f64,
    pub inflow_cap: f64,
    pub rwa_density: f64,
    pub cet1_ratio: f64,
    pub ni_to_nii: f64,
    pub payout: f64,
}
#[derive(Clone, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Mode {
    All,
    Eve,
    Lcr,
    Nsfr,
    Capital,
}
#[derive(Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct KpiRequest {
    pub mode: Mode,
    pub books: Books,
    pub equity: Option<f64>,
    pub asof: i32,
    pub weights: Weights,
    pub nii: Option<Vec<f64>>,
    pub stress_aoci: Vec<f64>,
    pub shocks: Vec<f64>,
    pub dv01s: BTreeMap<String, f64>,
    pub risk: Option<Box<BalanceRiskRequest>>,
}
fn rows<'a>(books: &'a Books, key: &str) -> &'a [Row] {
    books.get(key).map_or(&[], Vec::as_slice)
}
fn mv(r: &Row) -> f64 {
    r.balance * r.price / 100.
}
fn sum(books: &Books, keys: &[&str]) -> f64 {
    keys.iter()
        .map(|k| rows(books, k).iter().map(mv).sum::<f64>())
        .sum()
}
fn mm(books: &Books, side: &str) -> f64 {
    rows(books, "mm")
        .iter()
        .filter(|r| r.side.as_deref() == Some(side))
        .map(|r| r.balance)
        .sum()
}
fn keyed(book: &[Row], weights: &BTreeMap<String, f64>, key: impl Fn(&Row) -> &str) -> f64 {
    weights
        .iter()
        .map(|(k, w)| {
            book.iter()
                .filter(|r| key(r) == k)
                .map(|r| r.balance)
                .sum::<f64>()
                * w
        })
        .sum()
}
impl KpiRequest {
    fn eve(&self, books: &Books, dv: &BTreeMap<String, f64>) -> Value {
        let a = sum(books, &["mbs", "loans"]) + mm(books, "asset");
        let l = sum(books, &["debt", "deposits", "cds"]) + mm(books, "liability");
        let eve = a - l;
        let da = ["mbs", "loans", "mm", "hedges"]
            .iter()
            .map(|k| dv.get(*k).copied().unwrap_or(0.))
            .sum::<f64>();
        let dl = ["debt", "deposits", "cds"]
            .iter()
            .map(|k| dv.get(*k).copied().unwrap_or(0.))
            .sum::<f64>();
        let net = da - dl;
        let dura = da * 1e4 / a.max(1e-9);
        let durl = dl * 1e4 / l.max(1e-9);
        let worst = if eve == 0. {
            f64::NAN
        } else {
            self.shocks
                .iter()
                .map(|s| (-net * s / eve * 100.).abs())
                .fold(0., f64::max)
        };
        let sensitivity:Vec<_>=self.shocks.iter().map(|s|json!({"shock_bp":s,"d_eve_$":-net*s,"d_eve_pct_eve":if eve==0.{f64::NAN}else{-net*s/eve*100.},"method":"first-order (parallel dv01); convexity in 9Q stress"})).collect();
        json!({"eve_$":eve,"mv_assets_$":a,"mv_liabilities_$":l,"irrbb_outlier":worst>15.,"irrbb_worst_pct_eve":worst,
            "dv01_net_$":net,"dur_assets_y":dura,"dur_liab_y":durl,"duration_gap_y":if a==0.{f64::NAN}else{dura-l/a*durl},
            "eve_duration_y":net*1e4/eve.max(1e-9),"hedge_dv01_$":dv.get("hedges").copied().unwrap_or(0.),"sensitivity":sensitivity})
    }
    fn lcr(&self, books: &Books) -> Value {
        let w = &self.weights;
        let markets = rows(books, "mm");
        let l1 = markets
            .iter()
            .filter(|r| r.id == "IEDB")
            .map(|r| r.balance)
            .sum::<f64>();
        let l2raw = rows(books, "mbs")
            .iter()
            .filter(|r| !r.id.starts_with("HL"))
            .map(mv)
            .sum::<f64>()
            * w.l2a_factor;
        let l2 = l2raw.min(l1 * w.l2_cap / (1. - w.l2_cap));
        let mut out = keyed(rows(books, "deposits"), &w.lcr_runoff, |r| {
            r.segment.as_deref().unwrap_or("")
        });
        let cds: Vec<_> = rows(books, "cds")
            .iter()
            .filter(|r| r.maturity.is_some_and(|d| d <= self.asof + 30))
            .cloned()
            .collect();
        out += keyed(&cds, &w.lcr_cd_runoff, |r| {
            r.channel.as_deref().unwrap_or("")
        });
        out += rows(books, "debt")
            .iter()
            .filter(|r| r.maturity.is_some_and(|d| d <= self.asof + 30))
            .map(|r| r.balance)
            .sum::<f64>();
        out += keyed(markets, &w.lcr_secured, |r| &r.id);
        let inflow = keyed(markets, &w.lcr_inflow, |r| &r.id).min(w.inflow_cap * out);
        let net = (out - inflow).max(1e-9);
        json!({"hqla_l2a_uncapped_$":l2raw,"hqla_l1_$":l1,"hqla_l2a_$":l2,"hqla_$":l1+l2,"outflows_$":out,"inflows_capped_$":inflow,"net_outflows_$":net,"lcr_pct":(l1+l2)/net*100.})
    }
    fn nsfr(&self) -> Value {
        let b = &self.books;
        let w = &self.weights;
        let y1 = self.asof + 365;
        let m6 = self.asof + 182;
        let mut asf = w
            .lcr_runoff
            .keys()
            .map(|k| {
                rows(b, "deposits")
                    .iter()
                    .filter(|r| r.segment.as_ref() == Some(k))
                    .map(|r| r.balance)
                    .sum::<f64>()
                    * w.nsfr_asf[k]
            })
            .sum::<f64>();
        for r in rows(b, "cds") {
            let key = if r.maturity >= Some(y1) {
                "cd_ge1y"
            } else if r.channel.as_deref() == Some("retail") {
                "cd_retail_lt1y"
            } else if r.channel.as_deref() == Some("brokered") && r.maturity < Some(m6) {
                "cd_brokered_lt6m"
            } else {
                continue;
            };
            asf += r.balance * w.nsfr_asf[key];
        }
        for r in rows(b, "debt") {
            asf += r.balance
                * w.nsfr_asf[if r.maturity >= Some(y1) {
                    "ltd_ge1y"
                } else {
                    "ltd_lt1y"
                }];
        }
        let equity = self.equity.unwrap_or_else(|| {
            sum(b, &["mbs", "loans"]) - sum(b, &["debt", "deposits", "cds"]) + mm(b, "asset")
                - mm(b, "liability")
        });
        asf += equity.max(0.) * w.nsfr_asf["equity"];
        let mut rsf = rows(b, "mm")
            .iter()
            .filter(|r| ["IEDB", "RESALE", "TRADING_A"].contains(&r.id.as_str()))
            .map(|r| r.balance * w.nsfr_rsf[&r.id])
            .sum::<f64>();
        for r in rows(b, "mbs") {
            rsf += mv(r)
                * w.nsfr_rsf[if r.id.starts_with("HL") {
                    "resi_mtg"
                } else {
                    "mbs_l2a"
                }];
        }
        for r in rows(b, "loans") {
            rsf += r.balance
                * w.nsfr_rsf[if r.maturity >= Some(y1) {
                    "loan_ge1y"
                } else {
                    "loan_lt1y"
                }];
        }
        json!({"asf_$":asf,"rsf_$":rsf,"nsfr_pct":asf/rsf.max(1e-9)*100.})
    }
    fn capital(&self) -> Value {
        let b = &self.books;
        let w = &self.weights;
        let mut rwa = rows(b, "mbs")
            .iter()
            .map(|r| {
                mv(r)
                    * w.rwa[if r.id.starts_with("HL") {
                        "resi_mtg"
                    } else {
                        "agency_mbs"
                    }]
            })
            .sum::<f64>();
        rwa += rows(b, "loans")
            .iter()
            .map(|r| {
                r.balance
                    * w.rwa[if r.id.starts_with("AUTO") {
                        "auto"
                    } else {
                        "corp_loan"
                    }]
            })
            .sum::<f64>();
        rwa += rows(b, "mm")
            .iter()
            .filter(|r| ["IEDB", "RESALE", "TRADING_A"].contains(&r.id.as_str()))
            .map(|r| r.balance * w.rwa[&r.id])
            .sum::<f64>();
        let assets = sum(b, &["mbs", "loans"]) + mm(b, "asset");
        let addon = (w.rwa_density * assets - rwa).max(0.);
        let total = rwa + addon;
        let initial = w.cet1_ratio * total;
        let mut cet1 = initial;
        let ratio = |v| {
            if total == 0. {
                f64::NAN
            } else {
                v / total * 100.
            }
        };
        let mut path =
            vec![json!({"quarter":0,"cet1_$":cet1,"cet1_ratio_pct":ratio(cet1),"drivers":"t0"})];
        if let Some(nii) = &self.nii {
            for (q, months) in nii.chunks(3).enumerate() {
                let retained = months.iter().sum::<f64>() * w.ni_to_nii * (1. - w.payout);
                let aoci = self.stress_aoci.get(q).copied().unwrap_or(0.);
                cet1 += retained + aoci;
                let drivers = format!(
                    "retained {:.0}M{}",
                    retained / 1e6,
                    if aoci != 0. {
                        format!(", AOCI {:+.0}M", aoci / 1e6)
                    } else {
                        String::new()
                    }
                );
                path.push(json!({"quarter":q+1,"cet1_$":cet1,"cet1_ratio_pct":ratio(cet1),"drivers":drivers}));
            }
        }
        json!({"rwa_credit_$":rwa,"rwa_addon_$":addon,"rwa_total_$":total,"rwa_density_pct":total/assets.max(1e-9)*100.,"cet1_t0_$":initial,"cet1_path":path,
            "cfh_aoci_note":"cash-flow-hedge AOCI is EXCLUDED from CET1 (12 CFR 217.22(b)); AFS AOCI flows through for Category I/II -- pass only AFS marks via stress_aoci_q",
            "note":"retained earnings = NII x NI_TO_NII (0.43, filing-calibrated net effect of provisions/opex/fees) x (1-payout); RWA static -- a capital-plan skeleton, not PPNR"})
    }
    pub fn run(&self) -> Result<Value, String> {
        crate::compute_span!("kpis");
        crate::conventions::Date::ordinal(self.asof)?;
        for (key, book) in &self.books {
            if !["mbs", "loans", "debt", "deposits", "cds", "mm"].contains(&key.as_str()) {
                return Err("unknown KPI book".into());
            }
            for row in book {
                finite(&[row.balance, row.price])?;
                if row.balance < 0. || row.price < 0. {
                    return Err("negative KPI balance or price".into());
                }
                if ["loans", "debt", "cds"].contains(&key.as_str()) {
                    crate::conventions::Date::ordinal(row.maturity.ok_or("missing KPI maturity")?)?;
                }
            }
        }
        let w = &self.weights;
        for map in [
            &w.lcr_runoff,
            &w.lcr_cd_runoff,
            &w.lcr_secured,
            &w.lcr_inflow,
            &w.nsfr_asf,
            &w.nsfr_rsf,
            &w.rwa,
        ] {
            for v in map.values() {
                finite(&[*v])?;
            }
        }
        finite(&[
            w.l2_cap,
            w.l2a_factor,
            w.inflow_cap,
            w.rwa_density,
            w.cet1_ratio,
            w.ni_to_nii,
            w.payout,
        ])?;
        if !(0.0..1.0).contains(&w.l2_cap) || !(0.0..=1.0).contains(&w.inflow_cap) {
            return Err("invalid liquidity composition caps".into());
        }
        if let Some(v) = self.equity {
            finite(&[v])?;
        }
        finite(&self.shocks)?;
        finite(&self.stress_aoci)?;
        if let Some(v) = &self.nii {
            finite(v)?;
        }
        for v in self.dv01s.values() {
            finite(&[*v])?;
        }
        // Require each model mapping explicitly; never silently substitute zero.
        for (map, keys) in [
            (
                &w.nsfr_asf,
                &[
                    "equity",
                    "cd_ge1y",
                    "cd_retail_lt1y",
                    "cd_brokered_lt6m",
                    "ltd_ge1y",
                    "ltd_lt1y",
                ][..],
            ),
            (
                &w.nsfr_rsf,
                &[
                    "IEDB",
                    "RESALE",
                    "TRADING_A",
                    "resi_mtg",
                    "mbs_l2a",
                    "loan_ge1y",
                    "loan_lt1y",
                ][..],
            ),
            (
                &w.rwa,
                &[
                    "resi_mtg",
                    "agency_mbs",
                    "auto",
                    "corp_loan",
                    "IEDB",
                    "RESALE",
                    "TRADING_A",
                ][..],
            ),
        ] {
            if keys.iter().any(|k| !map.contains_key(*k)) {
                return Err("incomplete KPI weight mapping".into());
            }
        }
        if w.lcr_runoff.keys().any(|k| !w.nsfr_asf.contains_key(k)) {
            return Err("missing deposit ASF mapping".into());
        }
        match self.mode {
            Mode::Lcr => Ok(self.lcr(&self.books)),
            Mode::Nsfr => Ok(self.nsfr()),
            Mode::Capital => Ok(self.capital()),
            Mode::Eve => Ok(self.eve(&self.books, &self.dv01s)),
            Mode::All => {
                let risk = self
                    .risk
                    .as_ref()
                    .ok_or("KPI run requires raw risk request")?
                    .run()?;
                let mut valued = self.books.clone();
                for (key, prices) in risk.valued_prices {
                    let book = valued.get_mut(&key).ok_or("risk result book mismatch")?;
                    if prices.len() != book.len() {
                        return Err("risk result row mismatch".into());
                    }
                    for (r, px) in book.iter_mut().zip(prices) {
                        r.price = px * 100.;
                    }
                }
                Ok(
                    json!({"nii_total_$":self.nii.as_ref().map_or(0.,|v|v.iter().sum::<f64>()),"eve":self.eve(&valued,&risk.dv01s),"dv01s":risk.dv01s,"lcr":self.lcr(&valued),"nsfr":self.nsfr(),"capital":self.capital()}),
                )
            }
        }
    }
}
