"""One cumulative storage/decoded/prefix budget for a verified cut chain."""
from .completed_source_reader import encoded

MAX_LINKS = 8
MAX_OBJECTS = 1024
MAX_BYTES = 64 << 20
MAX_PREFIX_BYTES = 32 << 20


class ChainBudget:
    def __init__(self):
        self.jobs = set()
        self.objects = set()
        self.read_bytes = self.decoded_bytes = self.logical_bytes = self.prefix_bytes = 0

    def enter(self, job_id):
        if job_id in self.jobs:
            raise ValueError('cyclic cut predecessor chain')
        if len(self.jobs) >= MAX_LINKS:
            raise ValueError('cut predecessor chain exceeds eight publications')
        self.jobs.add(job_id)

    def read(self, ref):
        identity = encoded(ref)
        if identity not in self.objects and len(self.objects) >= MAX_OBJECTS:
            raise ValueError('cut chain aggregate object admission')
        if self.read_bytes + ref['bytes'] > MAX_BYTES:
            raise ValueError('cut chain aggregate read admission')
        self.objects.add(identity)
        self.read_bytes += ref['bytes']

    def expand(self, count):
        if self.decoded_bytes + count > MAX_BYTES:
            raise ValueError('cut chain aggregate decoded admission')
        self.decoded_bytes += count

    def materialize(self, count):
        if self.logical_bytes + count > MAX_BYTES:
            raise ValueError('cut chain aggregate logical admission')
        self.logical_bytes += count

    def retain(self, count):
        if self.prefix_bytes + count > MAX_PREFIX_BYTES:
            raise ValueError('cut chain aggregate prefix admission')
        self.prefix_bytes += count
