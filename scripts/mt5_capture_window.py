#!/usr/bin/env python3
"""Read-only, bounded, common broker-clock window capture; never sends orders.

The window is [start,end), in explicit broker epoch seconds. No timezone inference.
Records source observations, not proof of complete broker history.
"""
import argparse
import hashlib
import json
import os
from pathlib import Path
import shutil
import tempfile
import time

from mt5_export import bar_lines, tick_lines


def capture(client, symbols, start, end, warmup_seconds=60):
    if type(start) is not int or type(end) is not int or start % 60 or end % 60 or not 0 < end-start <= 3600:
        raise ValueError('require minute-aligned broker seconds and a window of at most one hour')
    if type(warmup_seconds) is not int or not 0 <= warmup_seconds <= 3600:
        raise ValueError('warmup must be 0–3600 seconds')
    if not symbols or len(set(symbols)) != len(symbols) or len(symbols) > 12:
        raise ValueError('require 1–12 distinct symbols')
    before = time.time_ns() // 1_000_000
    account = client.account_info()
    if account is None:
        raise ValueError('account metadata unavailable')
    identity = {k: getattr(account, k) for k in ('server', 'currency', 'leverage', 'trade_mode')}
    markets = []
    for symbol in symbols:
        info = client.symbol_info(symbol)
        latest = client.symbol_info_tick(symbol)
        if info is None or latest is None:
            raise ValueError(f'{symbol}: symbol metadata or latest tick unavailable')
        if end > int(latest.time) // 60 * 60:
            raise ValueError(f'{symbol}: requested window is not closed according to latest broker tick')
        started = time.time_ns() // 1_000_000
        bars = client.copy_rates_range(symbol, client.TIMEFRAME_M1, start, end-1)
        ticks = client.copy_ticks_range(symbol, start-warmup_seconds, end, client.COPY_TICKS_ALL)
        if bars is None or ticks is None or len(bars) == 0 or len(ticks) == 0:
            raise ValueError(f'{symbol}: empty or failed history response')
        # Preserve all source fields and duplicate-millisecond observations.
        def rows(data):
            return [{name: row[name].item() for name in data.dtype.names} for row in data]
        response_ticks = rows(ticks)
        if any(not (start-warmup_seconds)*1000 <= row['time_msc'] < (end+1)*1000 for row in response_ticks):
            raise ValueError(f'{symbol}: response ticks outside requested range')
        # Validate chronology and prices before partitioning the response.
        list(tick_lines(ticks))
        warmup = ticks[ticks['time_msc'] < start*1000]
        ticks = ticks[(ticks['time_msc'] >= start*1000) & (ticks['time_msc'] < end*1000)]
        if len(ticks) == 0: raise ValueError(f'{symbol}: no ticks inside half-open window')
        raw_bars, raw_ticks = rows(bars), rows(ticks)
        if [int(row['time']) for row in raw_bars] != list(range(start, end, 60)):
            raise ValueError(f'{symbol}: incomplete, duplicate or out-of-window candles')
        if any(not start*1000 <= int(row['time_msc']) < end*1000 for row in raw_ticks):
            raise ValueError(f'{symbol}: ticks outside requested window')
        bar_csv = 'timestamp_broker,open,high,low,close,volume\n' + ''.join(bar_lines(bars))
        tick_csv = 'timestamp_broker_ms,bid,ask\n' + ''.join(tick_lines(ticks))
        markets.append({'symbol': symbol, 'spec': {k: getattr(info, k) for k in
            ('name','digits','point','trade_contract_size','trade_tick_size','volume_min','volume_max','volume_step','currency_base','currency_profit','currency_margin','trade_calc_mode')},
            'request_host_utc_ms': [started, time.time_ns() // 1_000_000],
            'raw_bars': raw_bars, 'raw_ticks': raw_ticks, 'raw_tick_response': response_ticks, 'raw_warmup_ticks': rows(warmup),
            'warmup_ticks_csv':'timestamp_broker_ms,bid,ask\n'+''.join(tick_lines(warmup)), 'bars_csv': bar_csv, 'ticks_csv': tick_csv})
    after_account = client.account_info()
    if after_account is None or identity != {k: getattr(after_account,k) for k in identity}:
        raise ValueError('account context changed during capture')
    return {'schema':1, 'account':identity, 'broker_window_seconds':[start,end], 'tick_warmup_seconds':warmup_seconds,
            'host_capture_utc_ms':[before,time.time_ns() // 1_000_000], 'markets':markets,
            'scope':'Common requested historical window; sequential reads, not atomic snapshots. Clock policy, complete ticks, conversion freshness and execution parity are not certified.'}


def publish(payload, root):
    data=json.dumps(payload,sort_keys=True,separators=(',', ':'),allow_nan=False).encode()
    identifier=hashlib.sha256(data).hexdigest()
    root=Path(root);root.mkdir(parents=True,exist_ok=True);destination=root/identifier
    if destination.exists():
        if (destination/'capture.json').read_bytes()!=data: raise ValueError('capture integrity mismatch')
        return destination
    temporary=Path(tempfile.mkdtemp(prefix='.capture-',dir=root))
    try:
        with (temporary/'capture.json').open('wb') as output:
            output.write(data);output.flush();os.fsync(output.fileno())
        temporary.rename(destination)
    finally:
        if temporary.exists(): shutil.rmtree(temporary)
    return destination


def main():
    from siliconmetatrader5 import MetaTrader5
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--symbols',required=True)
    parser.add_argument('--broker-start',type=int,required=True)
    parser.add_argument('--broker-end',type=int,required=True)
    parser.add_argument('--out',required=True)
    parser.add_argument('--warmup-seconds',type=int,default=60)
    args=parser.parse_args()
    client=MetaTrader5(host='127.0.0.1',port=8001,keepalive=False,timeout=30)
    try:
        if not client.initialize(): raise RuntimeError('MT5 initialization failed')
        payload=capture(client,[s.strip() for s in args.symbols.split(',')],args.broker_start,args.broker_end,args.warmup_seconds)
        print(json.dumps({'capture':str(publish(payload,args.out)), 'markets':len(payload['markets'])}))
    finally:
        client.close()


if __name__=='__main__':
    main()
