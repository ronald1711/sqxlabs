#!/usr/bin/env python3
"""Offline M1 bid-candle consistency check; not proof of complete broker ticks."""
import argparse
from bisect import bisect_right
from decimal import Decimal
import hashlib
import json
from pathlib import Path


def check(capture):
    start, end = capture['broker_window_seconds']
    if start % 60 or end % 60 or end <= start:
        raise ValueError('Expected an aligned positive M1 capture range')
    rows = []; symbols = set()
    for market in capture['markets']:
        if market['symbol'] in symbols:
            raise ValueError('Duplicate market symbol')
        symbols.add(market['symbol'])
        bars = market['raw_bars']; ticks = market['raw_ticks']
        if [b['time'] for b in bars] != list(range(start, end, 60)):
            raise ValueError('Missing, duplicate or unordered candles: ' + market['symbol'])
        previous = None; buckets = {}
        for tick in ticks:
            time = tick['time_msc']
            if not start * 1000 <= time < end * 1000 or (previous is not None and time < previous):
                raise ValueError('Out-of-range or unordered ticks: ' + market['symbol'])
            if tick['time'] != time // 1000:
                raise ValueError('Inconsistent tick timestamps')
            bid, ask = (Decimal(str(tick[k])) for k in ['bid', 'ask'])
            if not bid.is_finite() or not ask.is_finite() or bid <= 0 or ask < bid:
                raise ValueError('Invalid bid/ask')
            previous = time
            buckets.setdefault(time // 60000 * 60, []).append(bid)
        for bar in bars:
            prices = buckets.get(bar['time'], [])
            observed = dict(zip(['open', 'high', 'low', 'close'],
                                [prices[0], max(prices), min(prices), prices[-1]])) if prices else {}
            mismatches = [key for key in ['open', 'high', 'low', 'close']
                          if key not in observed or Decimal(str(bar[key])) != observed[key]]
            rows.append({'symbol': market['symbol'], 'broker_time': bar['time'],
                         'observed_ticks': len(prices), 'reported_tick_volume': bar['tick_volume'],
                         'mismatched_fields': mismatches})
    if not rows:
        raise ValueError('No markets/candles to check')
    return {'status': 'consistent' if all(not r['mismatched_fields'] for r in rows) else 'discrepancies',
            'bars_checked': len(rows), 'rows': rows,
            'scope': 'Exact bid OHLC agreement with observed ticks in response order. Equal-time ticks are preserved. '
                     'Agreement does not establish tick completeness, authentic broker origin, UTC normalization, '
                     'valid conversion quotes, or executable fills. Tick-volume counts are informational.'}


def conversion_coverage(capture, event_symbol, conversion_symbols, max_age_ms):
    """Availability at every observed event tick, including while a strategy is flat."""
    check(capture)
    if not isinstance(max_age_ms, int) or isinstance(max_age_ms, bool) or max_age_ms < 0:
        raise ValueError('Conversion age limit must be a nonnegative integer')
    if not conversion_symbols or len(set(conversion_symbols)) != len(conversion_symbols):
        raise ValueError('Specify distinct conversion symbols')
    markets = {m['symbol']: m for m in capture['markets']}
    events = markets[event_symbol]['raw_ticks']
    if not events:
        raise ValueError('No event ticks for conversion coverage')
    result = []
    for symbol in conversion_symbols:
        market = markets[symbol]
        quotes = market.get('raw_warmup_ticks', []) + market['raw_ticks']
        times = []
        for quote in quotes:
            time = quote['time_msc']
            bid, ask = (Decimal(str(quote[k])) for k in ['bid', 'ask'])
            if times and time < times[-1]:
                raise ValueError('Unordered conversion quotes')
            if not bid.is_finite() or not ask.is_finite() or bid <= 0 or ask < bid:
                raise ValueError('Invalid conversion quote')
            times.append(time)
        missing = stale = 0; maximum = None; first_failure = None
        for event in events:
            time = event['time_msc']; index = bisect_right(times, time) - 1
            age = None if index < 0 else time - times[index]
            if age is None: missing += 1
            else:
                maximum = age if maximum is None else max(maximum, age)
                if age > max_age_ms: stale += 1
            if first_failure is None and (age is None or age > max_age_ms):
                first_failure = {'event_broker_time_ms': time, 'age_ms': age}
        result.append({'symbol': symbol, 'events_checked': len(events), 'missing_previous_quote': missing,
                       'stale_quote_events': stale, 'maximum_age_ms': maximum, 'first_failure': first_failure})
    return {'status': 'available' if all(not r['missing_previous_quote'] and not r['stale_quote_events'] for r in result) else 'unavailable_or_stale',
            'event_symbol': event_symbol, 'max_age_ms': max_age_ms, 'symbols': result,
            'scope': 'Last quote at or before each observed event; no future quotes used. '
                     'Raw broker clocks are assumed comparable within this capture. This checks availability, '
                     'not currency-route correctness, conversion arithmetic or actual strategy exposure.'}


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--capture', type=Path, required=True, help='Content-addressed capture directory')
    parser.add_argument('--event-symbol')
    parser.add_argument('--conversion-symbols', help='Comma-separated quote symbols; requires event symbol and age limit')
    parser.add_argument('--max-conversion-age-ms', type=int)
    args = parser.parse_args()
    supplied = [args.event_symbol is not None, args.conversion_symbols is not None, args.max_conversion_age_ms is not None]
    if any(supplied) and not all(supplied):
        parser.error('Supply event symbol, conversion symbols and age limit together')
    raw = (args.capture / 'capture.json').read_bytes()
    digest = hashlib.sha256(raw).hexdigest()
    if digest != args.capture.name:
        raise ValueError('Capture directory hash does not match content')
    capture = json.loads(raw)
    result = check(capture); result['capture_sha256'] = digest
    if all(supplied):
        result['conversion_coverage'] = conversion_coverage(capture, args.event_symbol,
            args.conversion_symbols.split(','), args.max_conversion_age_ms)
        if result['conversion_coverage']['status'] != 'available': result['status'] = 'discrepancies'
    print(json.dumps(result, indent=2, allow_nan=False))
    raise SystemExit(0 if result['status'] == 'consistent' else 1)
