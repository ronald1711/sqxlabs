import copy
from pathlib import Path
import sys
import unittest
sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from mt5_capture_quality import check, conversion_coverage


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

    def test_conversion_never_borrows_future_quotes_and_enforces_age_boundary(self):
        data = self.fixture(); conversion = copy.deepcopy(data['markets'][0]); conversion['symbol'] = 'FX'
        conversion['raw_ticks'] = [dict(t, time_msc=60002) for t in conversion['raw_ticks']]
        conversion['raw_warmup_ticks'] = [{'time': 59, 'time_msc': 59991, 'bid': 1.1, 'ask': 1.2}]
        data['markets'].append(conversion)
        result = conversion_coverage(data, 'TEST', ['FX'], 10)
        self.assertEqual(result['status'], 'available')
        self.assertEqual(result['symbols'][0]['maximum_age_ms'], 10)
        self.assertEqual(conversion_coverage(data, 'TEST', ['FX'], 9)['symbols'][0]['stale_quote_events'], 2)
        conversion['raw_warmup_ticks'] = []
        result = conversion_coverage(data, 'TEST', ['FX'], 10000)
        self.assertEqual(result['symbols'][0]['missing_previous_quote'], 2)
        self.assertIsNone(result['symbols'][0]['maximum_age_ms'])


if __name__ == '__main__':
    unittest.main()
