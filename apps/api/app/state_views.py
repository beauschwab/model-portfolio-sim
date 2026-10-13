"""Restore the sole worker-local materialization from a durable revision."""
from .contracts import WorkspaceSnapshot


def restore(owner, state: WorkspaceSnapshot) -> None:
    for field, name in (("tapes", "TAPES"), ("cohort_publications", "COHORT_PUBLICATIONS"),
                        ("books", "BOOKS"), ("market", "MARKET"), ("scenarios", "SCENARIOS"),
                        ("programs", "PROGRAMS"), ("assumptions", "ASSUMPTIONS")):
        target = getattr(owner, name)
        target.clear()
        target.update(state.get(field, {}) if field in ('tapes', 'cohort_publications') else state[field])
    for field, name in (("settings", "SETTINGS"), ("dep_hist", "DEP_HIST"), ("mbs_hists", "MBS_HISTS"),
                        ("asof", "ASOF"), ("equity", "EQUITY"), ("hedges", "HEDGES")):
        setattr(owner, name, state[field])
    owner.STATE_META["revision"] = state["revision"]
    owner.CACHE.clear()
    owner.UNITLIB = owner.BASE_KPIS = None

