"""Application-owned conversations and evidence, in the existing scoped SQL database.

These additive application tables do not change the shared financial schema or its version.
Goose histories are disposable transport state, never another financial source of truth.
"""
import hashlib
import json
import re
import time
import uuid

import sqlalchemy as sa

from .repository import Conflict

metadata = sa.MetaData()


def columns():
    return [sa.Column('tenant', sa.String(128), primary_key=True),
            sa.Column('workspace', sa.String(128), primary_key=True)]


conversations = sa.Table('analyst_conversations', metadata, *columns(),
    sa.Column('id', sa.String(64), primary_key=True), sa.Column('principal', sa.String(200), nullable=False),
    sa.Column('role', sa.String(20), nullable=False), sa.Column('title', sa.String(120), nullable=False),
    sa.Column('created_at', sa.Float, nullable=False), sa.Column('active_turn', sa.String(64)))
turns = sa.Table('analyst_turns', metadata, *columns(),
    sa.Column('id', sa.String(64), primary_key=True), sa.Column('conversation_id', sa.String(64), nullable=False),
    sa.Column('request_id', sa.String(64), nullable=False), sa.Column('request_sha256', sa.String(64), nullable=False),
    sa.Column('revision', sa.Integer, nullable=False), sa.Column('workflow', sa.String(40), nullable=False),
    sa.Column('prompt', sa.Text, nullable=False), sa.Column('answer', sa.Text, nullable=False),
    sa.Column('status', sa.String(24), nullable=False), sa.Column('detail', sa.Text),
    sa.Column('created_at', sa.Float, nullable=False), sa.Column('deadline', sa.Float, nullable=False),
    sa.Column('tool_count', sa.Integer, nullable=False), sa.Column('receipt', sa.JSON, nullable=False),
    sa.UniqueConstraint('tenant', 'workspace', 'conversation_id', 'request_id'))
evidence = sa.Table('analyst_evidence', metadata, *columns(),
    sa.Column('id', sa.String(64), primary_key=True), sa.Column('turn_id', sa.String(64), nullable=False),
    sa.Column('tool', sa.String(120), nullable=False), sa.Column('arguments', sa.JSON, nullable=False),
    sa.Column('result', sa.JSON), sa.Column('sha256', sa.String(64)),
    sa.Column('created_at', sa.Float, nullable=False))


def digest(value):
    return hashlib.sha256(json.dumps(value, sort_keys=True, separators=(',', ':'), allow_nan=False).encode()).hexdigest()


class AnalystRepository:
    def __init__(self, repo):
        self.repo = repo

    def migrate(self):
        metadata.create_all(self.repo.engine)

    def scope(self, table):
        return self.repo.scoped(table)

    def _conversation(self, conn, cid, principal):
        row = conn.execute(sa.select(conversations).where(self.scope(conversations),
            conversations.c.id == cid, conversations.c.principal == principal)).mappings().first()
        if row is None:
            raise KeyError('Conversation not found')
        return dict(row)

    def create(self, principal, role, title):
        item = dict(id=uuid.uuid4().hex, principal=principal, role=role, title=title,
                    created_at=time.time(), active_turn=None)
        with self.repo.engine.begin() as conn:
            conn.execute(conversations.insert().values(**self.repo.identity, **item))
        return item

    def list(self, principal):
        with self.repo.engine.connect() as conn:
            return [dict(x) for x in conn.execute(sa.select(conversations).where(self.scope(conversations),
                conversations.c.principal == principal).order_by(conversations.c.created_at.desc()).limit(50)).mappings()]

    def get(self, cid, principal):
        with self.repo.engine.begin() as conn:
            item = self._conversation(conn, cid, principal)
            rows = [dict(x) for x in conn.execute(sa.select(turns).where(self.scope(turns),
                turns.c.conversation_id == cid).order_by(turns.c.created_at).limit(50)).mappings()]
            for row in rows:
                if row['status'] == 'running' and row['deadline'] <= time.time():
                    self._finish(conn, row['id'], 'interrupted', 'Turn expired; financial jobs were not changed.')
                    row.update(status='interrupted', detail='Turn expired; financial jobs were not changed.')
                row['evidence'] = [dict(x) for x in conn.execute(sa.select(
                    evidence.c.id, evidence.c.tool, evidence.c.sha256, evidence.c.created_at).where(
                    self.scope(evidence), evidence.c.turn_id == row['id']).order_by(evidence.c.created_at)).mappings()]
            item['turns'] = rows
            return item

    def start(self, cid, principal, request, revision, receipt, seconds=180):
        with self.repo.engine.begin() as conn:
            self.repo._lock(conn)  # Portable workspace admission lock; no revision change.
            self._conversation(conn, cid, principal)
            old = conn.execute(sa.select(turns).where(self.scope(turns), turns.c.conversation_id == cid,
                turns.c.request_id == request['request_id'])).mappings().first()
            fingerprint = digest(request)
            if old:
                if old['request_sha256'] != fingerprint:
                    raise Conflict('Request ID already belongs to different analyst input')
                return dict(old), False
            from .repository import workspaces
            current = conn.execute(sa.select(workspaces.c.revision).where(self.repo.scoped(workspaces))).scalar_one()
            if current != revision:
                raise Conflict('Saved inputs changed during analyst admission')
            active = conn.execute(sa.select(turns).where(self.scope(turns), turns.c.conversation_id == cid,
                turns.c.status == 'running')).mappings().first()
            if active and active['deadline'] > time.time():
                raise Conflict('This conversation already has an active turn')
            if active:
                self._finish(conn, active['id'], 'interrupted', 'Previous host did not finish before its deadline.')
            count = conn.execute(sa.select(sa.func.count()).select_from(turns).where(self.scope(turns),
                turns.c.conversation_id == cid)).scalar_one()
            if count >= 50:
                raise Conflict('Conversation limit reached; start a new conversation')
            # Bound model/process admission across API replicas, independently of financial jobs.
            running = conn.execute(sa.select(sa.func.count()).select_from(turns).where(self.scope(turns),
                turns.c.status == 'running', turns.c.deadline > time.time())).scalar_one()
            if running >= 2:
                raise Conflict('Two analyst turns are already active in this workspace')
            item = dict(id=uuid.uuid4().hex, conversation_id=cid, request_id=request['request_id'],
                request_sha256=fingerprint, revision=revision, workflow=request['workflow'], prompt=request['prompt'],
                answer='', status='running', detail=None, created_at=time.time(), deadline=time.time()+seconds,
                tool_count=0, receipt=receipt)
            conn.execute(turns.insert().values(**self.repo.identity, **item))
            conn.execute(conversations.update().where(self.scope(conversations), conversations.c.id == cid)
                         .values(active_turn=item['id']))
            return item, True

    def active(self, tid):
        with self.repo.engine.connect() as conn:
            row = conn.execute(sa.select(turns).where(self.scope(turns), turns.c.id == tid)).mappings().first()
            return bool(row and row['status'] == 'running' and row['deadline'] > time.time())

    def _finish(self, conn, tid, status, detail=None, answer=None):
        values = dict(status=status, detail=detail)
        if answer is not None:
            values['answer'] = answer
        conn.execute(turns.update().where(self.scope(turns), turns.c.id == tid,
            turns.c.status == 'running').values(**values))
        conn.execute(conversations.update().where(self.scope(conversations),
            conversations.c.active_turn == tid).values(active_turn=None))

    def finish(self, tid, status, detail=None, answer=None):
        with self.repo.engine.begin() as conn:
            self.repo._lock(conn)
            if status == 'done':
                deadline = conn.execute(sa.select(turns.c.deadline).where(self.scope(turns), turns.c.id == tid)).scalar_one()
                if deadline <= time.time():
                    status, detail = 'interrupted', 'Turn expired before completion; draft output is retained.'
                else:
                    rows = conn.execute(sa.select(evidence.c.id, evidence.c.result).where(
                        self.scope(evidence), evidence.c.turn_id == tid, evidence.c.sha256.is_not(None))).all()
                    known = {row.id for row in rows if row.result and row.result.get('ok')}
                    cited = set(re.findall(r'\[(E[A-Za-z0-9]+)\]', answer or ''))
                    if not cited or not cited <= known:
                        status, detail = 'error', 'Draft lacks valid captured evidence citations. Inspect the evidence before relying on it.'
                    from .repository import workspaces
                    admitted = conn.execute(sa.select(turns.c.revision).where(self.scope(turns), turns.c.id == tid)).scalar_one()
                    current = conn.execute(sa.select(workspaces.c.revision).where(self.repo.scoped(workspaces))).scalar_one()
                    if admitted != current:
                        status, detail = 'error', 'Saved inputs changed during this answer; start a new turn at the current revision.'
            self._finish(conn, tid, status, detail, answer)

    def read_evidence(self, cid, principal, eid):
        with self.repo.engine.connect() as conn:
            self._conversation(conn, cid, principal)
            row = conn.execute(sa.select(evidence).join(turns, sa.and_(
                evidence.c.tenant == turns.c.tenant, evidence.c.workspace == turns.c.workspace,
                evidence.c.turn_id == turns.c.id)).where(self.scope(evidence), evidence.c.id == eid,
                    turns.c.conversation_id == cid)).mappings().first()
            if row is None:
                raise KeyError('Evidence not found')
            if row['sha256'] and digest(row['result']) != row['sha256']:
                raise Conflict('Stored analyst evidence hash does not match')
            return dict(row)

    def append(self, tid, answer):
        if len(answer) > 64000:
            raise ValueError('Analyst output limit reached')
        with self.repo.engine.begin() as conn:
            conn.execute(turns.update().where(self.scope(turns), turns.c.id == tid,
                turns.c.status == 'running', turns.c.deadline > time.time()).values(answer=answer))

    def reserve_tool(self, tid, tool, arguments):
        eid = 'E' + uuid.uuid4().hex[:16]
        with self.repo.engine.begin() as conn:
            self.repo._lock(conn)
            from .repository import workspaces
            admitted = conn.execute(sa.select(turns.c.revision).where(self.scope(turns), turns.c.id == tid)).scalar_one_or_none()
            current = conn.execute(sa.select(workspaces.c.revision).where(self.repo.scoped(workspaces))).scalar_one()
            if admitted != current:
                raise Conflict('Saved inputs changed during this answer; start a new turn at the current revision')
            result = conn.execute(turns.update().where(self.scope(turns), turns.c.id == tid,
                turns.c.status == 'running', turns.c.deadline > time.time(), turns.c.tool_count < 24)
                .values(tool_count=turns.c.tool_count+1))
            if not result.rowcount:
                raise Conflict('Turn inactive, expired or tool budget exhausted')
            conn.execute(evidence.insert().values(**self.repo.identity, id=eid, turn_id=tid,
                tool=tool, arguments=arguments, result=None, sha256=None, created_at=time.time()))
        return eid

    def record_tool(self, eid, result):
        sha = digest(result)
        with self.repo.engine.begin() as conn:
            updated = conn.execute(evidence.update().where(self.scope(evidence), evidence.c.id == eid,
                evidence.c.sha256.is_(None)).values(result=result, sha256=sha))
            if updated.rowcount != 1:
                raise Conflict('Analyst evidence is immutable once recorded')
        return sha
