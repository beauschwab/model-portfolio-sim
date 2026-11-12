import pytest

from workbench_mcp import analyst
from workbench_mcp.adapter import catalog


def test_embedded_tools_are_exact_reads_without_agent_recursion():
    reviewed = analyst.operations()
    assert set(reviewed) == analyst.TOOLS and len(reviewed) == 15
    assert all(op['method'] == 'GET' and op['permission'] == 'read' for op in reviewed.values())
    assert not any(op['path'].startswith('/analysts') for op in catalog()['operations'].values())


@pytest.mark.parametrize('change', ['missing', 'write'])
def test_catalog_drift_fails_closed(monkeypatch, change):
    operations = {name: dict(value) for name, value in analyst.operations().items()}
    if change == 'missing':
        del operations['get_state']
    else:
        operations['get_state']['permission'] = 'edit'
    monkeypatch.setattr(analyst, 'catalog', lambda: {'operations': operations})
    with pytest.raises(RuntimeError): analyst.operations()
