#!/usr/bin/env python3
"""Offline export of a bounded, contiguous committed recording range.

Original captures are embedded as provenance. Only atomic committed records are
read, allowing an active writer to continue. No broker connection is made.
"""
import argparse
import copy
import hashlib
import json
from pathlib import Path
from mt5_capture_journal import Journal,encode,publish


def export(root,start,end,out):
    root=Path(root)
    config=json.loads((root/'config.json').read_bytes())
    journal=Journal(root,config)
    windows=journal.snapshot(start,end)
    first=windows[0]['capture'];last=windows[-1]['capture']
    result=copy.deepcopy(first)
    result['broker_window_seconds']=[start,end]
    result['host_capture_utc_ms']=[first['host_capture_utc_ms'][0],last['host_capture_utc_ms'][1]]
    for index,market in enumerate(result['markets']):
        originals=[w['capture']['markets'][index] for w in windows]
        market.pop('spec_observation',None)
        market['spec_observations']=[original.get('spec_observation') for original in originals]
        for key in ('raw_bars','raw_ticks'):
            market[key]=[row for original in originals for row in original[key]]
        # Only the first window's warmup belongs before the selected range.
        # Subsequent warmups overlap existing main ticks and are not appended.
        market['raw_tick_response']=market['raw_warmup_ticks']+market['raw_ticks']
        for key in ('bars_csv','ticks_csv'):
            header=originals[0][key].splitlines()[0]
            if any(original[key].splitlines()[0]!=header for original in originals):
                raise ValueError('inconsistent CSV schema in recording')
            market[key]=header+'\n'+''.join(original[key].split('\n',1)[1] for original in originals)
        market['request_host_utc_ms']=[originals[0]['request_host_utc_ms'][0],originals[-1]['request_host_utc_ms'][1]]
    result['recording_provenance']={'configuration':config,'windows':windows,
        'scope':'Derived concatenation of committed windows; full original API responses remain embedded here. The top-level tick response is reconstructed, not one broker response.'}
    result['scope']='Offline recording range export; sequential captures, not atomic or certified complete broker history.'
    data=encode(result);identifier=hashlib.sha256(data).hexdigest()
    destination=Path(out)/identifier
    publish(destination/'capture.json',data)
    return destination


def main():
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--journal',type=Path,required=True)
    parser.add_argument('--broker-start',type=int,required=True)
    parser.add_argument('--broker-end',type=int,required=True)
    parser.add_argument('--out',type=Path,required=True)
    args=parser.parse_args()
    print(json.dumps({'capture':str(export(args.journal,args.broker_start,args.broker_end,args.out))}))


if __name__=='__main__':main()
