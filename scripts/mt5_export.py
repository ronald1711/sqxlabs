#!/usr/bin/env python3
"""Export raw MT5 broker-clock CSV via SiliconMetaTrader5.

Normalize with mt5_normalize.py and an explicit clock policy before SqxLabs import.

Requires the SiliconMT5 runtime running and the client venv at
~/Library/Application Support/SiliconMT5/venv (has siliconmetatrader5 + numpy).

Usage:
  python scripts/mt5_export.py --symbols EURUSD,GBPUSD --timeframe M5 --count 5000 --out data/mt5
"""
import math
import argparse
import os
import sys
import tempfile
from contextlib import contextmanager

from siliconmetatrader5 import MetaTrader5

TFS = ["M1", "M5", "M15", "M30", "H1", "H4", "D1"]


@contextmanager
def atomic_csv(path):
    """Publish a fully written file; leave previous data intact on failure."""
    temporary = None
    try:
        with tempfile.NamedTemporaryFile(mode="w", encoding="utf-8", newline="",
                                         dir=os.path.dirname(path),
                                         prefix=".mt5-export-", delete=False) as output:
            temporary = output.name
            yield output
            output.flush()
            os.fsync(output.fileno())
        os.replace(temporary, path)
        temporary = None
    finally:
        if temporary is not None:
            os.unlink(temporary)


def integer(value, label):
    parsed = int(value)
    if parsed != value or not -(2**63) <= parsed < 2**63:
        raise ValueError(f"invalid integer {label}")
    return parsed


def tick_lines(rows):
    previous = None
    for row in rows:
        time = integer(row[5], "time_msc")
        seconds = integer(row[0], "time")
        bid, ask = float(row[1]), float(row[2])
        if (time // 1000 != seconds or
                not all(math.isfinite(p) and p > 0 for p in (bid, ask)) or
                ask < bid or (previous is not None and time < previous)):
            raise ValueError("invalid or unordered tick")
        previous = time
        yield f"{time},{bid:.17g},{ask:.17g}\n"


def bar_lines(rows):
    previous = None
    for row in rows:
        time = integer(row[0], "bar time")
        prices = [float(row[i]) for i in range(1, 5)]
        if len(prices) != 4:
            raise ValueError("incomplete OHLC row")
        opening, high, low, close = prices
        volume = integer(row[5], "tick volume")
        if (not all(math.isfinite(p) and p > 0 for p in prices) or volume < 0 or
                low > min(opening, close) or high < max(opening, close) or
                (previous is not None and time <= previous)):
            raise ValueError("invalid or duplicate/unordered candle")
        previous = time
        yield f"{time},{opening:.17g},{high:.17g},{low:.17g},{close:.17g},{volume}\n"


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("--symbols", default="EURUSD,GBPUSD")
    ap.add_argument("--timeframe", default="M5", choices=TFS)
    ap.add_argument("--count", type=int, default=5000)
    ap.add_argument("--out", default="data/mt5")
    ap.add_argument("--ticks", action="store_true", help="export ticks instead of bars")
    ap.add_argument("--host", default="localhost")
    ap.add_argument("--port", type=int, default=8001)
    args = ap.parse_args()
    symbols = [s.strip() for s in args.symbols.split(",") if s.strip()]
    if not symbols:
        ap.error("at least one symbol is required")
    if args.count <= 0:
        ap.error("count must be positive")

    mt5 = MetaTrader5(host=args.host, port=args.port)
    try:
        if not mt5.initialize():
            print("initialize failed:", mt5.last_error(), file=sys.stderr)
            return 1
        os.makedirs(args.out, exist_ok=True)
        tf = getattr(mt5, "TIMEFRAME_" + args.timeframe)
        total = 0
        failed = False
        if args.ticks:
            from datetime import datetime, timezone
            for sym in symbols:
                if not mt5.symbol_select(sym, True):
                    print(f"{sym}: not selectable", file=sys.stderr)
                    failed = True
                    continue
                tk = mt5.copy_ticks_from(sym, datetime(2000, 1, 1, tzinfo=timezone.utc), args.count, mt5.COPY_TICKS_ALL)
                if tk is None or len(tk) == 0:
                    print(f"{sym}: no ticks ({mt5.last_error()})", file=sys.stderr)
                    failed = True
                    continue
                path = os.path.join(args.out, f"{sym}_ticks.csv")
                with atomic_csv(path) as f:
                    f.write("timestamp_broker_ms,bid,ask\n")
                    f.writelines(tick_lines(tk))
                print(f"{sym}: {len(tk)} ticks -> {path}")
                total += len(tk)
            print(f"{'partial export' if failed else 'done'}: {total} ticks")
            return 1 if failed else 0
        for sym in symbols:
            if not mt5.symbol_select(sym, True):
                print(f"{sym}: not selectable ({mt5.last_error()})", file=sys.stderr)
                failed = True
                continue
            rates = mt5.copy_rates_from_pos(sym, tf, 1, args.count)
            if rates is None or len(rates) == 0:
                print(f"{sym}: no rates ({mt5.last_error()})", file=sys.stderr)
                failed = True
                continue
            path = os.path.join(args.out, f"{sym}_{args.timeframe}.csv")
            with atomic_csv(path) as f:
                f.write("timestamp_broker,open,high,low,close,volume\n")
                f.writelines(bar_lines(rates))
            print(f"{sym}: {len(rates)} bars -> {path}")
            total += len(rates)
        print(f"{'partial export' if failed else 'done'}: {total} bars")
        return 1 if failed else 0
    finally:
        # Only disconnect this client; shutdown() affects every bridge user.
        mt5.close()


if __name__ == "__main__":
    sys.exit(main())
