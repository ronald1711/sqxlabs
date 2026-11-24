import importlib.util
import json
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest
from unittest.mock import patch

PATH=Path(__file__).resolve().parents[1]/'mt5_capture_journal.py'
spec=importlib.util.spec_from_file_location('journal_under_test',PATH)
journal=importlib.util.module_from_spec(spec);spec.loader.exec_module(journal)
CONFIG={'schema':1,'symbols':['EURUSD'],'server':'fixture','broker_start':120,'window_seconds':60,'warmup_seconds':60}
def payload(start=120):
    return {'schema':1,'broker_window_seconds':[start,start+60],'tick_warmup_seconds':60,'account':{'server':'fixture','currency':'EUR','leverage':30},'markets':[{'symbol':'EURUSD','spec':{'trade_contract_size':100000},'raw_ticks':[{'time_msc':start*1000,'bid':1.1,'ask':1.10002}]}]}


class RecordingJournal(unittest.TestCase):
    def test_resume_identity_lock_and_corruption(self):
        with tempfile.TemporaryDirectory() as root:
            with journal.Journal(root,CONFIG) as writer:
                first=writer.append(payload())
                with self.assertRaises(BlockingIOError):
                    with journal.Journal(root,CONFIG):pass
                wrong=payload(180);wrong['account']['currency']='USD'
                with self.assertRaisesRegex(ValueError,'account context'):writer.append(wrong)
                self.assertEqual(writer.verify()[0],180)
            with journal.Journal(root,CONFIG) as writer:
                writer.append(payload(180));self.assertEqual(writer.verify()[0],240)
            with self.assertRaisesRegex(ValueError,'conflicts'):
                with journal.Journal(root,{**CONFIG,'window_seconds':120}):pass
            (Path(root)/'captures'/f'{first}.json').write_text('broken')
            with self.assertRaisesRegex(ValueError,'integrity'):
                with journal.Journal(root,CONFIG):pass

    def test_process_exit_after_blob_does_not_advance_and_releases_lock(self):
        with tempfile.TemporaryDirectory() as root:
            code='''import sys,os,json
sys.path.insert(0,sys.argv[1])
import mt5_capture_journal as j
original=j.publish
def interrupt(path,data):
 if path.parent.name=='records':os._exit(17)
 original(path,data)
j.publish=interrupt
with j.Journal(sys.argv[2],json.loads(sys.argv[3])) as writer:writer.append(json.loads(sys.argv[4]))
'''
            child=subprocess.run([sys.executable,'-c',code,str(PATH.parent),root,json.dumps(CONFIG),json.dumps(payload())],timeout=10)
            self.assertEqual(child.returncode,17)
            self.assertEqual(len(list((Path(root)/'captures').glob('*.json'))),1)
            with journal.Journal(root,CONFIG) as writer:
                self.assertEqual(writer.verify()[0],120)
                writer.append(payload());self.assertEqual(writer.verify()[0],180)

    def test_missing_middle_window_and_write_failure_do_not_silently_skip(self):
        with tempfile.TemporaryDirectory() as root:
            with journal.Journal(root,CONFIG) as writer:
                with patch.object(journal.os,'link',side_effect=OSError('disk full')):
                    with self.assertRaises(OSError):writer.append(payload())
                self.assertEqual(writer.verify()[0],120)
                for start in (120,180,240):writer.append(payload(start))
            (Path(root)/'records/180.json').unlink()
            with self.assertRaisesRegex(ValueError,'missing or conflicting'):
                with journal.Journal(root,CONFIG):pass

    def test_configuration_is_frozen_and_disk_changes_are_detected(self):
        config=json.loads(json.dumps(CONFIG))
        with tempfile.TemporaryDirectory() as root:
            writer=journal.Journal(root,config)
            config['symbols'][0]='USDJPY';config['window_seconds']=120
            with writer:
                view=writer.config;view['symbols'][0]='GBPUSD'
                self.assertEqual(writer.config,CONFIG)
                writer.append(payload());self.assertEqual(writer.verify()[0],180)
                (Path(root)/'config.json').write_text(json.dumps({**CONFIG,'window_seconds':120}))
                with self.assertRaisesRegex(ValueError,'configuration changed on disk'):writer.append(payload(180))
                self.assertEqual(len(list((Path(root)/'records').glob('*.json'))),1)

    def test_missing_configuration_cannot_reinitialize_existing_capture(self):
        with tempfile.TemporaryDirectory() as root:
            with journal.Journal(root,CONFIG) as writer:writer.append(payload())
            original=(Path(root)/'config.json').read_bytes()
            (Path(root)/'config.json').unlink()
            for config in (CONFIG,{**CONFIG,'broker_start':180}):
                with self.assertRaisesRegex(ValueError,'missing its configuration'):
                    with journal.Journal(root,config):pass
                self.assertFalse((Path(root)/'config.json').exists())
            (Path(root)/'config.json').write_bytes(original)
            with journal.Journal(root,CONFIG) as writer:self.assertEqual(writer.verify()[0],180)

    def test_instrument_change_is_rejected_after_restart(self):
        with tempfile.TemporaryDirectory() as root:
            with journal.Journal(root,CONFIG) as writer:writer.append(payload())
            with journal.Journal(root,CONFIG) as writer:
                changed=payload(180);changed['markets'][0]['spec']['trade_contract_size']=1000
                with self.assertRaisesRegex(ValueError,'instrument specifications changed'):writer.append(changed)
                self.assertEqual(writer.verify()[0],180)
                self.assertEqual(len(list((Path(root)/'records').glob('*.json'))),1)

    def test_status_is_read_only_during_active_writer_and_rejects_corruption(self):
        with tempfile.TemporaryDirectory() as directory:
            root=Path(directory)
            with journal.Journal(root,CONFIG) as writer:
                identifier=writer.append(payload())
                before={str(p.relative_to(root)):p.read_bytes() for p in root.rglob('*') if p.is_file()}
                command=[sys.executable,str(PATH.with_name('mt5_recording_status.py')),'--journal',directory]
                result=subprocess.run(command,capture_output=True,text=True,timeout=10)
                self.assertEqual(result.returncode,0,result.stderr)
                status=json.loads(result.stdout)
                self.assertEqual(status['integrity'],'verified');self.assertEqual(status['committed_windows'],1)
                self.assertEqual(status['next_broker_start'],180)
                self.assertEqual(status['writer_status'],'not_observed')
                after={str(p.relative_to(root)):p.read_bytes() for p in root.rglob('*') if p.is_file()}
                self.assertEqual(before,after)
                (root/'captures'/f'{identifier}.json').write_text('corrupt')
                failed=subprocess.run(command,capture_output=True,text=True,timeout=10)
                self.assertEqual(failed.returncode,1);self.assertEqual(failed.stdout,'')
                self.assertEqual(json.loads(failed.stderr)['integrity'],'not_verified')
