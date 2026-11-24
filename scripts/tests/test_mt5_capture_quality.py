import copy
from pathlib import Path
import sys
import unittest
sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from mt5_capture_quality import check


class CaptureQualityTests(unittest.TestCase):
    def fixture(self):
        return {'broker_window_seconds': [60, 120], 'markets': [{'symbol': 'TEST',
            'raw_bars': [{'time': 60, 'open': 1.1, 'high': 1.2, 'low': 1.1, 'close': 1.2, 'tick_volume': 2}],
            'raw_ticks': [{'time': 60, 'time_msc': 60001, 'bid': 1.1, 'ask': 1.3},
                          {'time': 60, 'time_msc': 60001, 'bid': 1.2, 'ask': 1.3}]}]}

    def test_equal_time_ticks_keep_response_order_and_expose_changed_close(self):
        data = self.fixture()
        self.assertEqual(check(data)['status'], 'consistent')
        data['markets'][0]['raw_ticks'].reverse()
        self.assertEqual(check(data)['rows'][0]['mismatched_fields'], ['open', 'close'])

    def test_missing_ticks_cannot_manufacture_a_consistent_candle(self):
        data = self.fixture(); data['markets'][0]['raw_ticks'] = []
        result = check(data)
        self.assertEqual(result['status'], 'discrepancies')
        self.assertEqual(len(result['rows'][0]['mismatched_fields']), 4)

    def test_missing_candles_and_invalid_quotes_are_rejected(self):
        original = self.fixture()
        mutations = [lambda d: d.update(broker_window_seconds=[60, 180]),
                     lambda d: d['markets'][0]['raw_ticks'][0].update(ask=1.0),
                     lambda d: d['markets'][0]['raw_ticks'][0].update(time_msc=120000),
                     lambda d: d['markets'][0]['raw_ticks'][0].update(time=61)]
        for mutate in mutations:
            with self.subTest(mutation=mutate):
                data = copy.deepcopy(original); mutate(data)
                with self.assertRaises(ValueError): check(data)


if __name__ == '__main__':
    unittest.main()
