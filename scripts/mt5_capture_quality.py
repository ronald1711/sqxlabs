#!/usr/bin/env python3
"""Offline M1 bid-candle consistency check; not proof of complete broker ticks."""
import argparse
from decimal import Decimal
import hashlib
import json
from pathlib import Path


def check(capture):
    start, end = capture['broker_window_seconds']
    if start % 60 or end % 60 or end <= start:
        raise ValueError('Expected an aligned positive M1 capture range')
    rows = []
    for market in capture['markets']:
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


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--capture', type=Path, required=True, help='Content-addressed capture directory')
    args = parser.parse_args()
    raw = (args.capture / 'capture.json').read_bytes()
    digest = hashlib.sha256(raw).hexdigest()
    if digest != args.capture.name:
        raise ValueError('Capture directory hash does not match content')
    result = check(json.loads(raw)); result['capture_sha256'] = digest
    print(json.dumps(result, indent=2, allow_nan=False))
    raise SystemExit(0 if result['status'] == 'consistent' else 1)
