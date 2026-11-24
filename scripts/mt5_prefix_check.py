"""Metamorphic future-data test for the bounded captured-data reference suite.

Derived data is synthetic and explicitly linked to its source capture.
"""
import argparse
import copy
import hashlib
import json
from pathlib import Path
import subprocess
import sys
from decimal import Decimal

if not __debug__:
    raise RuntimeError('Prefix checks require assertions; do not run Python with -O')
parser=argparse.ArgumentParser(description=__doc__)
parser.add_argument('--capture',type=Path,required=True)
parser.add_argument('--policy',type=Path,required=True)
parser.add_argument('--executable',type=Path,default=Path('target/debug/sqx-cli'))
parser.add_argument('--out',type=Path,required=True)
args=parser.parse_args()
original=args.capture.resolve();raw=(original/'capture.json').read_bytes()
assert hashlib.sha256(raw).hexdigest()==original.name
payload=json.loads(raw);derived=copy.deepcopy(payload)
market=next(m for m in derived['markets'] if m['symbol']=='USDJPY')
cutoff=market['raw_bars'][-1]['time']*1000
shift=Decimal('0.050')
for bar in market['raw_bars']:
    if bar['time']*1000>=cutoff:
        for field in ('open','high','low','close'):bar[field]=float(Decimal(str(bar[field]))+shift)
for key in ('raw_ticks','raw_tick_response'):
    for tick in market[key]:
        if tick['time_msc']>=cutoff:
            for field in ('bid','ask'):tick[field]=float(Decimal(str(tick[field]))+shift)
def shifted_csv(text, tick):
    lines=text.splitlines();out=[lines[0]]
    for line in lines[1:]:
        fields=line.split(',')
        if int(fields[0])*(1 if tick else 1000)>=cutoff:
            for i in (range(1,3) if tick else range(1,5)):
                fields[i]=str(Decimal(fields[i])+shift)
        out.append(','.join(fields))
    return '\n'.join(out)+'\n'
market['bars_csv']=shifted_csv(market['bars_csv'],False)
market['ticks_csv']=shifted_csv(market['ticks_csv'],True)
derived['synthetic_derivation']={'source_capture':original.name,'broker_cutoff_ms':cutoff,'price_shift':str(shift),'scope':'Synthetic future mutation, not broker observations'}
blob=json.dumps(derived,sort_keys=True,separators=(',',':'),allow_nan=False).encode()
target=args.out.resolve()/hashlib.sha256(blob).hexdigest();target.mkdir(parents=True,exist_ok=True)
if (target/'capture.json').exists():assert (target/'capture.json').read_bytes()==blob
else:(target/'capture.json').write_bytes(blob)
checker=Path(__file__).with_name('mt5_reference_check.py').resolve()
def verify(path):
    run=subprocess.run([sys.executable,str(checker),'--capture',str(path),'--policy',str(args.policy.resolve()),'--executable',str(args.executable.resolve())],capture_output=True,text=True)
    assert run.returncode==0,run.stderr
    return json.loads((path/'reference/comparison.json').read_text())
# Clear prior success before starting either expensive verification.
report_path=args.out.resolve()/'prefix-comparison.json'
report_path.write_text(json.dumps({'status':'verification_not_completed'}))
base=verify(original);modified=verify(target)
policy=json.loads(args.policy.read_text())
interval=next(r for r in policy['intervals'] if r['raw_start_ms']<=cutoff<r['raw_end_ms'])
utc_cutoff=cutoff-interval['offset_seconds']*1000
results=[];changed=0
for a,b in zip(base['cases'],modified['cases'],strict=True):
    assert (a['rule'],a['side'])==(b['rule'],b['side'])
    def result(path,case):return json.loads((path/'reference/archive/runs'/f"{case['run_id']}.json").read_text())['result']
    before=result(original,a);after=result(target,b)
    for field,clock in [('entry_attempts','attempt_time'),('cash_settlements','time')]:
        x=[r for r in before['execution'][field] if r[clock]<utc_cutoff]
        y=[r for r in after['execution'][field] if r[clock]<utc_cutoff]
        assert x==y,(a['rule'],a['side'],field)
    def snapshots(run):
        return [r for r in run['execution']['account_snapshots'] if r.get('quote_time') is not None and r['quote_time']<utc_cutoff]
    assert snapshots(before)==snapshots(after)
    assert snapshots(before), 'prefix comparison must have account evidence'
    changed+=before['final_equity']!=after['final_equity']
    results.append({'rule':a['rule'],'side':a['side'],'prefix_matches':True,'base_run':a['run_id'],'derived_run':b['run_id']})
assert changed>0,'future mutation must affect at least one terminal result'
report={'status':'passed','source_capture':original.name,'synthetic_capture':target.name,'cutoff_utc_ms':utc_cutoff,'changed_terminal_results':changed,'cases':results,'scope':'One future-price mutation across the explicit reference cases; not a general no-look-ahead proof.'}
report_path.write_text(json.dumps(report,indent=2));print(json.dumps(report,indent=2))
