"""Select explicit commercial transport without altering historical model routes."""
def is_commercial_cohort(schema,raw):
    if schema!='graph-cohort-audit-1' or type(raw)is not dict:return False
    cohorts=raw.get('cohorts')
    if type(cohorts)is not list:return False
    for cohort in cohorts:
        if type(cohort)is not dict:continue
        members=cohort.get('members');rows=[cohort.get('representative')]
        if type(members)is list:rows+=members
        for row in rows:
            if type(row)is dict and type(row.get('graph'))is dict and row['graph'].get('kind')=='commercial':return True
    return False
