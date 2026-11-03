//! Shared prepaid traversal for audited built-in JSON serializers.
//! No Value clone, sizing walk or intermediate encoded document is required.
use serde::ser::{self, Serialize, Serializer};
use std::{
    cell::{Cell, RefCell},
    io::Write,
};

struct Meter<F> {
    charge: RefCell<F>,
    work: Cell<usize>,
}
impl<F: FnMut(usize) -> Result<(), String>> Meter<F> {
    fn pay<E: ser::Error>(&self, work: usize) -> Result<(), E> {
        let next = self
            .work
            .get()
            .checked_add(work)
            .ok_or_else(|| E::custom("cut codec work overflow"))?;
        (self.charge.borrow_mut())(work).map_err(E::custom)?;
        self.work.set(next);
        Ok(())
    }
}
struct Wrapped<'a, T: ?Sized, F> {
    value: &'a T,
    meter: &'a Meter<F>,
    depth: usize,
}
impl<T: ?Sized + Serialize, F: FnMut(usize) -> Result<(), String>> Serialize for Wrapped<'_, T, F> {
    fn serialize<S: Serializer>(&self, inner: S) -> Result<S::Ok, S::Error> {
        if self.depth > 128 {
            return Err(ser::Error::custom("cut codec nesting exceeds128"));
        }
        self.meter.pay::<S::Error>(0)?;
        self.value.serialize(Paid {
            inner,
            meter: self.meter,
            depth: self.depth,
        })
    }
}
struct Paid<'a, S, F> {
    inner: S,
    meter: &'a Meter<F>,
    depth: usize,
}
macro_rules! number {
    ($($name:ident: $ty:ty),* $(,)?) => {$(
        fn $name(self, value: $ty) -> Result<Self::Ok, Self::Error> {
            self.meter.pay::<Self::Error>(33)?;
            self.inner.$name(value)
        }
    )*};
}
impl<'a, S: Serializer, F: FnMut(usize) -> Result<(), String>> Serializer for Paid<'a, S, F> {
    type Ok = S::Ok;
    type Error = S::Error;
    type SerializeSeq = Paid<'a, S::SerializeSeq, F>;
    type SerializeTuple = Paid<'a, S::SerializeTuple, F>;
    type SerializeTupleStruct = Paid<'a, S::SerializeTupleStruct, F>;
    type SerializeTupleVariant = Paid<'a, S::SerializeTupleVariant, F>;
    type SerializeMap = Paid<'a, S::SerializeMap, F>;
    type SerializeStruct = Paid<'a, S::SerializeStruct, F>;
    type SerializeStructVariant = Paid<'a, S::SerializeStructVariant, F>;

    number!(serialize_i8:i8, serialize_i16:i16, serialize_i32:i32, serialize_i64:i64,
        serialize_u8:u8, serialize_u16:u16, serialize_u32:u32, serialize_u64:u64);
    fn serialize_i128(self, value: i128) -> Result<Self::Ok, Self::Error> {
        if value < i64::MIN as i128 || value > u64::MAX as i128 {
            return Err(ser::Error::custom(
                "cut codec integer outside native JSON domain",
            ));
        }
        self.meter.pay::<Self::Error>(33)?;
        self.inner.serialize_i128(value)
    }
    fn serialize_u128(self, value: u128) -> Result<Self::Ok, Self::Error> {
        if value > u64::MAX as u128 {
            return Err(ser::Error::custom(
                "cut codec integer outside native JSON domain",
            ));
        }
        self.meter.pay::<Self::Error>(33)?;
        self.inner.serialize_u128(value)
    }
    fn serialize_f32(self, value: f32) -> Result<Self::Ok, Self::Error> {
        if !value.is_finite() {
            return Err(ser::Error::custom("cut codec nonfinite number"));
        }
        self.meter.pay::<Self::Error>(33)?;
        self.inner.serialize_f32(value)
    }
    fn serialize_f64(self, value: f64) -> Result<Self::Ok, Self::Error> {
        if !value.is_finite() {
            return Err(ser::Error::custom("cut codec nonfinite number"));
        }
        self.meter.pay::<Self::Error>(33)?;
        self.inner.serialize_f64(value)
    }
    fn serialize_bool(self, value: bool) -> Result<Self::Ok, Self::Error> {
        self.meter.pay::<Self::Error>(1)?;
        self.inner.serialize_bool(value)
    }
    fn serialize_str(self, value: &str) -> Result<Self::Ok, Self::Error> {
        self.meter.pay::<Self::Error>(
            value
                .len()
                .checked_add(1)
                .ok_or_else(|| ser::Error::custom("cut codec string overflow"))?,
        )?;
        self.inner.serialize_str(value)
    }
    fn serialize_char(self, value: char) -> Result<Self::Ok, Self::Error> {
        self.meter.pay::<Self::Error>(1 + value.len_utf8())?;
        self.inner.serialize_char(value)
    }
    fn serialize_bytes(self, value: &[u8]) -> Result<Self::Ok, Self::Error> {
        let work = value
            .len()
            .checked_mul(33)
            .and_then(|n| n.checked_add(1))
            .ok_or_else(|| ser::Error::custom("cut codec bytes overflow"))?;
        self.meter.pay::<Self::Error>(work)?;
        self.inner.serialize_bytes(value)
    }
    fn serialize_none(self) -> Result<Self::Ok, Self::Error> {
        self.serialize_unit()
    }
    fn serialize_some<T: ?Sized + Serialize>(self, value: &T) -> Result<Self::Ok, Self::Error> {
        self.inner.serialize_some(&Wrapped {
            value,
            meter: self.meter,
            depth: self.depth,
        })
    }
    fn serialize_unit(self) -> Result<Self::Ok, Self::Error> {
        self.meter.pay::<Self::Error>(1)?;
        self.inner.serialize_unit()
    }
    fn serialize_unit_struct(self, name: &'static str) -> Result<Self::Ok, Self::Error> {
        self.meter.pay::<Self::Error>(1)?;
        self.inner.serialize_unit_struct(name)
    }
    fn serialize_unit_variant(
        self,
        name: &'static str,
        index: u32,
        variant: &'static str,
    ) -> Result<Self::Ok, Self::Error> {
        self.meter.pay::<Self::Error>(1 + variant.len())?;
        self.inner.serialize_unit_variant(name, index, variant)
    }
    fn serialize_newtype_struct<T: ?Sized + Serialize>(
        self,
        name: &'static str,
        value: &T,
    ) -> Result<Self::Ok, Self::Error> {
        if name == "$serde_json::private::RawValue" || name == "$serde_json::private::Number" {
            return Err(ser::Error::custom(
                "opaque JSON is unsupported by prepaid cut codec",
            ));
        }
        self.inner.serialize_newtype_struct(
            name,
            &Wrapped {
                value,
                meter: self.meter,
                depth: self.depth,
            },
        )
    }
    fn serialize_newtype_variant<T: ?Sized + Serialize>(
        self,
        name: &'static str,
        index: u32,
        variant: &'static str,
        value: &T,
    ) -> Result<Self::Ok, Self::Error> {
        self.meter.pay::<Self::Error>(2 + variant.len())?;
        self.inner.serialize_newtype_variant(
            name,
            index,
            variant,
            &Wrapped {
                value,
                meter: self.meter,
                depth: self.depth + 1,
            },
        )
    }
    fn serialize_seq(self, len: Option<usize>) -> Result<Self::SerializeSeq, Self::Error> {
        self.meter.pay::<Self::Error>(1)?;
        Ok(Paid {
            inner: self.inner.serialize_seq(len)?,
            meter: self.meter,
            depth: self.depth + 1,
        })
    }
    fn serialize_tuple(self, len: usize) -> Result<Self::SerializeTuple, Self::Error> {
        self.meter.pay::<Self::Error>(1)?;
        Ok(Paid {
            inner: self.inner.serialize_tuple(len)?,
            meter: self.meter,
            depth: self.depth + 1,
        })
    }
    fn serialize_tuple_struct(
        self,
        name: &'static str,
        len: usize,
    ) -> Result<Self::SerializeTupleStruct, Self::Error> {
        self.meter.pay::<Self::Error>(1)?;
        Ok(Paid {
            inner: self.inner.serialize_tuple_struct(name, len)?,
            meter: self.meter,
            depth: self.depth + 1,
        })
    }
    fn serialize_tuple_variant(
        self,
        name: &'static str,
        index: u32,
        variant: &'static str,
        len: usize,
    ) -> Result<Self::SerializeTupleVariant, Self::Error> {
        self.meter.pay::<Self::Error>(3 + variant.len())?;
        Ok(Paid {
            inner: self
                .inner
                .serialize_tuple_variant(name, index, variant, len)?,
            meter: self.meter,
            depth: self.depth + 2,
        })
    }
    fn serialize_map(self, len: Option<usize>) -> Result<Self::SerializeMap, Self::Error> {
        self.meter.pay::<Self::Error>(1)?;
        Ok(Paid {
            inner: self.inner.serialize_map(len)?,
            meter: self.meter,
            depth: self.depth + 1,
        })
    }
    fn serialize_struct(
        self,
        name: &'static str,
        len: usize,
    ) -> Result<Self::SerializeStruct, Self::Error> {
        if name == "$serde_json::private::RawValue" || name == "$serde_json::private::Number" {
            return Err(ser::Error::custom(
                "opaque JSON is unsupported by prepaid cut codec",
            ));
        }
        self.meter.pay::<Self::Error>(1)?;
        Ok(Paid {
            inner: self.inner.serialize_struct(name, len)?,
            meter: self.meter,
            depth: self.depth + 1,
        })
    }
    fn serialize_struct_variant(
        self,
        name: &'static str,
        index: u32,
        variant: &'static str,
        len: usize,
    ) -> Result<Self::SerializeStructVariant, Self::Error> {
        self.meter.pay::<Self::Error>(3 + variant.len())?;
        Ok(Paid {
            inner: self
                .inner
                .serialize_struct_variant(name, index, variant, len)?,
            meter: self.meter,
            depth: self.depth + 2,
        })
    }
    fn collect_str<T: ?Sized + std::fmt::Display>(self, _: &T) -> Result<Self::Ok, Self::Error> {
        Err(ser::Error::custom(
            "unbounded Display is unsupported by prepaid cut codec",
        ))
    }
    fn is_human_readable(&self) -> bool {
        self.inner.is_human_readable()
    }
}
macro_rules! sequence {
    ($trait:ident, $method:ident) => {
        impl<S: ser::$trait, F: FnMut(usize) -> Result<(), String>> ser::$trait for Paid<'_, S, F> {
            type Ok = S::Ok;
            type Error = S::Error;
            fn $method<T: ?Sized + Serialize>(&mut self, value: &T) -> Result<(), Self::Error> {
                self.inner.$method(&Wrapped {
                    value,
                    meter: self.meter,
                    depth: self.depth,
                })
            }
            fn end(self) -> Result<Self::Ok, Self::Error> {
                self.inner.end()
            }
        }
    };
}
sequence!(SerializeSeq, serialize_element);
sequence!(SerializeTuple, serialize_element);
sequence!(SerializeTupleStruct, serialize_field);
sequence!(SerializeTupleVariant, serialize_field);
impl<S: ser::SerializeMap, F: FnMut(usize) -> Result<(), String>> ser::SerializeMap
    for Paid<'_, S, F>
{
    type Ok = S::Ok;
    type Error = S::Error;
    fn serialize_key<T: ?Sized + Serialize>(&mut self, key: &T) -> Result<(), Self::Error> {
        self.inner.serialize_key(&Wrapped {
            value: key,
            meter: self.meter,
            depth: self.depth,
        })
    }
    fn serialize_value<T: ?Sized + Serialize>(&mut self, value: &T) -> Result<(), Self::Error> {
        self.inner.serialize_value(&Wrapped {
            value,
            meter: self.meter,
            depth: self.depth,
        })
    }
    fn end(self) -> Result<Self::Ok, Self::Error> {
        self.inner.end()
    }
}
macro_rules! structure {
    ($trait:ident) => {
        impl<S: ser::$trait, F: FnMut(usize) -> Result<(), String>> ser::$trait for Paid<'_, S, F> {
            type Ok = S::Ok;
            type Error = S::Error;
            fn serialize_field<T: ?Sized + Serialize>(
                &mut self,
                key: &'static str,
                value: &T,
            ) -> Result<(), Self::Error> {
                self.meter.pay::<Self::Error>(1 + key.len())?;
                self.inner.serialize_field(
                    key,
                    &Wrapped {
                        value,
                        meter: self.meter,
                        depth: self.depth,
                    },
                )
            }
            fn skip_field(&mut self, key: &'static str) -> Result<(), Self::Error> {
                self.inner.skip_field(key)
            }
            fn end(self) -> Result<Self::Ok, Self::Error> {
                self.inner.end()
            }
        }
    };
}
structure!(SerializeStruct);
structure!(SerializeStructVariant);

/// Fund JSON cell visits, numeric formatting and UTF-8/escape scans before the
/// downstream serializer sees them. The separate writer still funds emitted
/// bytes. This covers audited built-in Serialize implementations, not arbitrary
/// work hidden in caller implementations, private allocator capacity or RSS.
pub fn write<T: Serialize>(
    output: &mut dyn Write,
    value: &T,
    charge: impl FnMut(usize) -> Result<(), String>,
) -> Result<usize, String> {
    let meter = Meter {
        charge: RefCell::new(charge),
        work: Cell::new(0),
    };
    serde_json::to_writer(
        output,
        &Wrapped {
            value,
            meter: &meter,
            depth: 0,
        },
    )
    .map_err(|error| error.to_string())?;
    Ok(meter.work.get())
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct WriteWork {
    pub traversal_work: usize,
    pub encoded_bytes: usize,
    /// Traversal plus two work units per emitted byte. The caller supplies
    /// the complete remaining budget; neither helper grants an allowance.
    pub total_work: usize,
}

struct FundedOutput<'a, F> {
    output: &'a mut dyn Write,
    meter: &'a Meter<F>,
    bytes: usize,
}
impl<F: FnMut(usize) -> Result<(), String>> Write for FundedOutput<'_, F> {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        // Large string writes cannot defer a cancellation/work check until
        // after forwarding the full string to a buffered/output writer.
        for chunk in bytes.chunks(32768) {
            let work = chunk
                .len()
                .checked_mul(2)
                .ok_or_else(|| std::io::Error::other("prepaid output work overflow"))?;
            self.meter
                .pay::<serde_json::Error>(work)
                .map_err(std::io::Error::other)?;
            self.output.write_all(chunk)?;
            self.bytes = self
                .bytes
                .checked_add(chunk.len())
                .ok_or_else(|| std::io::Error::other("prepaid output byte overflow"))?;
        }
        Ok(bytes.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        self.meter
            .pay::<serde_json::Error>(0)
            .map_err(std::io::Error::other)?;
        self.output.flush()
    }
}

/// Fund the same audited traversal AND every emitted byte before forwarding.
/// No intermediate document, size walk, Value or work hidden in an arbitrary
/// caller Serialize implementation is included by implication.
pub fn write_with_output_charge<T: Serialize>(
    output: &mut dyn Write,
    value: &T,
    charge: impl FnMut(usize) -> Result<(), String>,
) -> Result<WriteWork, String> {
    let meter = Meter {
        charge: RefCell::new(charge),
        work: Cell::new(0),
    };
    let mut output = FundedOutput {
        output,
        meter: &meter,
        bytes: 0,
    };
    serde_json::to_writer(
        &mut output,
        &Wrapped {
            value,
            meter: &meter,
            depth: 0,
        },
    )
    .map_err(|e| e.to_string())?;
    let encoded_bytes = output.bytes;
    let output_work = encoded_bytes
        .checked_mul(2)
        .ok_or("prepaid output work overflow")?;
    let total_work = meter.work.get();
    Ok(WriteWork {
        traversal_work: total_work
            .checked_sub(output_work)
            .ok_or("prepaid output work differs")?,
        encoded_bytes,
        total_work,
    })
}

/// Diagnostic-only conversion of one audited built-in projection. Every cell,
/// key and scalar is funded before the Value serializer sees it. The caller
/// must separately retain the existing source/Value component allowance and
/// drop this temporary before moving to the next source. This does not prove
/// allocator layout or grant any publication/admission authority.
pub fn diagnostic_value<T: Serialize>(
    value: &T,
    charge: impl FnMut(usize) -> Result<(), String>,
) -> Result<(serde_json::Value, usize), String> {
    let meter = Meter {
        charge: RefCell::new(charge),
        work: Cell::new(0),
    };
    let result = Wrapped {
        value,
        meter: &meter,
        depth: 0,
    }
    .serialize(serde_json::value::Serializer)
    .map_err(|error| error.to_string())?;
    Ok((result, meter.work.get()))
}

#[cfg(test)]
#[path = "prepaid_json_tests.rs"]
mod tests;
