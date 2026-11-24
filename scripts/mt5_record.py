#!/usr/bin/env python3
"""Record bounded, closed MT5 windows with restartable append-only progress.

Uses only read APIs on the local bridge. --follow waits for closed windows;
--windows bounds each invocation. Restart with the exact same recording config.
"""
import argparse
import json
import sys
import time
from mt5_capture_journal import Journal


NETWORK_ERRORS=(ConnectionError,TimeoutError,EOFError)

def record_windows(journal,factory,capture_window,windows,follow=False,poll_seconds=5,reconnect_attempts=3,sleep=time.sleep,emit=None):
    emit=emit or (lambda value:print(json.dumps(value),flush=True))
    config=journal.config;client=None;completed=0;retries=0
    expected_account=journal.verify()[1]
    def disconnect():
        nonlocal client
        old,client=client,None
        if old is not None:
            try:old.close()
            except NETWORK_ERRORS:pass
    try:
        while completed<windows:
            cursor,_=journal.verify();end=cursor+config['window_seconds']
            try:
                if client is None:
                    client=factory()
                    if not client.initialize():raise ConnectionError('MT5 initialization failed')
                info=client.account_info()
                if info is None:raise ConnectionError('account context unavailable')
                observed={key:getattr(info,key) for key in ('server','currency','leverage','trade_mode')}
                if observed['server']!=config['server'] or (expected_account is not None and observed!=expected_account):
                    raise ValueError('recording account context changed before reading window')
                if expected_account is None:expected_account=observed
                ticks=[client.symbol_info_tick(s) for s in config['symbols']]
                if any(t is None for t in ticks):raise ConnectionError('latest broker tick unavailable')
                closed=min(int(t.time)//60*60 for t in ticks)
                payload=None
                if end<=closed:
                    payload=capture_window(client,config['symbols'],cursor,end,config['warmup_seconds'])
            except NETWORK_ERRORS as error:
                disconnect()
                if retries>=reconnect_attempts:raise
                retries+=1;delay=min(2**(retries-1),30)
                emit({'status':'reconnecting','attempt':retries,'delay_seconds':delay,'next_broker_start':cursor,'error_type':type(error).__name__})
                sleep(delay);continue
            if payload is None:
                emit({'status':'waiting_for_closed_window','next_broker_start':cursor,'closed_through':closed,'committed_this_invocation':completed})
                if not follow:return completed
                sleep(poll_seconds);continue
            # Storage and identity failures must never be interpreted as network retries.
            if payload["account"]!=expected_account:raise ValueError("recording account context changed during window acquisition")
            identifier=journal.append(payload);completed+=1;retries=0
            emit({'status':'window_committed','capture_sha256':identifier,'broker_window_seconds':[cursor,end],'next_broker_start':end})
        return completed
    finally:disconnect()


def main():
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--symbols',required=True)
    parser.add_argument('--server',required=True)
    parser.add_argument('--broker-start',type=int,required=True)
    parser.add_argument('--window-seconds',type=int,default=60)
    parser.add_argument('--warmup-seconds',type=int,default=60)
    parser.add_argument('--windows',type=int,default=1)
    parser.add_argument('--follow',action='store_true')
    parser.add_argument('--poll-seconds',type=int,default=5)
    parser.add_argument('--out',required=True)
    parser.add_argument('--reconnect-attempts',type=int,default=3)
    args=parser.parse_args()
    if args.windows<=0 or not 1<=args.poll_seconds<=60:parser.error('positive window count and polling interval 1–60 required')
    if not 0<=args.reconnect_attempts<=10:parser.error('reconnect-attempts must be 0–10')
    config={'schema':1,'symbols':[s.strip() for s in args.symbols.split(',')],'server':args.server,'broker_start':args.broker_start,'window_seconds':args.window_seconds,'warmup_seconds':args.warmup_seconds}
    from siliconmetatrader5 import MetaTrader5
    from mt5_capture_window import capture
    with Journal(args.out,config) as journal:
        factory=lambda:MetaTrader5(host='127.0.0.1',port=8001,keepalive=False,timeout=30)
        record_windows(journal,factory,capture,args.windows,args.follow,args.poll_seconds,args.reconnect_attempts)



if __name__=='__main__':
    try:
        main()
    except (ValueError,OSError,EOFError) as error:
        print(json.dumps({'status':'recording_failed','error_type':type(error).__name__,'message':str(error),'scope':'No success is implied for the pending window. Restart verifies committed records before continuing.'}),file=sys.stderr,flush=True)
        raise SystemExit(1)
