//! Immutable, checksummed, content-addressed objects with the exact layout of
//! `apps/api/app/artifacts.py`: `workbench/<sha256(tenant \0 workspace)>/<d[:2]>/<d>.<format>`,
//! descriptors `{key, sha256, bytes, format}`, formats `json`, `parquet` or `bin`. Every read
//! checks scope, the declared length before transport and the SHA-256 after it. Local files and,
//! with the `s3` feature, S3-compatible buckets are supported; no request chooses the prefix.
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::path::PathBuf;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Descriptor {
    pub key: String,
    pub sha256: String,
    pub bytes: u64,
    pub format: String,
}

impl Descriptor {
    pub fn to_json(&self) -> Value {
        json!({"key": self.key, "sha256": self.sha256, "bytes": self.bytes, "format": self.format})
    }
    pub fn from_json(v: &Value) -> Result<Descriptor, String> {
        let obj = v.as_object().ok_or("object descriptor is not an object")?;
        if obj.len() != 4 {
            return Err("object descriptor has unexpected fields".into());
        }
        let s = |k: &str| {
            obj.get(k)
                .and_then(Value::as_str)
                .map(str::to_string)
                .ok_or_else(|| format!("object descriptor `{k}` missing"))
        };
        Ok(Descriptor {
            key: s("key")?,
            sha256: s("sha256")?,
            bytes: obj
                .get("bytes")
                .and_then(Value::as_u64)
                .ok_or("object descriptor `bytes` missing")?,
            format: s("format")?,
        })
    }
}

enum Backend {
    Local(PathBuf),
    #[cfg(feature = "s3")]
    S3 {
        store: object_store::aws::AmazonS3,
        runtime: tokio::runtime::Runtime,
    },
}

pub struct Objects {
    pub prefix: String,
    backend: Backend,
}

pub fn workspace_prefix(tenant: &str, workspace: &str) -> String {
    let mut h = Sha256::new();
    h.update(tenant.as_bytes());
    h.update([0u8]);
    h.update(workspace.as_bytes());
    format!("workbench/{}", crate::hex(&h.finalize()))
}

fn valid_digest(d: &str) -> bool {
    d.len() == 64
        && d.bytes()
            .all(|c| c.is_ascii_digit() || (b'a'..=b'f').contains(&c))
}

impl Objects {
    pub fn open(url: &str, tenant: &str, workspace: &str) -> Result<Objects, String> {
        let prefix = workspace_prefix(tenant, workspace);
        if let Some(rest) = url.strip_prefix("s3://") {
            #[cfg(feature = "s3")]
            {
                let (bucket, path) = rest.split_once('/').unwrap_or((rest, ""));
                let prefix = [path.trim_matches('/'), prefix.as_str()]
                    .iter()
                    .filter(|p| !p.is_empty())
                    .copied()
                    .collect::<Vec<_>>()
                    .join("/");
                let mut builder =
                    object_store::aws::AmazonS3Builder::from_env().with_bucket_name(bucket);
                if let Ok(endpoint) = std::env::var("S3_ENDPOINT_URL") {
                    builder = builder
                        .with_endpoint(&endpoint)
                        .with_allow_http(endpoint.starts_with("http://"))
                        .with_virtual_hosted_style_request(false);
                }
                let store = builder
                    .build()
                    .map_err(|e| format!("S3 configuration: {e}"))?;
                let runtime = tokio::runtime::Builder::new_current_thread()
                    .enable_all()
                    .build()
                    .map_err(|e| e.to_string())?;
                return Ok(Objects {
                    prefix,
                    backend: Backend::S3 { store, runtime },
                });
            }
            #[cfg(not(feature = "s3"))]
            {
                let _ = rest;
                return Err("S3 artifact storage needs a build with the `s3` feature".into());
            }
        }
        let root = PathBuf::from(url.strip_prefix("file://").unwrap_or(url));
        std::fs::create_dir_all(&root).map_err(|e| format!("artifact root: {e}"))?;
        let root = root.canonicalize().map_err(|e| e.to_string())?;
        Ok(Objects {
            prefix,
            backend: Backend::Local(root),
        })
    }

    fn expected_key(&self, sha256: &str, format: &str) -> String {
        format!("{}/{}/{}.{}", self.prefix, &sha256[..2], sha256, format)
    }

    pub fn put(&self, data: &[u8], format: &str) -> Result<Descriptor, String> {
        if !["json", "parquet", "bin"].contains(&format) {
            return Err("unsupported artifact format".into());
        }
        let digest = crate::sha256_hex(data);
        let key = self.expected_key(&digest, format);
        match &self.backend {
            Backend::Local(root) => {
                let target = root.join(&key);
                let parent = target.parent().expect("object key has a parent");
                std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
                // Content addressing makes an intact existing object reusable. A damaged one (its
                // bytes no longer hash to its key) is atomically replaced with the true bytes,
                // which restores it without changing what any publication references.
                let intact = std::fs::read(&target)
                    .map(|b| b.len() == data.len() && crate::sha256_hex(&b) == digest)
                    .unwrap_or(false);
                if !intact {
                    let temp = parent.join(format!(".{}.{}.tmp", digest, uuid::Uuid::new_v4()));
                    let written = (|| -> std::io::Result<()> {
                        use std::io::Write;
                        let mut f = std::fs::File::create(&temp)?;
                        f.write_all(data)?;
                        f.sync_all()?;
                        Ok(())
                    })();
                    if let Err(e) = written {
                        let _ = std::fs::remove_file(&temp);
                        return Err(format!("object write failed: {e}"));
                    }
                    if let Err(e) = std::fs::rename(&temp, &target) {
                        let _ = std::fs::remove_file(&temp);
                        // A concurrent writer of the same content is fine; anything else is not.
                        let now_intact = std::fs::read(&target)
                            .map(|b| crate::sha256_hex(&b) == digest)
                            .unwrap_or(false);
                        if !now_intact {
                            return Err(format!("object publish failed: {e}"));
                        }
                    }
                }
            }
            #[cfg(feature = "s3")]
            Backend::S3 { store, runtime } => {
                use object_store::ObjectStore;
                let path = object_store::path::Path::from(key.as_str());
                let mut attributes = object_store::Attributes::new();
                attributes.insert(
                    object_store::Attribute::Metadata("sha256".into()),
                    digest.clone().into(),
                );
                let options = object_store::PutOptions {
                    attributes,
                    ..Default::default()
                };
                runtime
                    .block_on(store.put_opts(
                        &path,
                        bytes::Bytes::copy_from_slice(data).into(),
                        options,
                    ))
                    .map_err(|e| format!("S3 put failed: {e}"))?;
            }
        }
        Ok(Descriptor {
            key,
            sha256: digest,
            bytes: data.len() as u64,
            format: format.into(),
        })
    }

    /// Reads at most `maximum` bytes for a descriptor in this workspace and checks its identity.
    pub fn get_bounded(&self, d: &Descriptor, maximum: u64) -> Result<Vec<u8>, String> {
        if !valid_digest(&d.sha256)
            || d.key != self.expected_key(&d.sha256, &d.format)
            || !["json", "parquet", "bin"].contains(&d.format.as_str())
        {
            return Err("artifact is outside this workspace or has an invalid identity".into());
        }
        if d.bytes > maximum {
            return Err("artifact exceeds its bounded read".into());
        }
        let data = match &self.backend {
            Backend::Local(root) => {
                let path = root.join(&d.key);
                let size = std::fs::metadata(&path)
                    .map_err(|e| format!("artifact missing: {e}"))?
                    .len();
                if size != d.bytes {
                    return Err("artifact file length differs from its descriptor".into());
                }
                use std::io::Read;
                let mut data = Vec::with_capacity(size as usize);
                std::fs::File::open(&path)
                    .map_err(|e| e.to_string())?
                    .take(maximum + 1)
                    .read_to_end(&mut data)
                    .map_err(|e| e.to_string())?;
                data
            }
            #[cfg(feature = "s3")]
            Backend::S3 { store, runtime } => {
                use object_store::ObjectStore;
                let path = object_store::path::Path::from(d.key.as_str());
                runtime
                    .block_on(async {
                        let got = store.get(&path).await?;
                        if got.meta.size != d.bytes {
                            return Err(object_store::Error::Generic {
                                store: "S3",
                                source: "artifact content length differs from its descriptor"
                                    .into(),
                            });
                        }
                        got.bytes().await
                    })
                    .map_err(|e| format!("S3 get failed: {e}"))?
                    .to_vec()
            }
        };
        if data.len() as u64 != d.bytes || crate::sha256_hex(&data) != d.sha256 {
            return Err("artifact checksum mismatch".into());
        }
        Ok(data)
    }
}
