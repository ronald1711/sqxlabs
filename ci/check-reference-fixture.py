#!/usr/bin/env python3
"""Run independent arithmetic and future-mutation checks without broker access."""
import argparse
import hashlib
import json
from pathlib import Path
import subprocess
import sys
import tempfile

parser=argparse.ArgumentParser(description=__doc__)
parser.add_argument('--executable',type=Path,required=True)
args=parser.parse_args()
repo=Path(__file__).resolve().parents[1]
start=1700000040
markets=[]
for symbol in ['EURUSD','USDJPY','EURJPY']:
    bars=[];ticks=[];warmup=[]
    for minute in range(-1,5):
        values=[]
        for offset in range(0,60,5):
            if symbol=='USDJPY':
                opening=[150.0,150.0,150.010,150.005,150.020][max(minute,0)]
                closing=[150.0,150.010,150.005,150.020,150.030][max(minute,0)]
                bid=opening if offset==0 else closing
                if minute==1 and offset==5:bid=150.004  # short stop gaps
                if minute==1 and offset==10:bid=149.998 # long stop gaps
                bid=round(bid,3);ask=round(bid+0.002,3)
            else:
                bid=1.1 if symbol=='EURUSD' else 165.0
                ask=1.10002 if symbol=='EURUSD' else 165.002
            time=start+minute*60+offset
            tick={'time':time,'time_msc':time*1000,'bid':bid,'ask':ask}
            (warmup if minute<0 else ticks).append(tick);values.append(bid)
        if minute>=0:
            bars.append({'time':start+minute*60,'open':values[0],'high':max(values),'low':min(values),'close':values[-1],'tick_volume':len(values)})
    def tick_csv(data):return 'timestamp_broker_ms,bid,ask\n'+''.join(f"{t['time_msc']},{t['bid']},{t['ask']}\n" for t in data)
    bar_csv='timestamp_broker,open,high,low,close,volume\n'+''.join(','.join(str(b[k]) for k in ['time','open','high','low','close','tick_volume'])+'\n' for b in bars)
    markets.append({'symbol':symbol,'spec':{'point':0.001,'trade_tick_size':0.001,'trade_contract_size':100000},'raw_bars':bars,'raw_ticks':ticks,'raw_warmup_ticks':warmup,'raw_tick_response':warmup+ticks,'bars_csv':bar_csv,'ticks_csv':tick_csv(ticks),'warmup_ticks_csv':tick_csv(warmup)})
payload={'schema':1,'account':{'server':'synthetic-regression','currency':'EUR','leverage':30,'trade_mode':0},'broker_window_seconds':[start,start+300],'tick_warmup_seconds':60,'markets':markets,'scope':'Synthetic arithmetic regression fixture; never broker evidence'}
blob=json.dumps(payload,sort_keys=True,separators=(',',':'),allow_nan=False).encode()
with tempfile.TemporaryDirectory(prefix='sqx-reference-') as directory:
    root=Path(directory);capture=root/hashlib.sha256(blob).hexdigest();capture.mkdir();(capture/'capture.json').write_bytes(blob)
    policy=root/'policy.json';policy.write_text(json.dumps({'schema':1,'server':'synthetic-regression','intervals':[{'raw_start_ms':(start-120)*1000,'raw_end_ms':(start+360)*1000,'offset_seconds':0,'source':'Synthetic UTC fixture'}]}))
    command=[sys.executable,str(repo/'scripts/mt5_prefix_check.py'),'--capture',str(capture),'--policy',str(policy),'--executable',str(args.executable.resolve()),'--out',str(root/'derived')]
    run=subprocess.run(command,cwd=repo,text=True,capture_output=True)
    if run.returncode:
        print(run.stdout);print(run.stderr,file=sys.stderr);raise SystemExit(run.returncode)
    result=json.loads(run.stdout)
    if result['status']!='passed' or len(result['cases'])!=8 or result['changed_terminal_results']==0:
        raise RuntimeError('incomplete independent reference result')
    print('Independent reference regression passed: eight long/short cases, archive replay, and future-price invariance; synthetic data only.')
