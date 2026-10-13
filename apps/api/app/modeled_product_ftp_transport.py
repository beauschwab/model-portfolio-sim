"""Bounded typed retention of already verified native source evidence; no finance."""
import polars as pl

JOURNAL = {"source_id":pl.String,"transaction_number":pl.Int64,"scenario":pl.String,
    "day":pl.Int64,"account":pl.String,"event":pl.String,"gl_account":pl.String,
    "instrument_id":pl.String,"value":pl.Float64}
COMPLETION = {"source_id":pl.String,"wire":pl.String,"complete":pl.Boolean,"protocol":pl.Int64}
STRINGS = ("scenario","account","event","gl_account","instrument_id")
MAX_ROWS = 250000

def source_tables(result,check=lambda:False):
    rows=[];completion=[]
    def probe():
        if check():raise InterruptedError("FTP source retention cancelled")
    def append(source_id,row):
        if set(row)!=set(JOURNAL)-{'source_id'}:raise ValueError('native source journal shape changed')
        if len(rows)>=MAX_ROWS:raise ValueError('bounded retained FTP journal rows')
        # Dictionaries are decoded without arithmetic, netting or reorder.
        rows.append(dict(source_id=source_id,**row))
    for source in result['result']['source_preparation']['sources']:
        probe();source_id=source['source_id'];native=source['raw_native_result']
        if 'ledger_messages'in native:
            dictionary=[];messages=native['ledger_messages'];done=False
            for message in messages:
                probe()
                if done:raise ValueError('native source data after completion')
                if 'complete'in message:
                    if set(message)!={'complete','protocol'}or type(message['complete'])is not bool or message['complete']is not True or type(message['protocol'])is not int or message['protocol']!=3:
                        raise ValueError('actual native source completion required')
                    done=True;continue
                dictionary.extend(message.get('dictionary',[]))
                if message.get('table')!='journal':continue
                if 'rows'in message:
                    for row in message['rows']:append(source_id,row)
                    continue
                columns=message['columns']
                if set(columns)!=set(JOURNAL)-{'source_id'}:raise ValueError('native journal columns changed')
                n=len(columns['value'])
                if any(len(v)!=n for v in columns.values()):raise ValueError('native journal column lengths changed')
                for i in range(n):
                    if i%256==0:probe()
                    row={k:v[i]for k,v in columns.items()}
                    for k in STRINGS:
                        key=row[k]
                        if type(key)is not int or not 0<=key<len(dictionary):raise ValueError('native journal dictionary identity changed')
                        row[k]=dictionary[key]
                    append(source_id,row)
            if not done:raise ValueError('incomplete native source stream')
            wire='actual_protocol3_completion_message'
        else:
            if type(native.get('complete'))is not bool or native['complete']is not True or type(native.get('protocol'))is not int or native['protocol']!=3:
                raise ValueError('complete native typed source required')
            for i,row in enumerate(native['journal']):
                if i%256==0:probe()
                append(source_id,row)
            wire='actual_typed_result_completion_fields'
        completion.append(dict(source_id=source_id,wire=wire,complete=True,protocol=3))
    probe()
    return dict(source_journal=pl.DataFrame(rows,schema=JOURNAL,strict=True),
        source_completion=pl.DataFrame(completion,schema=COMPLETION,strict=True))
