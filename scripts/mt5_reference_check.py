"""Independent Decimal arithmetic against frozen constant-entry and SMA(2) tick backtests; not broker fills."""
import argparse,json,sys,subprocess,hashlib,csv,io
from decimal import Decimal as D
from pathlib import Path
if not __debug__:
 raise RuntimeError('Reference checks require assertions; do not run Python with -O')
sys.path.insert(0,'scripts')
from mt5_normalize import publish
def sma2_signal_index(closes,long):
 # The last candle cannot produce a next-open fill inside this bounded capture.
 return next((i for i in range(1,len(closes)-1) if (closes[i]>(closes[i]+closes[i-1])/2 if long else closes[i]<(closes[i]+closes[i-1])/2)),None)

def coverage_status(reports):
 expected={(mode,side) for mode in ('constant','sma2','stop','signal_exit') for side in ('long','short')}
 seen={(row['rule'],row['side']) for row in reports}
 return 'passed' if len(reports)==8 and seen==expected and all(row.get('status')=='passed' and row.get('matches') is True for row in reports) else 'coverage_incomplete'

def main():
 parser=argparse.ArgumentParser(description=__doc__)
 parser.add_argument('--capture',type=Path,required=True)
 parser.add_argument('--policy',type=Path,required=True)
 parser.add_argument('--executable',type=Path,default=Path('target/debug/sqx-cli'))
 args=parser.parse_args()
 root=args.capture.resolve()
 data=(root/'capture.json').read_bytes()
 assert hashlib.sha256(data).hexdigest()==root.name
 capture=json.loads(data)
 assert capture['account']['currency']=='EUR' and capture['account']['leverage']==30, 'reference requires EUR account with leverage 30'
 markets={m['symbol']:m for m in capture['markets']}
 spec=markets['USDJPY']['spec']
 assert spec['trade_contract_size']==100000 and spec['trade_tick_size']==0.001 and spec['point']==0.001, 'unsupported USDJPY specification'
 policy=args.policy.resolve();server=capture['account']['server'];out=root/'reference';out.mkdir(exist_ok=True)
 # Replace any previous green summary before executing a new verification attempt.
 # A failed/interrupted attempt therefore cannot leave a stale success summary.
 checker_id=hashlib.sha256(Path(__file__).read_bytes()).hexdigest()
 (out/'comparison.json').write_text(json.dumps({'status':'verification_not_completed','capture':root.name,'checker_sha256':checker_id}))
 bundles={}
 for sym,m in markets.items():
  for kind,duration in [('bars',60000),('ticks',None),('conversion',None)]:
   text=m[kind+'_csv'] if kind!='conversion' else m['warmup_ticks_csv']+m['ticks_csv'].split('\n',1)[1]
   source=out/f'{sym}-{kind}.csv';source.write_text(text)
   bundles[sym,kind]=publish(source,policy,out/'normalized',server,duration)
 def rows(sym,kind):return list(csv.DictReader(io.StringIO((bundles[sym,kind]/'utc.csv').read_text())))
 def rates(sym,base):
  # Explicit last-in-response convention for multiple conversion ticks at one ms.
  unique={int(t['timestamp_ms']):t for t in rows(sym,'conversion')}
  return [{'time':time,'from':base,'to':'EUR','long_rate':float(D(1)/D(t['bid'])),'short_rate':float(D(1)/D(t['ask']))} for time,t in unique.items()]
 account={'currency':'EUR','max_leverage':30,'margin_model':'forex_base','profit_conversion_model':'directional','max_conversion_age_ms':10000,'margin_conversion_rates':rates('EURUSD','USD'),'profit_conversion_rates':rates('EURJPY','JPY')}
 (out/'account.json').write_text(json.dumps(account))
 exe=args.executable.resolve();reports=[]
 def checkpoint_report(pending=None):
  (out/'comparison.json').write_text(json.dumps({'status':'verification_not_completed','capture':root.name,'checker_sha256':checker_id,'cases':reports,'pending_case':pending},indent=2))
 def verify_replay(run_id):
  replay=subprocess.run([str(exe),'replay-backtest','--archive-dir',str(archive),'--run-id',run_id],text=True,capture_output=True)
  assert replay.returncode==0,replay.stderr
 for mode,long in [(mode,long) for mode in ['constant','sma2','stop','signal_exit'] for long in [True,False]]:
  side=('long' if long else 'short');case=mode+'-'+side;sign=D(1 if long else -1)
  checkpoint_report(case)
  strategy={'name':'aligned-'+case,'long':long,'short':not long,'signals':[{'kind':'LongEntry' if long else 'ShortEntry','conditions':[{'left':{'Const':1},'cmp':'Gt','right':{'Const':0}}],'join':'And','fuzzy':None}],'exits':[],'stop_loss':None,'profit_target':None,'money_management':{'FixedSize':{'size':0.1}}}
  if mode=='signal_exit':
   strategy['signals'].append({'kind':'LongExit' if long else 'ShortExit','conditions':[{'left':{'Const':1},'cmp':'Gt','right':{'Const':0}}],'join':'And','fuzzy':None})
  if mode in ('stop','signal_exit'):
   strategy['trading_options']={'max_trades_per_day':1,'exit_at_end_of_day':False,'exit_on_friday':False,'limit_hours':None}
  if mode=='stop':
   strategy['stop_loss']={'Points':0.4}
   strategy['trading_options']={'max_trades_per_day':1,'exit_at_end_of_day':False,'exit_on_friday':False,'limit_hours':None}
  if mode=='sma2':
   strategy['signals'][0]['conditions']=[{'left':{'Price':['Close',0]},'cmp':'Gt' if long else 'Lt','right':{'Indicator':['SMA',[{'name':'period','value':2.0}]]}}]
  sp=out/f'{case}.json' ;sp.write_text(json.dumps(strategy));archive=out/'archive'
  args=[str(exe),'backtest','--strategy',str(sp),'--data',str(bundles['USDJPY','bars']/'utc.csv'),'--normalization-bundle',str(bundles['USDJPY','bars']),'--ticks',str(bundles['USDJPY','ticks']/'utc.csv'),'--tick-normalization-bundle',str(bundles['USDJPY','ticks']),'--timeframe','M1','--symbol','USDJPY','--account-config',str(out/'account.json'),'--commission','3.5','--slippage','0.1','--archive-dir',str(archive)]
  result=subprocess.run(args,text=True,capture_output=True);assert result.returncode==0,result.stderr
  receipt=json.loads(next(l.removeprefix('Backtest archive: ') for l in result.stdout.splitlines() if l.startswith('Backtest archive: ')))
  run=json.loads((archive/'runs'/f"{receipt['run_id']}.json").read_text());actual=run['result'];assert not actual['errors']
  bars=rows('USDJPY','bars');ticks=rows('USDJPY','ticks')
  closes=[D(b['close']) for b in bars]
  signal_index=0 if mode!='sma2' else sma2_signal_index(closes,long)
  if signal_index is None:
   assert not actual['trades'], (case,'unexpected trade without a qualifying signal')
   assert not actual['execution']['entry_attempts'], (case,'unexpected entry attempt')
   assert D(str(actual['initial_deposit']))==D(10000) and D(str(actual['final_equity']))==D(10000)
   verify_replay(receipt['run_id'])
   reports.append({'rule':mode,'side':side,'run_id':receipt['run_id'],'status':'not_exercised','reason':'No qualifying pre-final-bar SMA(2) signal in the complete fixed capture.','no_trade_matches':True,'matches':None})
   checkpoint_report();continue
  assert len(actual['trades'])==1,(case,'expected exactly one exercised trade',len(actual['trades']))
  next_open=int(bars[signal_index+1]['timestamp'])*1000
  attempt=actual['execution']['entry_attempts'][0]
  assert attempt['signal_bar_index']==signal_index
  if mode=='sma2':
   observed=attempt['signal']['conditions'][0]
   assert abs(D(str(observed['right']))-(closes[signal_index]+closes[signal_index-1])/2)<D('0.00000001')
   assert abs(D(str(observed['left']))-closes[signal_index])<D('0.00000001')
  entry=next(t for t in ticks if int(t['timestamp_ms'])>=next_open);exit=ticks[-1]
  opening=D(entry['ask' if long else 'bid']).quantize(D('0.001'))+sign*D('0.001')
  stop_level=None
  if mode=='stop':
   stop_level=opening-sign*D('0.004')
   exit=next(t for t in ticks if int(t['timestamp_ms'])>=int(entry['timestamp_ms']) and (D(t['bid']).quantize(D('0.001'))<=stop_level if long else D(t['ask']).quantize(D('0.001'))>=stop_level))
  if mode=='signal_exit':
   exit=next(t for t in ticks if int(t['timestamp_ms'])>=int(bars[2]['timestamp'])*1000)
  closing=D(exit['bid' if long else 'ask']).quantize(D('0.001'))-sign*D('0.001')
  quote=next(t for t in reversed(rows('EURJPY','conversion')) if int(t['timestamp_ms'])<=int(exit['timestamp_ms']))
  rate=D(1)/D(quote['bid' if long else 'ask']);gross=(closing-opening)*sign*D(100000)*D('0.1');commission=D('0.35');pnl=gross*rate-commission
  margin_quote=next(t for t in reversed(rows('EURUSD','conversion')) if int(t['timestamp_ms'])<=int(entry['timestamp_ms']))
  margin=D(100000)*D('0.1')/D(30)/D(margin_quote['bid' if long else 'ask'])
  admission=next(a['margin_check'] for a in actual['execution']['entry_attempts'] if a.get('margin_check'))
  assert abs(D(str(admission['required_margin']))-margin)<D('0.00000001')
  trade=actual['trades'][0]
  assert trade['exit_reason']==('StopLoss' if mode=='stop' else 'ExitSignal' if mode=='signal_exit' else 'EndOfData')
  expected={'open_price':opening,'close_price':closing,'pnl':pnl,'commission':commission}
  for key,value in expected.items():assert abs(D(str(trade[key]))-value)<D('0.00000001'),(side,key,trade[key],value)
  assert trade['open_time']==int(entry['timestamp_ms']);assert trade['close_time']==int(exit['timestamp_ms'])
  assert abs(D(str(actual['final_equity']))-(D(10000)+pnl))<D('0.00000001')
  verify_replay(receipt['run_id'])
  reports.append({'status':'passed','rule':mode,'signal_bar_index':signal_index,'side':side,'run_id':receipt['run_id'],'expected':{k:str(v) for k,v in expected.items()},'stop_level':str(stop_level) if stop_level is not None else None,'exit_reason':trade['exit_reason'],'expected_margin_eur':str(margin),'equity':actual['final_equity'],'matches':True})
  checkpoint_report()
 report={'status':coverage_status(reports),'execution_cases_verified':sum(row.get('status')=='passed' for row in reports),'requested_execution_cases':8,'checker_sha256':checker_id,'engine_sha256':hashlib.sha256(exe.read_bytes()).hexdigest(),'capture':root.name,'cases':reports,'assumptions':{'lots':0.1,'deposit_eur':10000,'commission_eur_per_lot_roundtrip':3.5,'slippage_broker_points_each_fill':1,'slippage_sqxlabs_points_each_fill':0.1,'broker_point':0.001,'sqxlabs_point':0.01,'max_conversion_age_ms':10000,'equal_time_conversion':'last in response at or before event','exit':'first adverse stop tick for stop cases; first tick after exit-signal candle for signal_exit; terminal liquidation otherwise'},'scope':'Independent Decimal arithmetic on captured quotes under declared model assumptions, not independently observed broker fills or broker fee certification.'}
 (out/'comparison.json').write_text(json.dumps(report,indent=2));print(json.dumps(report,indent=2))

 if report["status"]!="passed":raise SystemExit(1)

if __name__=="__main__":main()
