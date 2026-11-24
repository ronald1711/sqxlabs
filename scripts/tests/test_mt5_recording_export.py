import importlib.util
import json
from pathlib import Path
import sys
import tempfile
import unittest
from unittest.mock import patch

ROOT=Path(__file__).resolve().parents[1]
with patch.object(sys,'path',[str(ROOT),*sys.path]):
    spec=importlib.util.spec_from_file_location('recording_export_under_test',ROOT/'mt5_recording_export.py')
    exporter=importlib.util.module_from_spec(spec);spec.loader.exec_module(exporter)
CONFIG={'schema':1,'symbols':['EURUSD'],'server':'fixture','broker_start':120,'window_seconds':60,'warmup_seconds':60}
def capture(start):
    ticks=[{'time_msc':start*1000,'bid':1,'ask':1.1},{'time_msc':start*1000,'bid':1.01,'ask':1.11}]
    warmup=[{'time_msc':(start-1)*1000,'bid':0.99,'ask':1}]
    return {'schema':1,'broker_window_seconds':[start,start+60],'tick_warmup_seconds':60,'account':{'server':'fixture','currency':'EUR'},'host_capture_utc_ms':[start*1000,(start+60)*1000],'markets':[{'symbol':'EURUSD','spec':{'point':0.0001},'request_host_utc_ms':[start*1000,(start+60)*1000],'raw_bars':[{'time':start}],'raw_ticks':ticks,'raw_warmup_ticks':warmup,'raw_tick_response':warmup+ticks+[{'endpoint_marker':True}],'warmup_ticks_csv':f'timestamp_broker_ms,bid,ask\n{(start-1)*1000},0.99,1\n','ticks_csv':f'timestamp_broker_ms,bid,ask\n{start*1000},1,1.1\n{start*1000},1.01,1.11\n','bars_csv':f'timestamp_broker,open,high,low,close,volume\n{start},1,1.01,1,1.01,2\n'}]}

class RecordingExport(unittest.TestCase):
    def test_subset_preserves_ties_first_warmup_and_original_responses(self):
        with tempfile.TemporaryDirectory() as directory:
            root=Path(directory)/'journal';out=Path(directory)/'out'
            with exporter.Journal(root,CONFIG) as journal:
                for start in (120,180,240):journal.append(capture(start))
            target=exporter.export(root,180,300,out)
            result=json.loads((target/'capture.json').read_bytes());market=result['markets'][0]
            self.assertEqual(result['broker_window_seconds'],[180,300])
            self.assertEqual([t['time_msc'] for t in market['raw_ticks']],[180000,180000,240000,240000])
            self.assertEqual(market['raw_warmup_ticks'],capture(180)['markets'][0]['raw_warmup_ticks'])
            self.assertEqual(len(market['ticks_csv'].splitlines()),5)
            self.assertEqual([w['capture'] for w in result['recording_provenance']['windows']],[capture(180),capture(240)])
            self.assertEqual(exporter.export(root,180,300,out),target)
            for start,end in [(181,300),(180,360),(180,180)]:
                with self.assertRaises(ValueError):exporter.export(root,start,end,out)

    def test_corruption_is_rejected_before_output_publication(self):
        with tempfile.TemporaryDirectory() as directory:
            root=Path(directory)/'journal';out=Path(directory)/'out'
            with exporter.Journal(root,CONFIG) as journal:identifier=journal.append(capture(120))
            (root/'captures'/f'{identifier}.json').write_text('corrupt')
            with self.assertRaisesRegex(ValueError,'integrity'):exporter.export(root,120,180,out)
            self.assertFalse(out.exists())

    def test_active_writer_snapshot_excludes_orphan_and_can_advance_later(self):
        with tempfile.TemporaryDirectory() as directory:
            root=Path(directory)/'journal';out=Path(directory)/'out'
            with exporter.Journal(root,CONFIG) as writer:
                writer.append(capture(120))
                # Simulate data published before its commit marker.
                blob=exporter.encode(capture(180))
                import hashlib
                orphan=root/'captures'/f'{hashlib.sha256(blob).hexdigest()}.json'
                exporter.publish(orphan,blob)
                with self.assertRaisesRegex(ValueError,'uncommitted'):exporter.export(root,120,240,out)
                first=exporter.export(root,120,180,out)
                first_bytes=(first/'capture.json').read_bytes()
                writer.append(capture(180))
                second=exporter.export(root,120,240,out)
                self.assertNotEqual(first,second)
                self.assertEqual((first/'capture.json').read_bytes(),first_bytes)
                self.assertEqual(len(json.loads((second/'capture.json').read_bytes())['recording_provenance']['windows']),2)

    def test_separate_writer_process_can_commit_during_reader_lifetime(self):
        import subprocess,select
        with tempfile.TemporaryDirectory() as directory:
            root=Path(directory)/'journal';out=Path(directory)/'out'
            code='''import sys,json
sys.path.insert(0,sys.argv[1])
from mt5_capture_journal import Journal
with Journal(sys.argv[2],json.loads(sys.argv[3])) as writer:
 writer.append(json.loads(sys.argv[4]))
 print('first committed',flush=True)
 if input()!='next':raise RuntimeError('unexpected command')
 writer.append(json.loads(sys.argv[5]))
 print('second committed',flush=True)
 input()
'''
            child=subprocess.Popen([sys.executable,'-c',code,str(ROOT),str(root),json.dumps(CONFIG),json.dumps(capture(120)),json.dumps(capture(180))],stdin=subprocess.PIPE,stdout=subprocess.PIPE,stderr=subprocess.PIPE,text=True)
            def receive():
                self.assertTrue(select.select([child.stdout],[],[],10)[0],'writer response timed out')
                return child.stdout.readline().strip()
            try:
                self.assertEqual(receive(),'first committed')
                self.assertIsNone(child.poll())
                first=exporter.export(root,120,180,out)
                with self.assertRaisesRegex(ValueError,'uncommitted'):exporter.export(root,120,240,out)
                child.stdin.write('next\n');child.stdin.flush()
                self.assertEqual(receive(),'second committed')
                self.assertIsNone(child.poll())
                second=exporter.export(root,120,240,out)
                self.assertEqual(len(json.loads((first/'capture.json').read_bytes())['recording_provenance']['windows']),1)
                self.assertEqual(len(json.loads((second/'capture.json').read_bytes())['recording_provenance']['windows']),2)
                child.stdin.write('done\n');child.stdin.flush()
                self.assertEqual(child.wait(timeout=10),0)
            finally:
                if child.poll() is None:child.kill();child.wait()
                child.stdin.close();child.stdout.close();child.stderr.close()

    def test_invalid_stored_window_is_rejected_before_range_arithmetic(self):
        with tempfile.TemporaryDirectory() as directory:
            root=Path(directory)/'journal';root.mkdir();out=Path(directory)/'out'
            (root/'config.json').write_text(json.dumps({**CONFIG,'window_seconds':0}))
            with self.assertRaisesRegex(ValueError,'invalid recording window'):
                exporter.export(root,120,180,out)
            self.assertFalse(out.exists())

    def test_snapshot_api_rejects_invalid_bounds_without_writes(self):
        with tempfile.TemporaryDirectory() as directory:
            root=Path(directory)/'journal'
            with exporter.Journal(root,CONFIG) as writer:
                writer.append(capture(120))
                for start,end in [(True,180),(120,180.0),(0,180),(121,180),(120,120),(120,3780)]:
                    with self.assertRaises(ValueError):writer.snapshot(start,end)
                self.assertEqual(writer.verify()[0],180)
