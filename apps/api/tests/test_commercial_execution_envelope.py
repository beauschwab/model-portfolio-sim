"""A retained millisecond cannot lose precision or enlarge its original budget."""
import pytest
from app.commercial_execution_envelope import retained_command


@pytest.mark.parametrize('schema,op',[
    ('commercial-holding-journal-1','analyze_commercial_holding_journal'),
    ('commercial-candidate-journal-1','analyze_commercial_candidate_journal'),
    ('commercial-treasury-1','analyze_commercial_treasury')])
@pytest.mark.parametrize('milliseconds',[1001,2003,8011,1800000])
def test_retained_commercial_millisecond_is_exact(schema,op,milliseconds):
    raw={'schema':schema}
    assert retained_command(schema,raw,1800.,milliseconds)==dict(op=op,schema=schema,execution_timeout_ms=milliseconds,request=raw)


@pytest.mark.parametrize('milliseconds',[True,0,1800001,1001.])
def test_retained_commercial_budget_does_not_accept_coercion_or_extension(milliseconds):
    with pytest.raises(ValueError):retained_command('commercial-holding-journal-1',{'schema':'commercial-holding-journal-1'},1800.,milliseconds)
