"""Selected deposit transport admission; no financial authority or RSS claim."""
import io

LIMIT=32*1024*1024


def parquet_metadata(data, rows, schema, *, decoded, logical, check):
    """Refuse excessive declared decoded column chunks before table decoding."""
    import pyarrow.parquet as pq
    import polars as pl
    check()
    metadata=pq.read_metadata(io.BytesIO(data))
    if metadata.num_rows!=rows:
        raise ValueError('deposit Parquet metadata rows')
    uncompressed=0
    for group_index in range(metadata.num_row_groups):
        check()
        group=metadata.row_group(group_index)
        for column_index in range(group.num_columns):
            check()
            size=group.column(column_index).total_uncompressed_size
            if type(size) is not int or size<0:
                raise ValueError('deposit invalid decoded column size')
            uncompressed+=size
            if uncompressed>LIMIT or decoded+uncompressed>LIMIT:
                raise ValueError('deposit decoded column-component byte cap')
    if {k:str(v) for k,v in pl.scan_parquet(io.BytesIO(data)).collect_schema().items()}!=schema:
        raise ValueError('deposit Parquet metadata schema')
    # Encoded dictionary pages can be small while repeated UTF8 materializes
    # many times. Admit logical Arrow rows separately, one row at a time.
    # This bounds retained decoded components, not all decoder/transient RSS.
    for batch in pq.ParquetFile(io.BytesIO(data)).iter_batches(batch_size=1):
        check()
        logical+=batch.nbytes
        if logical>LIMIT:raise ValueError('deposit logical decoded component byte cap')
        check()
    return decoded+uncompressed,logical
