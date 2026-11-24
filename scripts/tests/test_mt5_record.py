import importlib.util
from pathlib import Path
import sys
import tempfile
import types
import unittest
from unittest.mock import patch

ROOT=Path(__file__).resolve().parents[1]
with patch.object(sys,'path',[str(ROOT),*sys.path]):
    spec=importlib.util.spec_from_file_location('recorder_under_test',ROOT/'mt5_record.py')
    recorder=importlib.util.module_from_spec(spec);spec.loader.exec_module(recorder)
CONFIG={'schema':1,'symbols':['EURUSD'],'server':'fixture','broker_start':120,'window_seconds':60,'warmup_seconds':60}
def capture(client,symbols,start,end,warmup):
    return {'schema':1,'broker_window_seconds':[start,end],'tick_warmup_seconds':warmup,'account':{'server':'fixture','currency':'EUR','leverage':30,'trade_mode':0},'markets':[{'symbol':s,'spec':{'trade_contract_size':100000}} for s in symbols]}
class Client:
    def __init__(self):self.closed=False
    def initialize(self):return True
    def account_info(self):return types.SimpleNamespace(server="fixture",currency="EUR",leverage=30,trade_mode=0)
    def symbol_info_tick(self,symbol):return types.SimpleNamespace(time=600)
    def close(self):self.closed=True

class ReconnectTests(unittest.TestCase):
    def test_partial_read_retries_same_window_then_commits_once(self):
        clients=[];starts=[];events=[];delays=[]
        def factory():
            client=Client();clients.append(client);return client
        def read(*args):
            starts.append(args[2])
            if len(starts)==1:raise EOFError('connection lost during read')
            return capture(*args)
        with tempfile.TemporaryDirectory() as root,recorder.Journal(root,CONFIG) as journal:
            self.assertEqual(recorder.record_windows(journal,factory,read,2,sleep=delays.append,emit=events.append),2)
            self.assertEqual(journal.verify()[0],240)
            self.assertEqual(starts,[120,120,180]);self.assertEqual(delays,[1])
            self.assertEqual([e['status'] for e in events],['reconnecting','window_committed','window_committed'])
        self.assertTrue(all(c.closed for c in clients));self.assertEqual(len(clients),2)

    def test_exhausted_retries_and_validation_or_disk_errors_preserve_cursor(self):
        with tempfile.TemporaryDirectory() as root,recorder.Journal(root,CONFIG) as journal:
            clients=[]
            def factory():
                c=Client();clients.append(c);return c
            with self.assertRaises(TimeoutError):
                recorder.record_windows(journal,factory,lambda *a:(_ for _ in ()).throw(TimeoutError()),1,reconnect_attempts=2,sleep=lambda _:None,emit=lambda _:None)
            self.assertEqual(len(clients),3);self.assertEqual(journal.verify()[0],120)
            for error in [ValueError('invalid candle'),OSError('disk full')]:
                count=len(clients)
                with self.assertRaises(type(error)):
                    recorder.record_windows(journal,factory,lambda *a:(_ for _ in ()).throw(error),1,sleep=lambda _:self.fail('must not retry'),emit=lambda _:None)
                self.assertEqual(len(clients),count+1);self.assertEqual(journal.verify()[0],120)
            with patch.object(journal,'append',side_effect=ConnectionError('storage adapter failure')):
                with self.assertRaises(ConnectionError):
                    recorder.record_windows(journal,factory,capture,1,sleep=lambda _:self.fail('must not retry storage'),emit=lambda _:None)
            self.assertTrue(all(c.closed for c in clients))

    def test_follow_waits_without_advancing_until_closed(self):
        client=Client();client.now=120
        client.symbol_info_tick=lambda _:types.SimpleNamespace(time=client.now)
        events=[]
        def advance(_):client.now=180
        with tempfile.TemporaryDirectory() as root,recorder.Journal(root,CONFIG) as journal:
            recorder.record_windows(journal,lambda:client,capture,1,follow=True,sleep=advance,emit=events.append)
            self.assertEqual(journal.verify()[0],180)
        self.assertEqual([e['status'] for e in events],['waiting_for_closed_window','window_committed'])

    def test_changed_account_after_reconnect_stops_before_reading_history(self):
        clients=[];reads=[]
        def factory():
            c=Client()
            if clients:c.account_info=lambda:types.SimpleNamespace(server='fixture',currency='USD',leverage=30,trade_mode=0)
            clients.append(c);return c
        def read(*args):
            reads.append(args[2]);raise EOFError('transport failed')
        with tempfile.TemporaryDirectory() as root,recorder.Journal(root,CONFIG) as journal:
            with self.assertRaisesRegex(ValueError,'before reading window'):
                recorder.record_windows(journal,factory,read,1,sleep=lambda _:None,emit=lambda _:None)
            self.assertEqual(reads,[120]);self.assertEqual(journal.verify()[0],120)
        self.assertEqual(len(clients),2);self.assertTrue(all(c.closed for c in clients))

    def test_history_availability_retries_same_window_and_resets_only_after_commit(self):
        client=Client();starts=[];events=[];counts={};delays=[]
        def read(*args):
            start=args[2];starts.append(start);counts[start]=counts.get(start,0)+1
            if counts[start]<=2:raise recorder.HistoryUnavailable('fixture history pending')
            return capture(*args)
        with tempfile.TemporaryDirectory() as root,recorder.Journal(root,CONFIG) as journal:
            def wait(delay):
                delays.append(delay)
                self.assertEqual(journal.verify()[0],starts[-1], 'unavailable history cannot advance progress')
            result=recorder.record_windows(journal,lambda:client,read,2,history_attempts=2,sleep=wait,emit=events.append)
            self.assertEqual(result,2);self.assertEqual(journal.verify()[0],240)
        self.assertEqual(starts,[120,120,120,180,180,180]);self.assertEqual(delays,[5]*4)
        self.assertEqual([e['status'] for e in events],['waiting_for_history']*2+['window_committed']+['waiting_for_history']*2+['window_committed'])
        self.assertTrue(client.closed)

    def test_exhausted_history_wait_keeps_cursor_and_closes_client(self):
        client=Client();attempts=[]
        def read(*args):attempts.append(args[2]);raise recorder.HistoryUnavailable('empty')
        with tempfile.TemporaryDirectory() as root,recorder.Journal(root,CONFIG) as journal:
            with self.assertRaises(recorder.HistoryUnavailable):
                recorder.record_windows(journal,lambda:client,read,1,history_attempts=2,sleep=lambda _:None,emit=lambda _:None)
            self.assertEqual(journal.verify()[0],120)
            self.assertFalse(list((Path(root)/'records').glob('*.json')))
        self.assertEqual(attempts,[120]*3);self.assertTrue(client.closed)

    def test_account_change_during_history_wait_is_rejected_before_retry_read(self):
        client=Client();attempts=[]
        def read(*args):attempts.append(args[2]);raise recorder.HistoryUnavailable('empty')
        def change(_):client.account_info=lambda:types.SimpleNamespace(server='fixture',currency='USD',leverage=30,trade_mode=0)
        with tempfile.TemporaryDirectory() as root,recorder.Journal(root,CONFIG) as journal:
            with self.assertRaisesRegex(ValueError,'account context changed'):
                recorder.record_windows(journal,lambda:client,read,1,sleep=change,emit=lambda _:None)
            self.assertEqual(journal.verify()[0],120)
        self.assertEqual(attempts,[120]);self.assertTrue(client.closed)

    def test_cli_empty_history_fails_without_committing_or_retrying(self):
        import subprocess,json
        code="""import sys,types,runpy
sys.path.insert(0,sys.argv[1])
class Client:
 TIMEFRAME_M1=1;COPY_TICKS_ALL=0
 def __init__(self,**kwargs):pass
 def initialize(self):return True
 def close(self):pass
 def account_info(self):return types.SimpleNamespace(server='fixture',currency='EUR',leverage=30,trade_mode=0)
 def symbol_info_tick(self,s):return types.SimpleNamespace(time=600)
 def symbol_info(self,s):
  names=('name','digits','point','trade_contract_size','trade_tick_size','volume_min','volume_max','volume_step','currency_base','currency_profit','currency_margin','trade_calc_mode')
  return types.SimpleNamespace(**dict.fromkeys(names,1))
 def copy_rates_range(self,*args):return None
 def copy_ticks_range(self,*args):return None
module=types.ModuleType('siliconmetatrader5');module.MetaTrader5=Client
sys.modules['siliconmetatrader5']=module
root=sys.argv[2];script=sys.argv[1]+'/mt5_record.py'
sys.argv=[script,'--symbols','EURUSD','--server','fixture','--broker-start','120','--out',root,'--history-attempts','0']
runpy.run_path(script,run_name='__main__')
"""
        with tempfile.TemporaryDirectory() as root:
            run=subprocess.run([sys.executable,'-c',code,str(ROOT),root],capture_output=True,text=True,timeout=10)
            self.assertEqual(run.returncode,1)
            error=json.loads(run.stderr)
            self.assertEqual(error['status'],'recording_failed')
            self.assertIn('empty or failed history response',error['message'])
            self.assertEqual(run.stdout,'')
            with recorder.Journal(root,CONFIG) as journal:self.assertEqual(journal.verify()[0],120)
            self.assertFalse(list((Path(root)/'records').glob('*.json')))
