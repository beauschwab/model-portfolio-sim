"""Initialize schemas and move typed input snapshots between deployments."""
import argparse
from dataclasses import replace
import json
from pathlib import Path

from . import persistence as db, store
from .artifacts import Objects, Codec
from .repository import Repository
from .storage_config import StorageConfig


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('command', choices=['migrate', 'init-demo', 'export', 'import'])
    parser.add_argument('directory', nargs='?')
    args = parser.parse_args()
    config = StorageConfig.from_env()
    repo = Repository(config.database_url, config.tenant, config.workspace)
    repo.migrate()
    try:
        if args.command == 'migrate':
            print('Database schema version 2 is ready.')
        elif args.command == 'init-demo':
            db.start(replace(config, seed_demo=True))
            print('Workspace is ready; existing inputs were preserved.')
        elif args.command == 'export':
            if not args.directory:
                parser.error('export requires a directory')
            db.start(config)
            folder = Path(args.directory)
            if (folder / 'snapshot.json').exists():
                parser.error('snapshot.json already exists; use an empty export directory')
            codec = Codec(Objects(str(folder), config.tenant, config.workspace))
            ref = codec.dump(store.snapshot())
            (folder / 'snapshot.json').write_text(json.dumps({
                'tenant': config.tenant, 'workspace': config.workspace, 'manifest': ref}, indent=2), encoding='utf-8')
            print('Input snapshot exported. This is not a full database/result backup.')
        else:
            if not args.directory:
                parser.error('import requires an export directory')
            if repo.head() is not None:
                parser.error('import requires a new workspace; existing state is never overwritten')
            folder = Path(args.directory)
            root = json.loads((folder / 'snapshot.json').read_text(encoding='utf-8'))
            source = Codec(Objects(str(folder), root['tenant'], root['workspace']))
            state = source.load(root['manifest'])
            required = {'books', 'market', 'scenarios', 'settings', 'dep_hist', 'mbs_hists',
                        'asof', 'equity', 'hedges', 'programs', 'assumptions', 'revision'}
            optional = {'tapes', 'cohort_publications'}
            if not required <= set(state) or set(state) - required - optional:
                raise ValueError('invalid input snapshot contract')
            state.setdefault('tapes', {})
            state.setdefault('cohort_publications', {})
            # Input-only exports retain rules/source identity but do not migrate
            # job histories into a new workspace. Rebuild to query its lineage.
            for receipt in state['cohort_publications'].values():
                receipt.pop('job_id', None)
                receipt['imported_input_snapshot'] = True
            state['revision'] = 0
            target = Codec(Objects(config.artifact_url, config.tenant, config.workspace))
            repo.initialize(target.dump(state))
            print('Snapshot imported into revision 0.')
    finally:
        repo.engine.dispose()
        db.stop()


if __name__ == '__main__':
    main()
