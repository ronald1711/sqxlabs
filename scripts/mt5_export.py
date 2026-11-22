#!/usr/bin/env python3
"""Export MT5 historical bars to SqxLabs CSV via SiliconMetaTrader5.

Requires the SiliconMT5 runtime running and the client venv at
~/Library/Application Support/SiliconMT5/venv (has siliconmetatrader5 + numpy).

Usage:
  python scripts/mt5_export.py --symbols EURUSD,GBPUSD --timeframe M5 --count 5000 --out data/mt5
"""
import argparse
import os
import sys

from siliconmetatrader5 import MetaTrader5

TFS = ["M1", "M5", "M15", "M30", "H1", "H4", "D1"]


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("--symbols", default="EURUSD,GBPUSD")
    ap.add_argument("--timeframe", default="M5", choices=TFS)
    ap.add_argument("--count", type=int, default=5000)
    ap.add_argument("--out", default="data/mt5")
    ap.add_argument("--host", default="localhost")
    ap.add_argument("--port", type=int, default=8001)
    args = ap.parse_args()

    mt5 = MetaTrader5(host=args.host, port=args.port)
    if not mt5.initialize():
        print("initialize failed:", mt5.last_error(), file=sys.stderr)
        return 1
    os.makedirs(args.out, exist_ok=True)
    tf = getattr(mt5, "TIMEFRAME_" + args.timeframe)
    total = 0
    for sym in [s.strip() for s in args.symbols.split(",") if s.strip()]:
        if not mt5.symbol_select(sym, True):
            print(f"{sym}: not selectable ({mt5.last_error()})", file=sys.stderr)
            continue
        rates = mt5.copy_rates_from_pos(sym, tf, 0, args.count)
        if rates is None or len(rates) == 0:
            print(f"{sym}: no rates ({mt5.last_error()})", file=sys.stderr)
            continue
        path = os.path.join(args.out, f"{sym}_{args.timeframe}.csv")
        with open(path, "w") as f:
            f.write("timestamp,open,high,low,close,volume\n")
            for r in rates:
                # (time, open, high, low, close, tick_volume, spread, real_volume)
                f.write(f"{int(r[0])},{float(r[1]):.5f},{float(r[2]):.5f},{float(r[3]):.5f},{float(r[4]):.5f},{int(r[5])}\n")
        print(f"{sym}: {len(rates)} bars -> {path}")
        total += len(rates)
    mt5.shutdown()
    print(f"done: {total} bars")
    return 0


if __name__ == "__main__":
    sys.exit(main())
