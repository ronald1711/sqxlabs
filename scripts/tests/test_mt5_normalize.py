import importlib.util
import json
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch

spec=importlib.util.spec_from_file_location('normalize',Path(__file__).resolve().parents[1]/'mt5_normalize.py')
module=importlib.util.module_from_spec(spec)
spec.loader.exec_module(module)

class ClockNormalization(unittest.TestCase):
    def policy(self):
        return {'schema':1,'server':'fixture','intervals':[{'raw_start_ms':7200000,'raw_end_ms':7500000,'offset_seconds':7200,'source':'declared fixture'}]}

    def test_ticks_preserve_equal_milliseconds_order_and_price_text(self):
        raw=b'timestamp_broker_ms,bid,ask\n7200123,1.123456789012345,1.123456789012349\n7200123,1.123456789012346,1.123456789012350\n'
        normalized,count=module.convert(raw,self.policy(),'fixture')
        self.assertEqual(count,2)
        self.assertEqual(normalized,raw.replace(b'7200123',b'123').replace(b'timestamp_broker_ms',b'timestamp_ms'))

    def test_utc_headers_cannot_be_silently_normalized_twice(self):
        utc=b'timestamp_ms,bid,ask\n7200000,1,2\n'
        with self.assertRaisesRegex(ValueError,'explicit broker-clock'):
            module.convert(utc,self.policy(),'fixture')
        converted,count=module.convert(utc,self.policy(),'fixture',legacy_broker_clock=True)
        self.assertEqual(converted,b'timestamp_ms,bid,ask\n0,1,2\n')
        self.assertEqual(count,1)

    def test_unknown_transition_and_crossing_candle_are_rejected(self):
        for stamp in [7199999,7500000]:
            with self.assertRaisesRegex(ValueError,'outside supported'):
                module.convert(f'timestamp_broker_ms,bid,ask\n{stamp},1,2\n'.encode(),self.policy(),'fixture')
        with self.assertRaisesRegex(ValueError,'crosses'):
            module.convert(b'timestamp_broker,open,high,low,close,volume\n7499,1,1,1,1,1\n',self.policy(),'fixture',60000)
        with self.assertRaisesRegex(ValueError,'server mismatch'):
            module.convert(b'timestamp_broker_ms,bid,ask\n7200000,1,2\n',self.policy(),'another')

    def test_policy_and_normalized_chronology_cannot_be_ambiguous(self):
        policy=self.policy();policy['intervals'].append(dict(policy['intervals'][0]))
        with self.assertRaisesRegex(ValueError,'overlapping'):
            module.convert(b'timestamp_broker_ms,bid,ask\n7200000,1,2\n',policy,'fixture')
        policy=self.policy();policy['intervals'].append({'raw_start_ms':7500000,'raw_end_ms':8000000,'offset_seconds':10800,'source':'fixture'})
        with self.assertRaisesRegex(ValueError,'normalized clock intervals overlap'):
            module.convert(b'timestamp_broker_ms,bid,ask\n7499999,1,2\n7500000,1,2\n',policy,'fixture')

    def test_raw_policy_and_utc_publish_together_and_verify_on_repeat(self):
        with tempfile.TemporaryDirectory() as directory:
            root=Path(directory);source=root/'source.csv';policy=root/'policy.json'
            raw=b'timestamp_broker,open,high,low,close,volume\n7200,1,1,1,1,1\n'
            source.write_bytes(raw);policy.write_text(json.dumps(self.policy()))
            bundle=module.publish(source,policy,root/'bundles','fixture',60000)
            self.assertEqual((bundle/'raw.csv').read_bytes(),raw)
            self.assertIn(b'\n0,1,1,1,1,1\n',(bundle/'utc.csv').read_bytes())
            self.assertEqual(module.publish(source,policy,root/'bundles','fixture',60000),bundle)
            self.assertEqual(source.read_bytes(),raw)
            (bundle/'utc.csv').write_text('corrupt')
            with self.assertRaisesRegex(ValueError,'integrity'):
                module.publish(source,policy,root/'bundles','fixture',60000)
            # Failed publication must not leave a visible partial bundle.
            with patch.object(module.os,'fsync',side_effect=OSError('fixture disk failure')):
                with self.assertRaises(OSError):module.publish(source,policy,root/'failed','fixture',60000)
            self.assertEqual(list((root/'failed').iterdir()),[])
