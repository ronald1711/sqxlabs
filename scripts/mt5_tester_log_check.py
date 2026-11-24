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
from html.parser import HTMLParser


def check_report_settings(text, short=False, symbol="SQX_EURUSD"):
    """Check the English HTML report's explicit settings, not its provenance.

    A matching journal alone cannot establish that profit-in-pips was disabled.
    Unknown/localized layouts fail closed; HTML is parsed as data, never rendered.
    """
    class Rows(HTMLParser):
        def __init__(self):
            super().__init__()
            self.rows = []
            self.row = None
            self.cell = None

        def handle_starttag(self, tag, attrs):
            if tag == 'tr':
                self.row = []
            elif tag in ('td', 'th'):
                self.cell = []

        def handle_data(self, data):
            if self.cell is not None:
                self.cell.append(data)

        def handle_endtag(self, tag):
            if tag in ('td', 'th') and self.cell is not None:
                if self.row is not None:
                    self.row.append(' '.join(''.join(self.cell).split()))
                self.cell = None
            elif tag == 'tr' and self.row is not None:
                self.rows.append(self.row)
                self.row = None

    if symbol not in {'SQX_EURUSD', 'SQX_EURUSD_V2'}:
        raise ValueError('Unsupported reference symbol')
    parser = Rows()
    parser.feed(text)
    parser.close()
    expected = {
        'Expert:': 'constant_signal_exit', 'Symbol:': symbol,
        'Period:': 'M1 (2026.01.05 - 2026.01.06)',
        'Currency:': 'USD', 'Initial Deposit:': '10 000.00', 'Leverage:': '1:30',
        'Inputs:': 'ShortSide=' + str(short).lower(),
    }
    for key, value in expected.items():
        matches = [r for r in parser.rows if r and r[0] == key]
        if matches != [[key, value]]:
            raise ValueError('Report setting missing, duplicated or different: ' + key + ' expected ' + value)
    inputs = [r[1] for r in parser.rows if len(r) == 2 and r[0] == '' and '=' in r[1]]
    if inputs != ['SignalBar=1767571200', 'Lots=0.1']:
        raise ValueError('Report input settings differ from baseline')
    return {'report_settings_match': True, 'report_deals_checked': False}


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
    parser.add_argument('--report', type=Path, help='English HTML report from the same run; checks explicit account/input settings')
    parser.add_argument('--symbol', choices=['SQX_EURUSD', 'SQX_EURUSD_V2'], default='SQX_EURUSD')
    args = parser.parse_args()
    data = args.log.read_bytes()
    try:
        text = data.decode('utf-16' if data.startswith((b'\xff\xfe', b'\xfe\xff')) else 'utf-8-sig')
        result = check(text, args.short, args.symbol)
        result['report_settings_match'] = False
        if args.report:
            report = args.report.read_bytes()
            report_text = report.decode('utf-16' if report.startswith((b'\xff\xfe', b'\xfe\xff')) else 'utf-8-sig')
            result.update(check_report_settings(report_text, args.short, args.symbol))
            result['report_sha256'] = hashlib.sha256(report).hexdigest()
    except (ValueError, UnicodeError, OSError) as error:
        result = {'status': 'rejected', 'reason': str(error)}
    result['log_sha256'] = hashlib.sha256(data).hexdigest()
    print(json.dumps(result, indent=2))
    return int(result['status'] == 'rejected')


if __name__ == '__main__':
    raise SystemExit(main())
