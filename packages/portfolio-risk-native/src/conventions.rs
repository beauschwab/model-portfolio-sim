//! Gregorian dates and the existing model's adjusted-accrual coupon conventions.
//! Dates cross the boundary as Python-compatible ordinals (0001-01-01 = 1).
use portfolio_model_core::cashflow::{
    add_calendar_months, ordinal_from_ymd, ymd_from_ordinal, MonthRoll,
};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct Date(i32);
fn leap(y: i32) -> bool {
    y % 4 == 0 && (y % 100 != 0 || y % 400 == 0)
}
fn before_year(y: i32) -> i32 {
    let n = y - 1;
    365 * n + n / 4 - n / 100 + n / 400
}
impl Date {
    pub fn ordinal(value: i32) -> Result<Self, String> {
        if !(1..=3652059).contains(&value) {
            return Err("date outside years 1..9999".into());
        }
        Ok(Self(value))
    }
    pub fn new(y: i32, m: i32, d: i32) -> Result<Self, String> {
        let month = u32::try_from(m).map_err(|_| "invalid Gregorian month")?;
        let day = u32::try_from(d).map_err(|_| "invalid Gregorian day")?;
        Self::ordinal(ordinal_from_ymd(y, month, day)?)
    }
    pub fn number(self) -> i32 {
        self.0
    }
    pub fn ymd(self) -> (i32, i32, i32) {
        // Date's private ordinal is validated by every constructor.
        let (year, month, day) = ymd_from_ordinal(self.0).expect("validated Gregorian ordinal");
        (year, month as i32, day as i32)
    }
    fn weekday(self) -> i32 {
        (self.0 - 1) % 7
    }
    pub fn add_days(self, n: i32) -> Result<Self, String> {
        Self::ordinal(self.0.checked_add(n).ok_or("date overflow")?)
    }
    pub fn add_months(self, n: i32) -> Result<Self, String> {
        Self::ordinal(add_calendar_months(self.0, n, MonthRoll::ClampDay)?)
    }
}

#[derive(Clone, Copy, Deserialize, Serialize)]
pub enum DayCount {
    #[serde(rename = "30/360")]
    Thirty360,
    #[serde(rename = "ACT/360")]
    Act360,
    #[serde(rename = "ACT/365F")]
    Act365,
    #[serde(rename = "ACT/ACT")]
    ActAct,
}
#[derive(Clone, Copy, Default, Deserialize, Serialize, PartialEq)]
pub enum Bdc {
    #[serde(rename = "F")]
    Following,
    #[default]
    #[serde(rename = "MF")]
    ModifiedFollowing,
    #[serde(rename = "P")]
    Preceding,
    #[serde(rename = "NONE")]
    None,
}
pub fn year_fraction(start: Date, end: Date, basis: DayCount) -> f64 {
    let (y1, m1, d1) = start.ymd();
    let (y2, m2, d2) = end.ymd();
    let days = (end.0 - start.0) as f64;
    match basis {
        DayCount::Thirty360 => {
            let dd1 = d1.min(30);
            let dd2 = if d2 == 31 && dd1 == 30 { 30 } else { d2 };
            ((y2 - y1) * 360 + (m2 - m1) * 30 + dd2 - dd1) as f64 / 360.
        }
        DayCount::Act360 => days / 360.,
        DayCount::Act365 => days / 365.,
        DayCount::ActAct => {
            let denom = |y| if leap(y) { 366. } else { 365. };
            if y1 == y2 {
                days / denom(y1)
            } else {
                (before_year(y1 + 1) + 1 - start.0) as f64 / denom(y1)
                    + (end.0 - before_year(y2) - 1) as f64 / denom(y2)
                    + (y2 - y1 - 1) as f64
            }
        }
    }
}
fn nth(y: i32, m: i32, w: i32, n: i32) -> Result<Date, String> {
    let d = Date::new(y, m, 1)?;
    d.add_days((w - d.weekday()).rem_euclid(7) + 7 * (n - 1))
}
fn observed(d: Date) -> Result<Date, String> {
    d.add_days(match d.weekday() {
        5 => -1,
        6 => 1,
        _ => 0,
    })
}
fn holidays(y: i32) -> Result<HashSet<i32>, String> {
    let a = y % 19;
    let (b, c) = (y / 100, y % 100);
    let (d, e) = (b / 4, b % 4);
    let g = (8 * b + 13) / 25;
    let h = (19 * a + b - d - g + 15) % 30;
    let (i, k) = (c / 4, c % 4);
    let l = (32 + 2 * e + 2 * i - h - k) % 7;
    let m = (a + 11 * h + 22 * l) / 451;
    let easter = Date::new(
        y,
        (h + l - 7 * m + 114) / 31,
        (h + l - 7 * m + 114) % 31 + 1,
    )?;
    let may = Date::new(y, 5, 31)?;
    let values = [
        observed(Date::new(y, 1, 1)?)?,
        nth(y, 1, 0, 3)?,
        nth(y, 2, 0, 3)?,
        easter.add_days(-2)?,
        may.add_days(-may.weekday())?,
        observed(Date::new(y, 6, 19)?)?,
        observed(Date::new(y, 7, 4)?)?,
        nth(y, 9, 0, 1)?,
        nth(y, 10, 0, 2)?,
        observed(Date::new(y, 11, 11)?)?,
        nth(y, 11, 3, 4)?,
        observed(Date::new(y, 12, 25)?)?,
    ];
    Ok(values.iter().map(|d| d.0).collect())
}

#[derive(Clone, Default, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CalendarSpec {
    pub name: String,
    #[serde(default)]
    pub extra_holidays: Vec<i32>,
}
pub struct Calendar {
    us: bool,
    extras: HashSet<i32>,
    years: std::collections::HashMap<i32, HashSet<i32>>,
}
impl Calendar {
    pub fn new(spec: &CalendarSpec) -> Result<Self, String> {
        for &d in &spec.extra_holidays {
            Date::ordinal(d)?;
        }
        Ok(Self {
            us: spec.name == "US",
            extras: spec.extra_holidays.iter().copied().collect(),
            years: Default::default(),
        })
    }
    fn business_day(&mut self, d: Date) -> Result<bool, String> {
        if d.weekday() >= 5 || self.extras.contains(&d.0) {
            return Ok(false);
        }
        if !self.us {
            return Ok(true);
        }
        let y = d.ymd().0;
        if let std::collections::hash_map::Entry::Vacant(e) = self.years.entry(y) {
            e.insert(holidays(y)?);
        }
        // Match the reference's year-scoped holiday convention exactly, including
        // its treatment of a next-year New Year's observation in December.
        Ok(!self.years[&y].contains(&d.0))
    }
    pub fn adjust(&mut self, d: Date, bdc: Bdc) -> Result<Date, String> {
        if bdc == Bdc::None {
            return Ok(d);
        }
        let step = if bdc == Bdc::Preceding { -1 } else { 1 };
        let mut out = d;
        while !self.business_day(out)? {
            out = out.add_days(step)?;
        }
        if bdc == Bdc::ModifiedFollowing && out.ymd().1 != d.ymd().1 {
            out = d;
            while !self.business_day(out)? {
                out = out.add_days(-1)?;
            }
        }
        Ok(out)
    }
    pub fn schedule(
        &mut self,
        effective: Date,
        maturity: Date,
        freq: i32,
        basis: DayCount,
        bdc: Bdc,
    ) -> Result<Vec<(Date, Date, f64)>, String> {
        if maturity <= effective || !(1..=1200).contains(&freq) {
            return Err("future maturity and positive coupon frequency required".into());
        }
        let mut ends = Vec::new();
        let mut date = maturity;
        while date > effective {
            if ends.len() >= 4096 {
                return Err("coupon schedule exceeds 4096 periods".into());
            }
            ends.push(date);
            let months = freq * ends.len() as i32;
            // A backward roll before year one is already before the effective date.
            if months > maturity.ymd().0 * 12 + maturity.ymd().1 - 1 - 12 {
                break;
            }
            date = maturity.add_months(-months)?;
        }
        ends.reverse();
        let mut prev = self.adjust(effective, bdc)?;
        let mut out = Vec::with_capacity(ends.len());
        for end in ends {
            let pay = self.adjust(end, bdc)?;
            out.push((prev, pay, year_fraction(prev, pay, basis)));
            prev = pay;
        }
        Ok(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn dates_daycounts_and_business_adjustments() {
        assert_eq!(Date::new(1, 1, 1).unwrap().number(), 1);
        assert_eq!(Date::new(9999, 12, 31).unwrap().number(), 3652059);
        for ordinal in [1, 59, 60, 366, 730179, 738945, 3652059] {
            let d = Date::ordinal(ordinal).unwrap();
            let (y, m, day) = d.ymd();
            assert_eq!(Date::new(y, m, day).unwrap(), d);
        }
        let a = Date::new(2024, 2, 29).unwrap();
        assert_eq!(a.add_months(12).unwrap(), Date::new(2025, 2, 28).unwrap());
        assert!(
            (year_fraction(
                Date::new(2023, 12, 31).unwrap(),
                Date::new(2024, 1, 2).unwrap(),
                DayCount::ActAct
            ) - 1. / 365.
                - 1. / 366.)
                .abs()
                < 1e-15
        );
        let mut cal = Calendar::new(&CalendarSpec {
            name: "US".into(),
            extra_holidays: vec![],
        })
        .unwrap();
        assert_eq!(
            cal.adjust(Date::new(2026, 7, 3).unwrap(), Bdc::Following)
                .unwrap(),
            Date::new(2026, 7, 6).unwrap()
        );
        assert_eq!(
            cal.adjust(Date::new(2026, 5, 31).unwrap(), Bdc::ModifiedFollowing)
                .unwrap(),
            Date::new(2026, 5, 29).unwrap()
        );
        assert!(cal.schedule(a, a, 3, DayCount::Act365, Bdc::None).is_err());
    }
}
