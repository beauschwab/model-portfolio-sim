//! Deployment configuration. One process serves one explicitly scoped workspace; the same
//! variables as the Python service select the database, objects and scope.
use std::path::PathBuf;

#[derive(Debug, Clone)]
pub struct Config {
    pub environment: String,
    pub database_url: String,
    pub artifact_url: String,
    pub tenant: String,
    pub workspace: String,
    pub api_token: Option<String>,
    /// The single browser origin allowed to call mutating routes (same-origin gateway).
    pub allowed_origin: Option<String>,
    pub bind: String,
    /// The native producer: the `portfolio-model-ledger` executable.
    pub ledger_exe: PathBuf,
    pub lease_seconds: f64,
    pub default_deadline_seconds: f64,
}

fn var(name: &str) -> Option<String> {
    std::env::var(name).ok().filter(|v| !v.is_empty())
}

impl Config {
    pub fn from_env() -> Result<Config, String> {
        let root = PathBuf::from(var("WORKBENCH_ROOT").unwrap_or_else(|| ".".into()));
        let data = root.join(".data").join("workbench");
        let environment = var("WORKBENCH_ENV").unwrap_or_else(|| "development".into());
        let config = Config {
            database_url: var("DATABASE_URL").unwrap_or_else(|| {
                format!(
                    "sqlite:///{}",
                    data.join("workbench.db")
                        .to_string_lossy()
                        .replace('\\', "/")
                )
            }),
            artifact_url: var("ARTIFACT_URL")
                .unwrap_or_else(|| data.join("artifacts").to_string_lossy().into_owned()),
            tenant: var("WORKBENCH_TENANT_ID").unwrap_or_else(|| "dev".into()),
            workspace: var("WORKBENCH_WORKSPACE_ID").unwrap_or_else(|| "example".into()),
            api_token: var("WORKBENCH_API_TOKEN"),
            allowed_origin: var("WORKBENCH_ALLOWED_ORIGIN"),
            bind: var("WORKBENCH_NATIVE_BIND").unwrap_or_else(|| "127.0.0.1:8003".into()),
            // Admin commands need no producer; admission and execution refuse without one.
            ledger_exe: PathBuf::from(var("PORTFOLIO_MODEL_LEDGER_EXE").unwrap_or_default()),
            lease_seconds: var("WORKBENCH_LEASE_SECONDS")
                .map(|v| {
                    v.parse::<f64>()
                        .map_err(|e| format!("WORKBENCH_LEASE_SECONDS: {e}"))
                })
                .transpose()?
                .unwrap_or(60.)
                .max(1.),
            default_deadline_seconds: var("WORKBENCH_NATIVE_DEADLINE_SECONDS")
                .map(|v| {
                    v.parse::<f64>()
                        .map_err(|e| format!("WORKBENCH_NATIVE_DEADLINE_SECONDS: {e}"))
                })
                .transpose()?
                .unwrap_or(900.),
            environment,
        };
        if config.environment == "production" {
            if config.api_token.is_none() {
                return Err("production requires WORKBENCH_API_TOKEN".into());
            }
            if config.tenant == "dev" || config.workspace == "example" {
                return Err("production requires explicit tenant and workspace IDs".into());
            }
        }
        for (name, value) in [("tenant", &config.tenant), ("workspace", &config.workspace)] {
            if value.is_empty() || value.len() > 128 {
                return Err(format!("{name} identifiers are 1-128 characters"));
            }
        }
        Ok(config)
    }
}
