#!/usr/bin/env python3
"""Offline broker-clock conversion with immutable raw/policy provenance.

Does not connect to MT5, infer offsets, or certify price/tick completeness.
"""
import argparse
import csv
import hashlib
import io
import json
import os
from pathlib import Path
import shutil
import tempfile


def encoded(value):
    return json.dumps(value, sort_keys=True, separators=(',', ':'), allow_nan=False).encode()


def digest(value):
    return hashlib.sha256(value).hexdigest()


def integer(value):
    if type(value) is not int:
        raise ValueError('policy times and offsets must be integers')
    return value


def convert(raw, policy, server, duration=None, legacy_broker_clock=False):
    if not isinstance(policy, dict) or type(policy.get('schema')) is not int or policy.get('schema') != 1 or policy.get('server') != server:
        raise ValueError('unsupported policy schema or server mismatch')
    intervals = policy.get('intervals')
    if not isinstance(intervals, list) or not intervals:
        raise ValueError('nonempty policy intervals required')
    previous_end, previous_utc_end = None, None
    for row in intervals:
        start, end, offset = (integer(row[k]) for k in ['raw_start_ms', 'raw_end_ms', 'offset_seconds'])
        if start >= end or abs(offset) > 14*3600 or (previous_end is not None and start < previous_end):
            raise ValueError('invalid, unordered or overlapping clock intervals')
        if not isinstance(row.get('source'), str) or not row['source'].strip():
            raise ValueError('each clock interval requires a source')
        utc_start, utc_end = start-offset*1000, end-offset*1000
        if not -(2**63) <= start < end < 2**63 or not -(2**63) <= utc_start < utc_end < 2**63:
            raise ValueError('clock interval out of timestamp range')
        if previous_utc_end is not None and utc_start < previous_utc_end:
            raise ValueError('normalized clock intervals overlap')
        previous_end, previous_utc_end = end, utc_end

    def resolve(stamp):
        matches = [r for r in intervals if r['raw_start_ms'] <= stamp < r['raw_end_ms']]
        if len(matches) != 1:
            raise ValueError(f'raw timestamp outside supported clock intervals: {stamp}')
        return matches[0]

    reader = csv.reader(io.StringIO(raw.decode('utf-8-sig')))
    header = next(reader, None)
    if header == ['timestamp_broker', 'open', 'high', 'low', 'close', 'volume'] or (legacy_broker_clock and header == ['timestamp', 'open', 'high', 'low', 'close', 'volume']):
        header[0] = 'timestamp'
        scale = 1000
        if type(duration) is not int or duration <= 0:
            raise ValueError('positive bar duration in milliseconds required for candles')
    elif header == ['timestamp_broker_ms', 'bid', 'ask'] or (legacy_broker_clock and header == ['timestamp_ms', 'bid', 'ask']):
        header[0] = 'timestamp_ms'
        scale = 1
        if duration is not None:
            raise ValueError('bar duration must not be supplied for ticks')
    else:
        raise ValueError('explicit broker-clock CSV header required; legacy input requires a deliberate legacy broker-clock declaration')
    output = io.StringIO(newline='')
    writer = csv.writer(output, lineterminator='\n')
    writer.writerow(header)
    count, previous_raw, previous_utc = 0, None, None
    for row in reader:
        if len(row) != len(header):
            raise ValueError('malformed CSV row')
        raw_time = int(row[0]) * scale
        if not -(2**63) <= raw_time < 2**63:
            raise ValueError("raw timestamp out of range")
        interval = resolve(raw_time)
        if duration is not None and raw_time + duration > interval['raw_end_ms']:
            raise ValueError('candle crosses an unresolved clock-policy boundary')
        utc = raw_time - interval['offset_seconds'] * 1000
        if not -(2**63) <= utc < 2**63 or utc % scale:
            raise ValueError('normalized timestamp out of range or precision')
        if previous_raw is not None and (raw_time < previous_raw or utc < previous_utc or (scale == 1000 and (raw_time == previous_raw or utc == previous_utc))):
            raise ValueError('non-increasing candles or reversed tick chronology')
        writer.writerow([str(utc // scale), *row[1:]])
        count += 1
        previous_raw, previous_utc = raw_time, utc
    if not count:
        raise ValueError('empty data')
    return output.getvalue().encode(), count


def publish(source, policy_path, root, server, duration=None, legacy_broker_clock=False):
    raw, policy_bytes = Path(source).read_bytes(), Path(policy_path).read_bytes()
    policy = json.loads(policy_bytes)
    normalized, count = convert(raw, policy, server, duration, legacy_broker_clock)
    manifest = {'schema': 1, 'server': server, 'raw_sha256': digest(raw),
                'policy_sha256': digest(policy_bytes), 'normalized_sha256': digest(normalized),
                'rows': count, 'bar_duration_ms': duration, 'legacy_broker_clock': legacy_broker_clock,
                'clock': 'UTC normalized using explicit broker-clock intervals',
                'scope': 'Timestamp conversion only; declared server identity is not authenticated, prices and completeness are not certified.'}
    files = {'raw.csv': raw, 'utc.csv': normalized, 'policy.json': policy_bytes, 'manifest.json': encoded(manifest)}
    identity = digest(files['manifest.json'])
    root = Path(root)
    root.mkdir(parents=True, exist_ok=True)
    target = root / identity
    if target.exists():
        if any((target/name).read_bytes() != data for name, data in files.items()):
            raise ValueError('existing normalization bundle failed integrity check')
        return target
    temporary = Path(tempfile.mkdtemp(prefix='.pending-', dir=root))
    try:
        for name, data in files.items():
            with (temporary/name).open('wb') as output:
                output.write(data)
                output.flush()
                os.fsync(output.fileno())
        # A published nonempty bundle cannot be replaced by another rename.
        try:
            temporary.rename(target)
        except OSError:
            if not target.exists() or any((target/name).read_bytes() != data for name, data in files.items()):
                raise
        return target
    finally:
        if temporary.exists():
            shutil.rmtree(temporary)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--input', required=True)
    parser.add_argument('--policy', required=True)
    parser.add_argument('--server', required=True)
    parser.add_argument('--out', required=True)
    parser.add_argument('--bar-duration-ms', type=int)
    parser.add_argument('--legacy-broker-clock', action='store_true', help='Explicitly declare an older timestamp/timestamp_ms CSV as raw broker time, never already normalized UTC')
    args = parser.parse_args()
    path = publish(args.input, args.policy, args.out, args.server, args.bar_duration_ms, args.legacy_broker_clock)
    print(json.dumps({'bundle': str(path), 'utc_csv': str(path/'utc.csv')}))


if __name__ == '__main__':
    main()
