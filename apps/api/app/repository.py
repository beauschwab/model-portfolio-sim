"""Portable transactional metadata repository (SQLite/PostgreSQL).

All operations are scoped to one tenant/workspace. Objects are uploaded before
their references commit; orphan objects are safe and can be collected later.
"""
from __future__ import annotations

import time
import uuid

import sqlalchemy as sa
from sqlalchemy.exc import IntegrityError
from sqlalchemy.engine import make_url
from pathlib import Path

metadata = sa.MetaData()
schema = sa.Table("schema_version", metadata, sa.Column("version", sa.Integer, primary_key=True))
workspaces = sa.Table("workspaces", metadata,
    sa.Column("tenant", sa.String(128), primary_key=True),
    sa.Column("workspace", sa.String(128), primary_key=True),
    sa.Column("revision", sa.Integer, nullable=False),
    sa.Column("owner", sa.String(64)), sa.Column("lease_until", sa.Float, nullable=False, default=0.))


def scope_columns():
    return [sa.Column("tenant", sa.String(128), primary_key=True),
            sa.Column("workspace", sa.String(128), primary_key=True)]


revisions = sa.Table("revisions", metadata, *scope_columns(),
    sa.Column("revision", sa.Integer, primary_key=True), sa.Column("manifest", sa.JSON, nullable=False),
    sa.Column("created_at", sa.Float, nullable=False), sa.Column("actor", sa.String(200), nullable=False),
    sa.Column("reason", sa.String(200), nullable=False))
jobs = sa.Table("jobs", metadata, *scope_columns(), sa.Column("id", sa.String(64), primary_key=True),
    sa.Column("kind", sa.String(64), nullable=False), sa.Column("revision", sa.Integer, nullable=False),
    sa.Column("status", sa.String(20), nullable=False), sa.Column("request", sa.JSON, nullable=False),
    sa.Column("result", sa.JSON), sa.Column("progress", sa.JSON), sa.Column("detail", sa.Text),
    sa.Column("created_at", sa.Float, nullable=False), sa.Column("finished_at", sa.Float),
    sa.Column("owner", sa.String(64)), sa.Column("attempt", sa.Integer, nullable=False, default=0),
    sa.Column("token", sa.String(64)), sa.Column("idempotency_key", sa.String(128)),
    sa.UniqueConstraint("tenant", "workspace", "idempotency_key"))
sa.Index("jobs_queue", jobs.c.tenant, jobs.c.workspace, jobs.c.status, jobs.c.created_at)
sessions = sa.Table("sessions", metadata, *scope_columns(), sa.Column("id", sa.String(64), primary_key=True),
    sa.Column("revision", sa.Integer, nullable=False), sa.Column("manifest", sa.JSON, nullable=False),
    sa.Column("version", sa.Integer, nullable=False), sa.Column("closed", sa.Boolean, nullable=False, default=False))
libraries = sa.Table("libraries", metadata, *scope_columns(),
    sa.Column("revision", sa.Integer, primary_key=True), sa.Column("manifest", sa.JSON, nullable=False))
research = sa.Table("research_snapshots", metadata, *scope_columns(),
    sa.Column("id", sa.String(64), primary_key=True), sa.Column("manifest", sa.JSON, nullable=False),
    sa.Column("created_at", sa.Float, nullable=False))
publications = sa.Table('catalog_publications', metadata, *scope_columns(),
    sa.Column('job_id', sa.String(64), primary_key=True),
    sa.Column('destination', sa.String(64), primary_key=True),
    sa.Column('manifest', sa.JSON, nullable=False), sa.Column('created_at', sa.Float, nullable=False))
for table in (revisions, jobs, sessions, libraries, research, publications):
    table.append_constraint(sa.ForeignKeyConstraint(["tenant", "workspace"], ["workspaces.tenant", "workspaces.workspace"]))


class Conflict(RuntimeError):
    pass


class Repository:
    def __init__(self, url, tenant, workspace):
        if not isinstance(url, str):
            url = url.render_as_string(hide_password=False)
        if url.startswith("postgres://"):
            url = "postgresql+psycopg://" + url[len("postgres://"):]
        elif url.startswith("postgresql://"):
            url = "postgresql+psycopg://" + url[len("postgresql://"):]
        parsed = make_url(url)
        if parsed.get_backend_name() not in {"sqlite", "postgresql"}:
            raise ValueError("DATABASE_URL must select SQLite or PostgreSQL")
        if parsed.get_backend_name() == "sqlite" and parsed.database not in {None, "", ":memory:"}:
            Path(parsed.database).resolve().parent.mkdir(parents=True, exist_ok=True)
        self.engine = sa.create_engine(url, pool_pre_ping=True,
            connect_args={"check_same_thread": False, "timeout": 30} if parsed.get_backend_name() == "sqlite" else {})
        if parsed.get_backend_name() == "sqlite":
            @sa.event.listens_for(self.engine, "connect")
            def sqlite_config(connection, _record):
                connection.isolation_level = None
                cursor = connection.cursor()
                cursor.execute("PRAGMA journal_mode=WAL")
                cursor.execute("PRAGMA foreign_keys=ON")
                cursor.execute("PRAGMA busy_timeout=30000")
                cursor.close()
            @sa.event.listens_for(self.engine, "begin")
            def sqlite_begin(connection):
                connection.exec_driver_sql("BEGIN")
        self.identity = {"tenant": tenant, "workspace": workspace}

    def migrate(self):
        # Version 2 adds only catalog_publications; existing rows are unchanged.
        metadata.create_all(self.engine)
        with self.engine.begin() as conn:
            versions = conn.execute(sa.select(schema.c.version)).scalars().all()
            if not versions:
                conn.execute(schema.insert().values(version=2))
            elif versions == [1]:
                conn.execute(schema.update().where(schema.c.version == 1).values(version=2))
            elif versions != [2]:
                raise RuntimeError("unsupported database schema version")

    def publication_receipts(self, jid):
        with self.engine.connect() as conn:
            return list(conn.execute(sa.select(publications).where(self.scoped(publications),
                publications.c.job_id == jid)).mappings())

    def record_publication(self, jid, destination, result, manifest):
        with self.engine.begin() as conn:
            self._lock(conn)
            job = conn.execute(sa.select(jobs).where(self.scoped(jobs), jobs.c.id == jid)).mappings().one()
            if job['status'] != 'done' or job['result'] != result:
                raise Conflict('result changed during catalog publication')
            existing = conn.execute(sa.select(publications.c.manifest).where(self.scoped(publications),
                publications.c.job_id == jid, publications.c.destination == destination)).scalar_one_or_none()
            if existing is not None:
                if existing != manifest:
                    raise Conflict('catalog publication receipt differs')
                return
            conn.execute(publications.insert().values(**self.identity, job_id=jid, destination=destination,
                manifest=manifest, created_at=time.time()))

    def scoped(self, table):
        return sa.and_(*(table.c[k] == v for k, v in self.identity.items()))

    def _lock(self, conn):
        result = conn.execute(workspaces.update().where(self.scoped(workspaces)).values(revision=workspaces.c.revision))
        if not result.rowcount:
            raise KeyError("workspace has not been initialized")

    def head(self):
        with self.engine.connect() as conn:
            return conn.execute(sa.select(workspaces.c.revision).where(self.scoped(workspaces))).scalar_one_or_none()

    def initialize(self, manifest):
        try:
            with self.engine.begin() as conn:
                conn.execute(workspaces.insert().values(**self.identity, revision=0, lease_until=0.))
                conn.execute(revisions.insert().values(**self.identity, revision=0, manifest=manifest,
                    created_at=time.time(), actor="bootstrap", reason="initial snapshot"))
        except IntegrityError:
            if self.head() is None:
                raise

    def revision(self, number=None):
        number = self.head() if number is None else number
        with self.engine.connect() as conn:
            row = conn.execute(sa.select(revisions).where(self.scoped(revisions), revisions.c.revision == number)).mappings().first()
            if row is None:
                raise KeyError("unknown revision")
            return dict(row)

    def save_revision(self, expected, manifest, *, actor="api", reason="input edit"):
        with self.engine.begin() as conn:
            result = conn.execute(workspaces.update().where(self.scoped(workspaces), workspaces.c.revision == expected)
                                  .values(revision=expected + 1))
            if result.rowcount != 1:
                raise Conflict("saved inputs changed; reload before editing")
            conn.execute(revisions.insert().values(**self.identity, revision=expected + 1, manifest=manifest,
                created_at=time.time(), actor=actor, reason=reason))
        return expected + 1

    def history(self, limit=100):
        with self.engine.connect() as conn:
            return [dict(r) for r in conn.execute(sa.select(revisions).where(self.scoped(revisions))
                    .order_by(revisions.c.revision.desc()).limit(limit)).mappings()]

    def enqueue(self, kind, revision, request, progress, *, key=None, max_queue=32):
        with self.engine.begin() as conn:
            self._lock(conn)
            if key:
                old = conn.execute(sa.select(jobs).where(self.scoped(jobs), jobs.c.idempotency_key == key)).mappings().first()
                if old:
                    if old["request"] != request:
                        raise Conflict("idempotency key already used for a different request")
                    return old["id"]
            count = conn.execute(sa.select(sa.func.count()).select_from(jobs).where(self.scoped(jobs),
                                     jobs.c.status.in_(["queued", "running"]))).scalar_one()
            if count >= max_queue:
                from .store import QueueFull
                raise QueueFull("compute queue is full; wait for a job to finish")
            jid = uuid.uuid4().hex
            conn.execute(jobs.insert().values(**self.identity, id=jid, kind=kind, revision=revision,
                status="queued", request=request, progress=progress, created_at=time.time(), attempt=0, idempotency_key=key))
            return jid

    def job(self, jid):
        with self.engine.connect() as conn:
            row = conn.execute(sa.select(jobs).where(self.scoped(jobs), jobs.c.id == jid)).mappings().first()
            if row is None:
                raise KeyError("unknown job")
            return dict(row)

    def acquire(self, owner, seconds=60):
        now = time.time()
        with self.engine.begin() as conn:
            result = conn.execute(workspaces.update().where(self.scoped(workspaces),
                sa.or_(workspaces.c.lease_until < now, workspaces.c.owner == owner))
                .values(owner=owner, lease_until=now + seconds))
            return result.rowcount == 1

    def owns(self, owner):
        with self.engine.connect() as conn:
            return conn.execute(sa.select(workspaces.c.owner).where(self.scoped(workspaces),
                workspaces.c.owner == owner, workspaces.c.lease_until > time.time())).scalar_one_or_none() is not None

    def renew(self, owner, seconds=60):
        now = time.time()
        with self.engine.begin() as conn:
            return conn.execute(workspaces.update().where(self.scoped(workspaces),
                workspaces.c.owner == owner, workspaces.c.lease_until > now)
                .values(lease_until=now + seconds)).rowcount == 1

    def release(self, owner):
        with self.engine.begin() as conn:
            conn.execute(workspaces.update().where(self.scoped(workspaces), workspaces.c.owner == owner).values(lease_until=0.))

    def claim(self, owner):
        with self.engine.begin() as conn:
            self._lock(conn)
            valid = conn.execute(sa.select(workspaces.c.owner).where(self.scoped(workspaces),
                workspaces.c.owner == owner, workspaces.c.lease_until > time.time())).scalar_one_or_none()
            if valid is None:
                return None
            # A new lease holder retries interrupted work. The old attempt token
            # cannot publish even if its process eventually resumes.
            conn.execute(jobs.update().where(self.scoped(jobs), jobs.c.status == "running", jobs.c.owner != owner)
                         .values(status="queued", owner=None, token=None))
            # Oldest interactive job first; background refreshes wait behind all of
            # them. The queue is bounded (MAX_QUEUE), so ranking in Python is cheap.
            queued = conn.execute(sa.select(jobs.c.id, jobs.c.progress).where(self.scoped(jobs), jobs.c.status == "queued")
                                  .order_by(jobs.c.created_at, jobs.c.id)).mappings().all()
            if not queued:
                return None
            pick = min(queued, key=lambda r: (r["progress"] or {}).get("priority") == "background")
            row = dict(conn.execute(sa.select(jobs).where(self.scoped(jobs), jobs.c.id == pick["id"])).mappings().one())
            if row["attempt"] >= 3:
                conn.execute(jobs.update().where(self.scoped(jobs), jobs.c.id == row["id"])
                    .values(status="error", detail="worker interrupted three attempts", finished_at=time.time()))
                return None
            changes = dict(owner=owner, token=uuid.uuid4().hex, attempt=row["attempt"] + 1, status="running")
            conn.execute(jobs.update().where(self.scoped(jobs), jobs.c.id == row["id"]).values(**changes))
            return row | changes

    def progress(self, job, progress):
        with self.engine.begin() as conn:
            conn.execute(jobs.update().where(self.scoped(jobs), jobs.c.id == job["id"],
                jobs.c.token == job["token"], jobs.c.status == "running").values(progress=progress))

    def complete(self, job, *, result=None, error=None, session=None, library=None):
        with self.engine.begin() as conn:
            self._lock(conn)
            owner = conn.execute(sa.select(workspaces.c.owner).where(self.scoped(workspaces),
                workspaces.c.owner == job["owner"], workspaces.c.lease_until > time.time())).scalar_one_or_none()
            if owner is None:
                raise Conflict("worker lease lost; result discarded")
            current = conn.execute(sa.select(workspaces.c.revision).where(self.scoped(workspaces))).scalar_one()
            if (session or library) and current != job["revision"]:
                raise Conflict("inputs changed; interactive publication discarded")
            updated = conn.execute(jobs.update().where(self.scoped(jobs), jobs.c.id == job["id"],
                jobs.c.token == job["token"], jobs.c.status == "running")
                .values(status="error" if error else "done", result=result, detail=error, finished_at=time.time()))
            if updated.rowcount != 1:
                raise Conflict("job cancelled or superseded; result discarded")
            if session:
                sid, version, manifest = session
                existing = conn.execute(sa.select(sessions.c.version, sessions.c.closed).where(
                    self.scoped(sessions), sessions.c.id == sid)).first()
                if existing:
                    if existing.closed or existing.version != version - 1:
                        raise Conflict("session closed or version superseded")
                    conn.execute(sessions.update().where(self.scoped(sessions), sessions.c.id == sid)
                                 .values(manifest=manifest, version=version))
                else:
                    conn.execute(sessions.insert().values(**self.identity, id=sid, revision=current,
                                                         version=version, manifest=manifest, closed=False))
            if library:
                conn.execute(libraries.delete().where(self.scoped(libraries), libraries.c.revision == current))
                conn.execute(libraries.insert().values(**self.identity, revision=current, manifest=library))

    def cancel(self, jid):
        with self.engine.begin() as conn:
            self._lock(conn)
            row = conn.execute(sa.select(jobs.c.status).where(self.scoped(jobs), jobs.c.id == jid)).scalar_one_or_none()
            if row is None:
                raise KeyError("unknown job")
            if row in {"queued", "running"}:
                conn.execute(jobs.update().where(self.scoped(jobs), jobs.c.id == jid)
                    .values(status="error", detail="cancelled by user; in-flight kernel output will be discarded",
                            token=None, finished_at=time.time()))
            return row in {"queued", "running"}

    def saved_sessions(self):
        with self.engine.connect() as conn:
            return [dict(r) for r in conn.execute(sa.select(sessions).where(self.scoped(sessions),
                sessions.c.closed == False, sessions.c.revision == self.head())).mappings()]

    def session(self, sid):
        with self.engine.connect() as conn:
            row = conn.execute(sa.select(sessions).where(self.scoped(sessions), sessions.c.id == sid)).mappings().first()
            return dict(row) if row else None

    def close_session(self, sid):
        with self.engine.begin() as conn:
            self._lock(conn)
            return conn.execute(sessions.update().where(self.scoped(sessions), sessions.c.id == sid,
                sessions.c.closed == False).values(closed=True)).rowcount > 0

    def library(self):
        with self.engine.connect() as conn:
            return conn.execute(sa.select(libraries.c.manifest).where(self.scoped(libraries),
                libraries.c.revision == self.head())).scalar_one_or_none()

    def save_research(self, sid, manifest):
        with self.engine.begin() as conn:
            self._lock(conn)
            exists = conn.execute(sa.select(research.c.id).where(self.scoped(research), research.c.id == sid)).first()
            if not exists:
                conn.execute(research.insert().values(**self.identity, id=sid, manifest=manifest, created_at=time.time()))

    def research(self, sid=None):
        with self.engine.connect() as conn:
            query = sa.select(research).where(self.scoped(research))
            if sid:
                row = conn.execute(query.where(research.c.id == sid)).mappings().first()
                if row is None:
                    raise KeyError(sid)
                return dict(row)
            return [dict(r) for r in conn.execute(query.order_by(research.c.created_at.desc()).limit(100)).mappings()]
