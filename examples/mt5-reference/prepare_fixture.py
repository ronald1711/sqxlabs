#!/usr/bin/env python3
"""Create frozen synthetic inputs; never connects to MT5 or places orders."""
import argparse
from datetime import datetime, timezone
import hashlib
import json
from pathlib import Path


def prepare(destination: Path):
    destination.mkdir(parents=True, exist_ok=False)
    start = int(datetime(2026, 1, 5, tzinfo=timezone.utc).timestamp())
    price = lambda units: f"{units // 100000}.{units % 100000:05d}"
    bars = ["timestamp,open,high,low,close,volume\n"]
    ticks = ["timestamp_ms,bid,ask\n"]
    for minute in range(120):
        opening = 110000 + (minute % 20) * 10
        values = [opening, opening - 3, opening + 4, opening + 1]
        bars.append(f"{start + minute * 60}," + ",".join(price(v) for v in
                    [values[0], max(values), min(values), values[-1]]) + ",4\n")
        for offset, bid in zip([0, 15, 30, 45], values):
            ticks.append(f"{(start + minute * 60 + offset) * 1000},{price(bid)},{price(bid + 20)}\n")
    (destination / "bars.csv").write_text("".join(bars))
    (destination / "ticks.csv").write_text("".join(ticks))
    for short in [False, True]:
        side = "short" if short else "long"
        strategy = {"name": "mt5-reference-" + side, "long": not short, "short": short,
                    "money_management": {"FixedSize": {"size": 0.1}}, "exits": [],
                    "signals": [{"kind": ("Short" if short else "Long") + event,
                                 "conditions": [{"left": {"Const": 1}, "cmp": "Gt", "right": {"Const": 0}}],
                                 "join": "And", "fuzzy": None} for event in ["Entry", "Exit"]],
                    "trading_options": {"max_trades_per_day": 1, "exit_at_end_of_day": False,
                                        "exit_on_friday": False, "limit_hours": None}}
        (destination / f"{side}.json").write_text(json.dumps(strategy, indent=2) + "\n")
        (destination / f"{side}.set").write_text(f"ShortSide={'true' if short else 'false'}\nSignalBar=2026.01.05 00:00:00\nLots=0.1\n")
        (destination / f"{side}.ini").write_text(
            "[Common]\nNewsEnable=0\n[Experts]\nAllowLiveTrading=0\nAllowDllImport=0\n[Tester]\n"
            f"Expert=constant_signal_exit.ex5\nExpertParameters={side}.set\nSymbol=SQX_EURUSD\n"
            "Period=M1\nModel=4\nExecutionMode=0\nOptimization=0\nFromDate=2026.01.05\nToDate=2026.01.06\n"
            "Deposit=10000\nCurrency=USD\nLeverage=1:30\nUseLocal=1\nUseRemote=0\nUseCloud=0\nVisual=0\n"
            f"Report={side}-reference.htm\nReplaceReport=0\nShutdownTerminal=1\n")
    (destination / "account.json").write_text(json.dumps({"currency": "USD", "max_leverage": 30}, indent=2) + "\n")
    manifest = {"schema": 1, "provenance": "synthetic integer-price fixture, not broker data",
                "symbol_sqxlabs": "EURUSD", "symbol_mt5": "SQX_EURUSD", "timeframe": "M1",
                "contract_size": 100000, "broker_point": 0.00001, "tick_size": 0.00001,
                "account_currency": "USD", "deposit": 10000, "leverage": 30, "lots": 0.1,
                "commission": 0, "slippage": 0, "financing": 0,
                "expected": {"long": {"entry": "1.10030", "exit": "1.10020", "net": "-1.00", "balance": "9999.00"},
                             "short": {"entry": "1.10010", "exit": "1.10040", "net": "-3.00", "balance": "9997.00"}},
                "entry_time_ms": (start + 60) * 1000, "exit_time_ms": (start + 120) * 1000,
                "comparison_status": "not_executed",
                "files": {p.name: hashlib.sha256(p.read_bytes()).hexdigest() for p in sorted(destination.iterdir())}}
    (destination / "manifest.json").write_text(json.dumps(manifest, indent=2) + "\n")


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("destination", type=Path, help="New directory; existing directories are refused")
    prepare(parser.parse_args().destination)
