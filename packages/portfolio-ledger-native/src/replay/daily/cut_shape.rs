//! Nonallocating structural JSON census. Numbers are token widths, never money.
use serde::Serialize;
use std::cell::RefCell;
use std::io::{self, Write};

struct Ticket {
    used: usize,
    limit: usize,
}
thread_local! {static TICKET:RefCell<Option<Ticket>>=const {RefCell::new(None)};}
pub fn spent() -> Option<usize> {
    TICKET.with(|t| t.borrow().as_ref().map(|t| t.used))
}
pub fn charge(work: usize) -> Result<(), String> {
    TICKET.with(|t|{
        let mut ticket=t.borrow_mut();
        if let Some(ticket)=ticket.as_mut(){
            let next=ticket.used.checked_add(work).ok_or("selected cut census work overflow")?;
            if next>ticket.limit{return Err(format!("selected cut pre-Sim census exhausted remaining work before scan: used={},next={next},limit={}",ticket.used,ticket.limit));}
            ticket.used=next;
        }
        Ok(())
    })
}
/// A scope cannot execute a sizing pass on an unreserved root-work allowance.
/// Errors restore the previous thread state, including serializer refusals.
pub fn with_budget<T>(limit: usize, body: impl FnOnce() -> Result<T, String>) -> Result<T, String> {
    if spent().is_some() {
        return Err("selected cut nested census cannot replace the remaining work ticket".into());
    }
    let previous = TICKET.with(|t| t.replace(Some(Ticket { used: 0, limit })));
    struct Restore(Option<Ticket>);
    impl Drop for Restore {
        fn drop(&mut self) {
            TICKET.with(|t| {
                t.replace(self.0.take());
            });
        }
    }
    let _restore = Restore(previous);
    body()
}

#[derive(Clone, Copy, Debug, Default, Serialize)]
pub struct Shape {
    pub encoded_bytes_upper: usize,
    pub cells_upper: usize,
    pub string_bytes_upper: usize,
}
fn add(a: usize, b: usize) -> Result<usize, String> {
    a.checked_add(b)
        .ok_or("cut structural JSON census overflow".into())
}
fn mul(a: usize, b: usize) -> Result<usize, String> {
    a.checked_mul(b)
        .ok_or("cut structural JSON census overflow".into())
}
impl Shape {
    pub fn number() -> Self {
        Self {
            encoded_bytes_upper: 32,
            cells_upper: 1,
            string_bytes_upper: 0,
        }
    }
    pub fn boolean() -> Self {
        Self {
            encoded_bytes_upper: 5,
            cells_upper: 1,
            string_bytes_upper: 0,
        }
    }
    pub fn checked_add(self, other: Self) -> Result<Self, String> {
        Ok(Self {
            encoded_bytes_upper: add(self.encoded_bytes_upper, other.encoded_bytes_upper)?,
            cells_upper: add(self.cells_upper, other.cells_upper)?,
            string_bytes_upper: add(self.string_bytes_upper, other.string_bytes_upper)?,
        })
    }
    pub fn times(self, count: usize) -> Result<Self, String> {
        Ok(Self {
            encoded_bytes_upper: mul(self.encoded_bytes_upper, count)?,
            cells_upper: mul(self.cells_upper, count)?,
            string_bytes_upper: mul(self.string_bytes_upper, count)?,
        })
    }
    pub fn max(self, other: Self) -> Self {
        Self {
            encoded_bytes_upper: self.encoded_bytes_upper.max(other.encoded_bytes_upper),
            cells_upper: self.cells_upper.max(other.cells_upper),
            string_bytes_upper: self.string_bytes_upper.max(other.string_bytes_upper),
        }
    }
    pub fn scan_work(self) -> Result<usize, String> {
        // One visit per cell plus every encoded byte, with32 characters for
        // each finite f64/u64/i64 token. This is independent of Value padding.
        add(self.encoded_bytes_upper, self.cells_upper)
    }
}

#[derive(Default)]
struct Counter {
    bytes: usize,
    cells: usize,
    strings: usize,
    numbers: usize,
    number_bytes: usize,
    number_width: usize,
    quoted: bool,
    escaped: bool,
    atom: bool,
}
impl Counter {
    fn finish_number(&mut self) -> io::Result<()> {
        if self.number_width > 32 {
            return Err(io::Error::other("cut numeric JSON token exceeds32bytes"));
        }
        if self.number_width > 0 {
            charge(32 - self.number_width).map_err(io::Error::other)?;
        }
        self.number_width = 0;
        Ok(())
    }
    fn finish(mut self) -> Result<Shape, String> {
        self.finish_number().map_err(|e| e.to_string())?;
        Ok(Shape {
            encoded_bytes_upper: add(
                self.bytes
                    .checked_sub(self.number_bytes)
                    .ok_or("cut numeric JSON census differs")?,
                mul(self.numbers, 32)?,
            )?,
            cells_upper: self.cells,
            string_bytes_upper: self.strings,
        })
    }
}
impl Write for Counter {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        portfolio_compute_control::checkpoint().map_err(io::Error::other)?;
        charge(bytes.len()).map_err(io::Error::other)?;
        let checked = |n: &mut usize, value| {
            *n = n
                .checked_add(value)
                .ok_or_else(|| io::Error::other("cut structural JSON census overflow"))?;
            Ok::<_, io::Error>(())
        };
        checked(&mut self.bytes, bytes.len())?;
        for &b in bytes {
            if self.quoted {
                checked(&mut self.strings, 1)?;
                if self.escaped {
                    self.escaped = false;
                } else if b == b'\\' {
                    self.escaped = true;
                } else if b == b'"' {
                    self.quoted = false;
                }
                continue;
            }
            if self.number_width > 0 {
                if b.is_ascii_digit() || matches!(b, b'.' | b'e' | b'E' | b'+' | b'-') {
                    checked(&mut self.number_width, 1)?;
                    checked(&mut self.number_bytes, 1)?;
                    continue;
                }
                self.finish_number()?;
            }
            if b == b'"' {
                self.atom = false;
                self.quoted = true;
                checked(&mut self.strings, 1)?;
                checked(&mut self.cells, 1)?;
                charge(1).map_err(io::Error::other)?;
            } else if b.is_ascii_digit() || b == b'-' {
                self.atom = false;
                self.number_width = 1;
                checked(&mut self.numbers, 1)?;
                checked(&mut self.number_bytes, 1)?;
                checked(&mut self.cells, 1)?;
                charge(1).map_err(io::Error::other)?;
            } else if matches!(b, b'{' | b'[') {
                self.atom = false;
                checked(&mut self.cells, 1)?;
                charge(1).map_err(io::Error::other)?;
            } else if b.is_ascii_alphabetic() {
                if !self.atom {
                    checked(&mut self.cells, 1)?;
                    charge(1).map_err(io::Error::other)?;
                    self.atom = true;
                }
            } else {
                self.atom = false;
            }
        }
        Ok(bytes.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

pub fn shape<T: Serialize + ?Sized>(value: &T) -> Result<Shape, String> {
    measured_shape(value).map(|(shape, _)| shape)
}

pub fn measured_shape<T: Serialize + ?Sized>(value: &T) -> Result<(Shape, usize), String> {
    let mut counter = Counter::default();
    serde_json::to_writer(&mut counter, value).map_err(|e| e.to_string())?;
    let bytes = counter.bytes;
    Ok((counter.finish()?, bytes))
}

pub fn object(fields: &[(&str, Shape)]) -> Result<Shape, String> {
    charge(add(fields.len(), 2)?)?;
    let mut result = Shape {
        encoded_bytes_upper: add(2, fields.len().saturating_sub(1))?,
        cells_upper: 1,
        string_bytes_upper: 0,
    };
    for (key, value) in fields {
        result = result.checked_add(shape(key)?)?.checked_add(*value)?;
        result.encoded_bytes_upper = add(result.encoded_bytes_upper, 1)?;
    }
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn numeric_width_is_independent_of_money_and_preserves_escaped_strings() {
        let a = shape(&serde_json::json!({"rights":[-0.0,1e-20],"id":"a\\\"\n\u{975e}"})).unwrap();
        let b =
            shape(&serde_json::json!({"rights":[f64::MAX,-1e200],"id":"a\\\"\n\u{975e}"})).unwrap();
        assert_eq!(a.encoded_bytes_upper, b.encoded_bytes_upper);
        assert_eq!(a.cells_upper, b.cells_upper);
        assert!(a.string_bytes_upper > 10);
    }
    #[test]
    fn object_inventory_matches_streaming_serializer_and_checks_overflow() {
        let actual = shape(&serde_json::json!({"day":2,"owner":"loan","valid":false})).unwrap();
        let expected = object(&[
            ("day", Shape::number()),
            ("owner", shape("loan").unwrap()),
            ("valid", Shape::boolean()),
        ])
        .unwrap();
        assert_eq!(actual.encoded_bytes_upper, expected.encoded_bytes_upper);
        assert_eq!(actual.cells_upper, expected.cells_upper);
        assert!(actual.times(usize::MAX).is_err());
    }
    #[test]
    fn sizing_ticket_refuses_before_unreserved_scan_and_restores_after_error() {
        assert!(with_budget(1, || shape("unreserved source body"))
            .unwrap_err()
            .contains("before scan"));
        assert!(spent().is_none());
        let expected = shape(&serde_json::json!({"clock":2,"rights":[-0.0,1.0]})).unwrap();
        with_budget(expected.scan_work().unwrap(), || {
            let got = shape(&serde_json::json!({"clock":2,"rights":[-0.0,1.0]}))?;
            assert_eq!(spent(), Some(got.scan_work()?));
            assert!(with_budget(usize::MAX, || Ok(()))
                .unwrap_err()
                .contains("cannot replace"));
            assert!(charge(1).is_err());
            Ok(())
        })
        .unwrap();
        assert!(spent().is_none());
    }
}
