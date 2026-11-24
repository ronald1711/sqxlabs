"""Crash-recoverable append-only capture registry for local Linux/macOS recording.

A published record commits one whole window. Orphan blobs are harmless and do
not advance the cursor. This does not certify broker history completeness.
"""
import fcntl
import hashlib
import json
import os
from pathlib import Path
import tempfile


def encode(value):
    return json.dumps(value,sort_keys=True,separators=(',',':'),allow_nan=False).encode()


def publish(path, data):
    path.parent.mkdir(parents=True,exist_ok=True)
    temporary=None
    try:
        with tempfile.NamedTemporaryFile(dir=path.parent,prefix='.pending-',delete=False) as output:
            temporary=Path(output.name);output.write(data);output.flush();os.fsync(output.fileno())
        try: os.link(temporary,path)
        except FileExistsError:
            if path.read_bytes()!=data: raise ValueError('immutable journal file conflicts with existing content')
        descriptor=os.open(path.parent,os.O_RDONLY)
        try:os.fsync(descriptor)
        finally:os.close(descriptor)
    finally:
        if temporary is not None:temporary.unlink(missing_ok=True)


class Journal:
    def __init__(self,root,config):
        self.root=Path(root);self._config_bytes=encode(config);self.lock=None
        config=json.loads(self._config_bytes)
        if set(config)!={'schema','symbols','server','broker_start','window_seconds','warmup_seconds'} or type(config['schema']) is not int or config['schema']!=1:
            raise ValueError('unsupported recording configuration')
        if not isinstance(config['symbols'],list) or not 1<=len(config['symbols'])<=12 or any(not isinstance(s,str) or not s.strip() for s in config['symbols']) or len(set(config['symbols']))!=len(config['symbols']) or not isinstance(config['server'],str) or not config['server'].strip():
            raise ValueError('symbols and server required')
        for key in ('broker_start','window_seconds','warmup_seconds'):
            if type(config[key]) is not int:raise ValueError('integer recording times required')
        if config['broker_start']%60 or not 60<=config['window_seconds']<=3600 or config['window_seconds']%60 or not 0<=config['warmup_seconds']<=3600:
            raise ValueError('invalid recording window or warmup')

    @property
    def config(self):
        return json.loads(self._config_bytes)

    def __enter__(self):
        self.root.mkdir(parents=True,exist_ok=True)
        self.lock=(self.root/'writer.lock').open('a+b')
        try:
            fcntl.flock(self.lock,fcntl.LOCK_EX|fcntl.LOCK_NB)
            if not (self.root/'config.json').exists() and any(
                path.name!='writer.lock' and not path.name.startswith('.pending-')
                for path in self.root.iterdir()
            ):
                raise ValueError('existing recording is missing its configuration; restore the original file')
            publish(self.root/'config.json',encode(self.config))
            self.verify()
        except BaseException:
            self.lock.close();self.lock=None;raise
        return self

    def __exit__(self,*args):
        self.lock.close();self.lock=None

    def validate_capture(self,payload,start,account):
        if payload.get('schema')!=1 or payload.get('broker_window_seconds')!=[start,start+self.config['window_seconds']]:
            raise ValueError('capture window does not match recording cursor')
        if payload.get('tick_warmup_seconds')!=self.config['warmup_seconds'] or [m['symbol'] for m in payload['markets']]!=self.config['symbols']:
            raise ValueError('capture symbols or warmup differ from recording configuration')
        identity=payload['account']
        if identity.get('server')!=self.config['server'] or (account is not None and identity!=account):
            raise ValueError('recording account context changed')
        return identity

    def specifications(self,payload):
        specs=[market.get('spec') for market in payload['markets']]
        if any(not isinstance(spec,dict) or not spec for spec in specs):
            raise ValueError('capture instrument specifications missing')
        return specs

    def _scan(self,start=None,end=None):
        selected=[]
        if (self.root/'config.json').read_bytes()!=self._config_bytes:
            raise ValueError('recording configuration changed on disk')
        cursor=self.config['broker_start'];account=None;specs=None
        records=self.root/'records'
        files=sorted(records.glob('*.json'),key=lambda p:int(p.stem)) if records.exists() else []
        for path in files:
            record=json.loads(path.read_bytes())
            identifier=record['capture_sha256']
            if not isinstance(identifier,str) or len(identifier)!=64 or any(c not in '0123456789abcdef' for c in identifier):
                raise ValueError('invalid capture identifier')
            if record!={'schema':1,'start':cursor,'capture_sha256':identifier} or path.name!=f'{cursor}.json':
                raise ValueError('recording has a missing or conflicting window')
            data=(self.root/'captures'/f'{identifier}.json').read_bytes()
            if hashlib.sha256(data).hexdigest()!=identifier:raise ValueError('capture integrity failure')
            payload=json.loads(data)
            account=self.validate_capture(payload,cursor,account)
            observed=self.specifications(payload)
            if specs is not None and observed!=specs:raise ValueError('recording instrument specifications changed')
            specs=observed
            if start is not None and start<=cursor<end:selected.append({'record':record,'capture':payload})
            cursor+=self.config['window_seconds']
        if (self.root/'config.json').read_bytes()!=self._config_bytes:
            raise ValueError('recording configuration changed during snapshot')
        return cursor,account,specs,selected

    def verify(self):
        if self.lock is None:raise RuntimeError('journal writer lock required')
        cursor,account,specs,_=self._scan()
        self._verified_specs=specs
        return cursor,account

    def snapshot(self,start,end):
        config=self.config;width=config['window_seconds']
        if type(start) is not int or type(end) is not int or not 0<end-start<=3600:
            raise ValueError('export requires an explicit range of at most one hour')
        if start<config['broker_start'] or (start-config['broker_start'])%width or (end-start)%width:
            raise ValueError('export boundaries must match whole recording windows')
        # Snapshot a finite list of atomic commit records. Never read orphan blobs.
        cursor,_,_,selected=self._scan(start,end)
        if end>cursor:raise ValueError('export includes an uncommitted window')
        return selected

    def status(self):
        cursor,account,specs,_=self._scan()
        config=self.config
        return {'schema':1,'integrity':'verified','configuration':config,
            'committed_windows':(cursor-config['broker_start'])//config['window_seconds'],
            'broker_window_seconds':[config['broker_start'],cursor],
            'next_broker_start':cursor,'account_context':account,
            'instrument_specifications':specs,'writer_status':'not_observed',
            'scope':'Verified committed snapshot only. Broker-clock timestamps are not UTC-normalized. Tick completeness, recorder liveness and historical specification validity are not certified.'}

    def append(self,payload):
        cursor,account=self.verify()
        self.validate_capture(payload,cursor,account)
        observed=self.specifications(payload)
        if self._verified_specs is not None and observed!=self._verified_specs:
            raise ValueError('recording instrument specifications changed')
        data=encode(payload);identifier=hashlib.sha256(data).hexdigest()
        publish(self.root/'captures'/f'{identifier}.json',data)
        # Publish the commit record last. No separate mutable cursor can race ahead.
        publish(self.root/'records'/f'{cursor}.json',encode({'schema':1,'start':cursor,'capture_sha256':identifier}))
        return identifier
