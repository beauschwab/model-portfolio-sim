"""Application composition for durable dispatch, preserving persisted names.

Resolution is an allowlist, never arbitrary getattr/import of a queued name.
Storage transports these names and has no knowledge of feature implementations.
The worker resolves them at its execution boundary. Facade functions are looked
up at call time so test substitution and module startup order remain supported.
"""


def registered():
    from . import store, decision_store, cohort_store, treasury_store
    handlers = {"store." + name: getattr(store, name) for name in (
        "run_pricing", "run_risk_all", "run_stress_all", "run_kpis_scenario", "run_nii",
        "build_unitlib_job", "run_strategy_job", "run_optimize_job", "run_scenario_grid",
        "run_forecast", "run_balance_stress", "run_streamed_balance_stress",
        "run_saved_balance_stress", "fetch_research_data", "run_treasury")}
    handlers.update({"decision.build": decision_store.build, "decision.update": decision_store.update,
                     "treasury.run": treasury_store.run})
    handlers.update({'cohort.' + name: getattr(cohort_store, name)
                     for name in ('import_tape', 'build', 'analytics', 'attribution', 'audit')})
    return handlers


def operation(fn):
    for name, handler in registered().items():
        if handler is fn:
            return name
    raise ValueError("function is not registered for durable execution")


def resolve(name):
    try:
        return registered()[name]
    except (KeyError, TypeError) as exc:
        raise ValueError("unregistered operation") from exc
