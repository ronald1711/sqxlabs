"""Synthetic journal tests; these are not evidence of an actual MT5 run."""
import sys
import hashlib
import json
import subprocess
import tempfile
from pathlib import Path
import unittest

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from mt5_tester_log_check import check

LONG = '''SQX_REFERENCE_INIT symbol=SQX_EURUSD short=0 signal_bar=1767571200 lots=0.10
SQX_REFERENCE_ACCOUNT currency=USD balance=10000.00000000 leverage=30 margin_mode=2
SQX_REFERENCE_SYMBOL base=EUR profit=USD margin=EUR contract=100000 point=0.00001 tick_size=0.00001 calculation=0
SQX_REFERENCE_ORDER closing=0 time_msc=1767571260000 bid=1.10010 ask=1.10030 retcode=10009 deal=2 price=1.10030 volume=0.10
SQX_REFERENCE_ORDER closing=1 time_msc=1767571320000 bid=1.10020 ask=1.10040 retcode=10009 deal=3 price=1.10020 volume=0.10
SQX_REFERENCE_DEAL ticket=1 time_msc=1767571200000 type=2 entry=0 position=0 price=0 volume=0 profit=10000 commission=0 swap=0 fee=0
SQX_REFERENCE_DEAL ticket=2 time_msc=1767571260000 type=0 entry=0 position=2 price=1.10030 volume=0.10 profit=0 commission=0 swap=0 fee=0
SQX_REFERENCE_DEAL ticket=3 time_msc=1767571320000 type=1 entry=1 position=2 price=1.10020 volume=0.10 profit=-1 commission=0 swap=0 fee=0
SQX_REFERENCE_RECONCILIATION market_deals=2 net=-1 residual=0
SQX_REFERENCE_COMPLETE ok=1 balance=9999 equity=9999
'''


class TesterLogCheck(unittest.TestCase):
    def test_reported_long_and_short_arithmetic(self):
        result = check(LONG)
        self.assertEqual(result['status'], 'reported_baseline_values_match')
        self.assertFalse(result['sqxlabs_parity_verified'])
        self.assertFalse(result['tick_provenance_verified'])
        short = LONG.replace('short=0', 'short=1').replace('price=1.10030', 'price=1.10010').replace('price=1.10020', 'price=1.10040').replace('type=0 entry=0 position=2', 'type=1 entry=0 position=2').replace('type=1 entry=1', 'type=0 entry=1').replace('profit=-1', 'profit=-3').replace('net=-1', 'net=-3').replace('9999', '9997')
        self.assertEqual(check(short, True)['side'], 'short')
        with self.assertRaises(ValueError):
            check(short)

    def test_versioned_symbol_must_be_explicit(self):
        changed = LONG.replace('SQX_EURUSD', 'SQX_EURUSD_V2')
        self.assertEqual(check(changed, symbol='SQX_EURUSD_V2')['status'], 'reported_baseline_values_match')
        with self.assertRaises(ValueError):
            check(changed)
        with self.assertRaises(ValueError):
            check(LONG, symbol='EURUSD')

    def test_command_line_encoding_hash_and_failure_status(self):
        script = Path(__file__).resolve().parents[1] / 'mt5_tester_log_check.py'
        with tempfile.TemporaryDirectory() as directory:
            log = Path(directory) / 'tester.log'
            for encoding in ['utf-8', 'utf-8-sig', 'utf-16']:
                data = LONG.encode(encoding)
                log.write_bytes(data)
                run = subprocess.run([sys.executable, str(script), str(log)], capture_output=True, text=True)
                self.assertEqual(run.returncode, 0, run.stderr)
                self.assertEqual(json.loads(run.stdout)['log_sha256'], hashlib.sha256(data).hexdigest())
            for data in [b'no reference run', bytes([255, 254, 0])]:
                log.write_bytes(data)
                run = subprocess.run([sys.executable, str(script), str(log)], capture_output=True, text=True)
                self.assertEqual(run.returncode, 1, run.stderr)
                self.assertEqual(json.loads(run.stdout)['status'], 'rejected')

    def test_reject_incomplete_duplicate_and_mixed_runs(self):
        for text in ['', LONG + LONG, '\n'.join(reversed(LONG.splitlines())), LONG.replace('SQX_REFERENCE_INIT', 'OTHER_INIT'),
                     LONG + 'SQX_REFERENCE_UNKNOWN ok=1', LONG.replace('lots=0.10', 'lots=0.10 lots=0.10')]:
            with self.subTest(text=text[:50]), self.assertRaises(ValueError):
                check(text)

    def test_reject_material_execution_and_configuration_changes(self):
        mutations = [('currency=USD', 'currency=EUR'), ('leverage=30', 'leverage=500'),
                     ('contract=100000', 'contract=1000'), ('retcode=10009', 'retcode=10010'),
                     ('deal=3', 'deal=2'), ('position=2', 'position=0'),
                     ('1767571260000', '1767571261000'), ('price=1.10030', 'price=1.10031'),
                     ('commission=0', 'commission=-0.35'), ('equity=9999', 'equity=NaN'),
                     ('ok=1', 'ok=0'), ('residual=0', 'residual=1'),
                     ('ticket=3', 'ticket=2'), ('profit=-1', 'profit=-150')]
        for before, after in mutations:
            with self.subTest(change=(before, after)), self.assertRaises(ValueError):
                check(LONG.replace(before, after))


if __name__ == '__main__':
    unittest.main()
