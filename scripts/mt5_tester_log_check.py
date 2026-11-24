#!/usr/bin/env python3
"""Check one constant_signal_exit MT5 log against the fixed synthetic baseline.

This verifies reported values only, not tick provenance or SqxLabs parity.
Supply a log containing exactly one run, not an accumulated terminal journal.
"""
import argparse
from datetime import datetime, timezone
from decimal import Decimal, InvalidOperation
import hashlib
import json
from pathlib import Path
import re


def check(text, short=False, symbol="SQX_EURUSD"):
    if symbol not in {"SQX_EURUSD", "SQX_EURUSD_V2"}:
        raise ValueError("Unsupported reference symbol")
    rows = {}
    sequence = []
    for line in text.splitlines():
        match = re.search(r'SQX_REFERENCE_([A-Z]+)\s+(.*)', line)
        if not match:
            continue
        kind, body = match.groups()
        sequence.append(kind)
        values = {}
        for token in body.split():
            if token.count('=') != 1:
                raise ValueError('Malformed reference record')
            key, value = token.split('=')
            if key in values:
                raise ValueError('Duplicate field')
            values[key] = value
        rows.setdefault(kind, []).append(values)
    if sequence != ['INIT', 'ACCOUNT', 'SYMBOL', 'ORDER', 'ORDER',
                    'DEAL', 'DEAL', 'DEAL', 'RECONCILIATION', 'COMPLETE']:
        raise ValueError('Incomplete, reordered or mixed reference run')
    counts = {'INIT': 1, 'ACCOUNT': 1, 'SYMBOL': 1, 'ORDER': 2,
              'RECONCILIATION': 1, 'COMPLETE': 1}
    if set(rows) != set(counts) | {'DEAL'}:
        raise ValueError('Missing or unknown reference record type')
    for kind, count in counts.items():
        if len(rows[kind]) != count:
            raise ValueError('Missing, duplicate or mixed runs: ' + kind)

    def require(row, **expected):
        for key, value in expected.items():
            actual = row.get(key)
            if isinstance(value, str):
                equal = actual == value
            else:
                try:
                    number = Decimal(actual)
                    equal = number.is_finite() and number == Decimal(str(value))
                except (InvalidOperation, TypeError, ValueError):
                    equal = False
            if not equal:
                raise ValueError(f'{key}: expected {value}, got {actual}')

    start = int(datetime(2026, 1, 5, tzinfo=timezone.utc).timestamp())
    require(rows['INIT'][0], symbol=symbol, short=int(short), signal_bar=start, lots=0.1)
    require(rows['ACCOUNT'][0], currency='USD', balance=10000, leverage=30)
    require(rows['SYMBOL'][0], base='EUR', profit='USD', margin='EUR', contract=100000,
            point=0.00001, tick_size=0.00001, calculation=0)
    prices = [Decimal('1.10010'), Decimal('1.10040')] if short else [Decimal('1.10030'), Decimal('1.10020')]
    net = Decimal('-3') if short else Decimal('-1')
    deals = rows['DEAL']
    if len(deals) != 3:
        raise ValueError('Expected initial deposit and exactly two market deals')
    require(deals[0], type=2, profit=10000, commission=0, swap=0, fee=0)
    position = deals[1].get('position')
    if not position or not position.isdigit() or int(position) <= 0:
        raise ValueError('Invalid position identifier')
    tickets = [d.get('ticket', '') for d in deals]
    if any(not t.isdigit() or int(t) <= 0 for t in tickets) or len(set(tickets)) != 3:
        raise ValueError('Invalid or duplicate deal tickets')
    for index in range(2):
        timestamp = (start + (index + 1) * 60) * 1000
        require(rows['ORDER'][index], closing=index, time_msc=timestamp,
                bid=Decimal('1.10010') + Decimal(index) / 10000,
                ask=Decimal('1.10030') + Decimal(index) / 10000,
                retcode=10009, deal=tickets[index + 1], price=prices[index], volume=0.1)
        require(deals[index + 1], time_msc=timestamp, type=int(short) ^ index,
                entry=index, position=position, price=prices[index], volume=0.1,
                profit=net if index else 0, commission=0, swap=0, fee=0)
    require(rows['RECONCILIATION'][0], market_deals=2, net=net, residual=0)
    require(rows['COMPLETE'][0], ok=1, balance=10000 + net, equity=10000 + net)
    return {'status': 'reported_baseline_values_match', 'side': 'short' if short else 'long',
            'tick_provenance_verified': False, 'sqxlabs_parity_verified': False}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('log', type=Path)
    parser.add_argument('--short', action='store_true')
    parser.add_argument('--symbol', choices=['SQX_EURUSD', 'SQX_EURUSD_V2'], default='SQX_EURUSD')
    args = parser.parse_args()
    data = args.log.read_bytes()
    try:
        text = data.decode('utf-16' if data.startswith((b'\xff\xfe', b'\xfe\xff')) else 'utf-8-sig')
        result = check(text, args.short, args.symbol)
    except (ValueError, UnicodeError) as error:
        result = {'status': 'rejected', 'reason': str(error)}
    result['log_sha256'] = hashlib.sha256(data).hexdigest()
    print(json.dumps(result, indent=2))
    return int(result['status'] == 'rejected')


if __name__ == '__main__':
    raise SystemExit(main())
