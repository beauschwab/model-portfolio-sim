"""Historical flat imports. Canonical internal imports use the layered package.

Aliases retain module identity (including monkeypatches) for existing consumers.
This is import compatibility only, never a production financial fallback.
"""
import importlib
import sys

ALIASES = {
    **{name: 'core.' + name for name in ('config', 'conventions', 'curve', 'interfaces',
       'kernels', 'lmm', 'pricing', 'scenarios', 'vol')},
    'models': 'models.models', 'prepay': 'models.prepay',
    **{name: 'products.' + name for name in ('cds', 'corp', 'deposits', 'hedges', 'mm')},
    **{name: 'analytics.' + name for name in ('accounting', 'kpis', 'risk', 'stress')},
    **{name: 'strategy.' + name for name in ('optimizer', 'strategies', 'unitlib')},
}


def install(package):
    for alias, canonical in ALIASES.items():
        module = importlib.import_module('.' + canonical, package)
        sys.modules[package + '.' + alias] = module
