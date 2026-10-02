"""Deployment configuration; a process serves one explicitly scoped workspace."""
from dataclasses import dataclass
import os
from pathlib import Path

ROOT = Path(__file__).resolve().parents[3]


@dataclass(frozen=True)
class StorageConfig:
    database_url: str
    artifact_url: str
    tenant: str = "dev"
    workspace: str = "example"
    execution: str = "external"
    worker_url: str = "http://127.0.0.1:8002"
    worker_token: str = ""
    api_token: str = ""
    seed_demo: bool = True

    @classmethod
    def from_env(cls):
        folder = ROOT / ".data" / "workbench"
        environment = os.getenv("WORKBENCH_ENV", "development")
        config = cls(
            database_url=os.getenv("DATABASE_URL", f"sqlite:///{(folder / 'workbench.db').as_posix()}"),
            artifact_url=os.getenv("ARTIFACT_URL", str(folder / "artifacts")),
            tenant=os.getenv("WORKBENCH_TENANT_ID", "dev"),
            workspace=os.getenv("WORKBENCH_WORKSPACE_ID", "example"),
            execution=os.getenv("WORKBENCH_EXECUTION", "external"),
            worker_url=os.getenv("WORKBENCH_WORKER_URL", "http://127.0.0.1:8002").rstrip("/"),
            worker_token=os.getenv("WORKBENCH_WORKER_TOKEN", ""),
            api_token=os.getenv("WORKBENCH_API_TOKEN", ""),
            seed_demo=os.getenv("WORKBENCH_SEED_DEMO", "1" if environment == "development" else "0") == "1",
        )
        if config.execution not in {"external", "memory"}:
            raise ValueError("WORKBENCH_EXECUTION must be external or memory (tests only)")
        if environment == "production":
            if config.execution != "external" or not config.worker_token or not config.api_token:
                raise ValueError("Production requires external workers and API/worker credentials")
            if config.tenant == "dev" or config.workspace == "example":
                raise ValueError("Production requires explicit tenant and workspace IDs")
        return config
