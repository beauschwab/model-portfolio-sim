use super::*;
struct Pending {
    token: u64,
    library: PreparedLibrary,
    result: Value,
}
/// Native actor transaction. Retained coefficients are immutable and shared
/// with a pending constraint-only update until explicit publish or abort.
pub struct Session {
    version: u64,
    nonce: u64,
    library: PreparedLibrary,
    pending: Option<Pending>,
}
impl Session {
    pub fn new(value: &Value) -> Result<(Self, Value), String> {
        let mut raw = value.clone();
        raw["op"] = json!("optimize_product");
        let library = PreparedLibrary::new(&raw)?;
        let snapshot = library.snapshot()?;
        Ok((
            Self {
                version: 0,
                nonce: 0,
                library,
                pending: None,
            },
            snapshot,
        ))
    }
    pub fn call(&mut self, value: Value) -> Result<Value, String> {
        if value["version"].as_u64() != Some(self.version) {
            return Err("stale product session version".into());
        }
        match value["op"].as_str().ok_or("missing product operation")? {
            "status" => Ok(
                json!({"schema":SCHEMA,"version":self.version,"pending":self.pending.is_some(),"snapshot":self.library.snapshot()?}),
            ),
            "eval" => {
                let allocation: Vec<Allocation> =
                    serde_json::from_value(value["allocation"].clone())
                        .map_err(|e| e.to_string())?;
                let mut result = self.library.evaluate(&allocation)?;
                result["version"] = json!(self.version);
                result["work"] =
                    json!({"units_rebuilt":0,"full_rebuild":false,"coefficient_only":true});
                Ok(result)
            }
            "solve" => {
                let mut result = self.library.solve()?;
                result["version"] = json!(self.version);
                result["work"] = json!({"units_rebuilt":0,"full_rebuild":false});
                Ok(result)
            }
            "update_product" => {
                if self.pending.is_some() {
                    return Err("product update already pending".into());
                }
                let mut r =
                    serde_json::to_value(&self.library.request).map_err(|e| e.to_string())?;
                for (key, v) in value.as_object().ok_or("product update object required")? {
                    if ["op", "handle", "version"].contains(&key.as_str()) {
                        continue;
                    }
                    if ![
                        "product",
                        "bases",
                        "constraints",
                        "face_limits",
                        "capital_dependency",
                        "max_purchase_cost_assets",
                    ]
                    .contains(&key.as_str())
                    {
                        return Err(format!("unsupported product update field {key}"));
                    }
                    r[key] = v.clone();
                }
                let request: Request = serde_json::from_value(r).map_err(|e| e.to_string())?;
                let same = serde_json::to_value(&request.product).map_err(|e| e.to_string())?
                    == serde_json::to_value(&self.library.request.product)
                        .map_err(|e| e.to_string())?;
                let (retained, units_rebuilt, full_rebuild) = if same {
                    (Arc::clone(&self.library.prepared), 0, false)
                } else {
                    let (prepared, rebuilt, full) = product_strategy::prepare_update(
                        &request.product, &self.library.prepared,
                    )?;
                    (Arc::new(prepared), rebuilt, full)
                };
                let mut library = PreparedLibrary::build(request, Some(retained))?;
                let mut result = library.solve()?;
                result["product_preparation"] =
                    serde_json::to_value(library.preparation()).map_err(|e| e.to_string())?;
                self.nonce = self.nonce.checked_add(1).ok_or("product token exhausted")?;
                result["token"] = json!(self.nonce);
                result["version"] = json!(self.version);
                result["work"] = json!({"full_rebuild":full_rebuild,"units_rebuilt":units_rebuilt,
                    "selective_rebuild":!same && !full_rebuild,"coefficient_only":same});
                self.pending = Some(Pending {
                    token: self.nonce,
                    library,
                    result: result.clone(),
                });
                Ok(result)
            }
            "publish" | "abort" => {
                let pending = self.pending.as_ref().ok_or("no product update pending")?;
                if value["token"].as_u64() != Some(pending.token) {
                    return Err("stale product transaction token".into());
                }
                if value["op"] == "abort" {
                    self.pending = None;
                    return Ok(json!({"aborted":true,"version":self.version}));
                }
                let version = self
                    .version
                    .checked_add(1)
                    .ok_or("product version exhausted")?;
                let pending = self.pending.take().unwrap();
                self.library = pending.library;
                self.version = version;
                let mut result = pending.result;
                result["version"] = json!(version);
                Ok(result)
            }
            _ => Err("unsupported product session operation".into()),
        }
    }
}
