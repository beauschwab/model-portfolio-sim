"""Private commercial SQL guard over independently verified uploaded objects."""


def prepare_publication(request,output,*,payload_identity,expected_deadline,job,stopping):
    # The established guard owns bounded descriptors, checks every object and
    # fences the raw queued authority, executable and lease inside the SQL lock.
    from .deposit_continuation_jobs import prepare_publication as prepare
    return prepare(request,output,payload_identity=payload_identity,
        expected_deadline=expected_deadline,job=job,stopping=stopping,
        selected_schemas=frozenset({'commercial-credit-ledger-1'}))
