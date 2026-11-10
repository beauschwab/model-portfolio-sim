//! Source identity for the unreleased consumer macro contract, independent of JSON float text.
use serde::Serialize;
use serde_json::Value;
use sha2::{Digest, Sha256};

pub const POLICY: &str = "consumer-macro-typed-binary64-1";

pub fn digest(
    value: &impl Serialize,
    check: &mut dyn FnMut() -> Result<(), String>,
) -> Result<String, String> {
    check()?;
    let value = serde_json::to_value(value).map_err(|e| e.to_string())?;
    let mut sha = Sha256::new();
    sha.update(POLICY.as_bytes());
    fn length(sha: &mut Sha256, n: usize) {
        sha.update((n as u64).to_le_bytes());
    }
    fn walk(
        value: &Value,
        sha: &mut Sha256,
        check: &mut dyn FnMut() -> Result<(), String>,
    ) -> Result<(), String> {
        check()?;
        match value {
            Value::Null => sha.update(b"n"),
            Value::Bool(b) => sha.update(if *b { b"t" } else { b"f" }),
            Value::String(s) => {
                sha.update(b"s");
                length(sha, s.len());
                sha.update(s.as_bytes());
            }
            Value::Number(n) if n.is_f64() => {
                let x = n.as_f64().ok_or("macro float identity")?;
                if !x.is_finite() {
                    return Err("macro nonfinite identity".into());
                }
                sha.update(b"d");
                sha.update(x.to_bits().to_le_bytes());
            }
            Value::Number(n) => {
                if let Some(x) = n.as_i64() {
                    sha.update(b"i");
                    sha.update(x.to_le_bytes());
                } else {
                    sha.update(b"u");
                    sha.update(n.as_u64().ok_or("macro integer identity")?.to_le_bytes());
                }
            }
            Value::Array(a) => {
                sha.update(b"a");
                length(sha, a.len());
                for v in a {
                    walk(v, sha, check)?;
                }
            }
            Value::Object(o) => {
                sha.update(b"o");
                length(sha, o.len());
                let mut keys: Vec<_> = o.keys().collect();
                keys.sort();
                for k in keys {
                    walk(&Value::String(k.clone()), sha, check)?;
                    walk(&o[k], sha, check)?;
                }
            }
        }
        Ok(())
    }
    walk(&value, &mut sha, check)?;
    Ok(format!("{:x}", sha.finalize()))
}

#[cfg(test)]
mod tests {
    fn digest(value: &impl serde::Serialize) -> Result<String, String> {
        super::digest(value, &mut || Ok(()))
    }
    #[test]
    fn typed_identity_preserves_signed_zero_and_integer_precision() {
        assert_ne!(digest(&0.0).unwrap(), digest(&-0.0).unwrap());
        assert_ne!(digest(&1_u64).unwrap(), digest(&1.0).unwrap());
        assert_ne!(
            digest(&(1_u64 << 63)).unwrap(),
            digest(&((1_u64 << 63) + 1)).unwrap()
        );
        assert_eq!(
            digest(&serde_json::json!({"z":1,"a":2})).unwrap(),
            digest(&serde_json::json!({"a":2,"z":1})).unwrap()
        );
    }
    #[test]
    fn typed_source_hash_retains_the_callers_control_inside_the_walk() {
        let mut calls = 0;
        let e = super::digest(&vec![1_u64; 100], &mut || {
            calls += 1;
            if calls == 8 {
                Err("original source cancellation".into())
            } else {
                Ok(())
            }
        })
        .unwrap_err();
        assert_eq!(e, "original source cancellation");
        assert_eq!(calls, 8);
    }
}
