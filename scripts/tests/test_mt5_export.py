"""Offline transport stub: verify CSV price preservation, not broker parity."""
import contextlib
import importlib.util
import io
from pathlib import Path
import sys
import tempfile
import types
import unittest
from unittest.mock import patch


class ExportPrecision(unittest.TestCase):
    def test_bar_and_tick_prices_round_trip_without_five_decimal_quantization(self):
        prices = [1.123456789012345, 1.123456789012349, 1.123456789012341,
                  1.123456789012347]
        class Client:
            TIMEFRAME_M5 = 5
            COPY_TICKS_ALL = 0
            def __init__(self, **kwargs): pass
            def initialize(self): return True
            def symbol_select(self, *args): return True
            def shutdown(self): pass
            def copy_rates_from_pos(self, *args):
                assert args[2] == 1, "must exclude current candle"
                return [(1700000000, *prices, 17, 0, 0)]
            def copy_ticks_from(self, *args):
                return [(1700000000, prices[0], prices[1], 0.0, 0, 1700000000123, 0, 0.0)]
        module = types.ModuleType('siliconmetatrader5')
        module.MetaTrader5 = Client
        path = Path(__file__).resolve().parents[1] / 'mt5_export.py'
        spec = importlib.util.spec_from_file_location('export_under_test', path)
        exporter = importlib.util.module_from_spec(spec)
        with patch.dict(sys.modules, {'siliconmetatrader5': module}):
            spec.loader.exec_module(exporter)
        with tempfile.TemporaryDirectory() as directory:
            for ticks in [False, True]:
                args = ['mt5_export', '--symbols', 'EURUSD', '--out', directory]
                if ticks: args.append('--ticks')
                with patch.object(sys, 'argv', args), contextlib.redirect_stdout(io.StringIO()):
                    self.assertEqual(exporter.main(), 0)
                name = 'EURUSD_ticks.csv' if ticks else 'EURUSD_M5.csv'
                lines = (Path(directory) / name).read_text().splitlines()
                fields = lines[1].split(',')
                expected = prices[:2] if ticks else prices
                self.assertEqual([float(v) for v in fields[1:1+len(expected)]], expected)
                self.assertEqual(fields[0], '1700000000123' if ticks else '1700000000')
                if ticks: self.assertEqual(lines[0], 'timestamp_ms,bid,ask')


    def test_failed_tick_export_preserves_previous_file_and_closes_client(self):
        closed = []
        class Client:
            TIMEFRAME_M5 = 5
            COPY_TICKS_ALL = 0
            def __init__(self, **kwargs): pass
            def initialize(self): return True
            def symbol_select(self, *args): return True
            def shutdown(self): closed.append(True)
            def copy_ticks_from(self, *args):
                # First row writes successfully; the second lacks time_msc.
                return [(1700000000, 1.1, 1.2, 0, 0, 1700000000123),
                        (1700000001, 1.1, 1.2)]
        module = types.ModuleType('siliconmetatrader5')
        module.MetaTrader5 = Client
        path = Path(__file__).resolve().parents[1] / 'mt5_export.py'
        spec = importlib.util.spec_from_file_location('export_failure_test', path)
        exporter = importlib.util.module_from_spec(spec)
        with patch.dict(sys.modules, {'siliconmetatrader5': module}):
            spec.loader.exec_module(exporter)
        with tempfile.TemporaryDirectory() as directory:
            target = Path(directory) / 'EURUSD_ticks.csv'
            previous = b'previous complete dataset\n'
            target.write_bytes(previous)
            args = ['mt5_export', '--symbols', 'EURUSD', '--out', directory, '--ticks']
            with patch.object(sys, 'argv', args), self.assertRaises(IndexError):
                exporter.main()
            self.assertEqual(target.read_bytes(), previous)
            self.assertEqual(list(Path(directory).glob('.mt5-export-*')), [])
            self.assertEqual(closed, [True])
            # Publication failure must also leave the original intact.
            with patch.object(exporter.os, 'replace', side_effect=OSError('disk error')):
                with self.assertRaises(OSError), exporter.atomic_csv(str(target)) as output:
                    output.write('replacement')
            self.assertEqual(target.read_bytes(), previous)
            self.assertEqual(list(Path(directory).glob('.mt5-export-*')), [])


    def test_partial_exports_fail_for_missing_or_unselectable_symbols(self):
        class Client:
            TIMEFRAME_M5 = 5
            COPY_TICKS_ALL = 0
            def __init__(self, **kwargs): pass
            def initialize(self): return True
            def shutdown(self): pass
            def last_error(self): return 'unavailable'
            def symbol_select(self, symbol, selected): return symbol != 'MISSING'
            def copy_rates_from_pos(self, symbol, *args):
                return [] if symbol == 'EMPTY' else [(1700000000, 1, 1, 1, 1, 1)]
            def copy_ticks_from(self, symbol, *args):
                return None if symbol == 'EMPTY' else [(1700000000, 1, 1.1, 0, 0, 1700000000123)]
        module = types.ModuleType('siliconmetatrader5'); module.MetaTrader5 = Client
        spec = importlib.util.spec_from_file_location('export_partial_test', Path(__file__).resolve().parents[1] / 'mt5_export.py')
        exporter = importlib.util.module_from_spec(spec)
        with patch.dict(sys.modules, {'siliconmetatrader5': module}): spec.loader.exec_module(exporter)
        for ticks in [False, True]:
            for missing in ['MISSING', 'EMPTY']:
                with tempfile.TemporaryDirectory() as directory:
                    args = ['mt5_export', '--symbols', 'EURUSD,' + missing, '--out', directory]
                    if ticks: args.append('--ticks')
                    output = io.StringIO()
                    with patch.object(sys, 'argv', args), contextlib.redirect_stdout(output), contextlib.redirect_stderr(io.StringIO()):
                        self.assertEqual(exporter.main(), 1)
                    self.assertIn('partial export', output.getvalue())
                    self.assertEqual(len(list(Path(directory).glob('*.csv'))), 1)


    def test_invalid_market_rows_cannot_replace_existing_data(self):
        module = types.ModuleType('siliconmetatrader5'); module.MetaTrader5 = object
        spec = importlib.util.spec_from_file_location('export_quality_test', Path(__file__).resolve().parents[1] / 'mt5_export.py')
        exporter = importlib.util.module_from_spec(spec)
        with patch.dict(sys.modules, {'siliconmetatrader5': module}): spec.loader.exec_module(exporter)
        bar = (1700000000, 1.0, 1.1, 0.9, 1.0, 10)
        tick = (1700000000, 1.0, 1.1, 0, 0, 1700000000123)
        cases = [
            (exporter.bar_lines, [bar, bar]),
            (exporter.bar_lines, [(1700000000, 1.0, 0.8, 0.9, 1.0, 10)]),
            (exporter.bar_lines, [(1700000000, 1.0, 1.1, 0.9, float('nan'), 10)]),
            (exporter.bar_lines, [(1700000000, 1.0, 1.1, 0.9, 1.0, -1)]),
            (exporter.tick_lines, [tick, (1700000000, 1.0, 1.1, 0, 0, 1700000000100)]),
            (exporter.tick_lines, [(1700000000, 1.1, 1.0, 0, 0, 1700000000123)]),
            (exporter.tick_lines, [(1700000000, 1.0, float('inf'), 0, 0, 1700000000123)]),
            (exporter.tick_lines, [(1700000000, 1.0, 1.1, 0, 0, 1700000001123)]),
        ]
        with tempfile.TemporaryDirectory() as directory:
            target = Path(directory) / 'market.csv'; target.write_text('previous')
            for formatter, rows in cases:
                with self.assertRaises(ValueError), exporter.atomic_csv(str(target)) as output:
                    output.writelines(formatter(rows))
                self.assertEqual(target.read_text(), 'previous')
            # Equal tick timestamps retain their supplied ordering; candles must be unique.
            self.assertEqual(len(list(exporter.tick_lines([tick, tick]))), 2)


if __name__ == '__main__':
    unittest.main()
