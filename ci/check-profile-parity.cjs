// Compare actual CLI research traces, including every candidate and metric.
// Executable identity must differ across builds; no result field is normalized.
const fs=require('node:fs'),path=require('node:path'),cp=require('node:child_process'),assert=require('node:assert/strict');
const binaries=process.argv.slice(2).map(p=>path.resolve(p));
assert.equal(binaries.length,2,'Provide debug and release sqx-cli paths');
fs.mkdirSync('work',{recursive:true});const root=fs.mkdtempSync(path.resolve('work/profile-parity-'));
try {
 const csv=['timestamp,open,high,low,close,volume'];
 const tickRows=['timestamp_ms,bid,ask'];
 for(let i=0;i<1500;i++){const p=1.1+Math.sin(i*0.3)*0.01;csv.push([1700000000+i*60,p,p+0.003,p-0.003,p+0.0002,10].join(','));
  for(const [offset,bid] of [[0,p],[15,p+0.003],[30,p-0.003],[59,p+0.0002]])tickRows.push([(1700000000+i*60+offset)*1000+123,bid,bid+0.0002].join(','));
 }
 const data=path.join(root,'bars.csv');fs.writeFileSync(data,csv.join('\n')+'\n');
 const ticks=path.join(root,'ticks.csv');fs.writeFileSync(ticks,tickRows.join('\n')+'\n');
 // Compare full execution evidence separately from candidate summaries. Source
 // audit wall-clock timestamps are outside this comparison; no execution field
 // is normalized or rounded.
 for(const tickMode of [false,true]) for(const short of [false,true]) {
  const strategy=JSON.parse(fs.readFileSync('examples/rsi-mean-reversion.json','utf8'));
  strategy.name=short?'Parity short':'Parity long';strategy.long=!short;strategy.short=short;
  strategy.signals=[{kind:short?'ShortEntry':'LongEntry',conditions:[{left:{Const:1},cmp:'Gt',right:{Const:0}}],join:'And',fuzzy:null}];
  strategy.exits=['TrailingStop','MoveToBreakEven'];strategy.stop_loss={Points:20};strategy.profit_target={Points:40};
  const strategyPath=path.join(root,short?'short.json':'long.json');fs.writeFileSync(strategyPath,JSON.stringify(strategy));
  const evidence=binaries.map(binary=>{
   const run=cp.spawnSync(binary,['backtest','--strategy',strategyPath,'--data',data,'--symbol','EURUSD','--timeframe','M1','--spread','2','--slippage','1','--commission','5',...(tickMode?['--ticks',ticks]:[])],{cwd:root,encoding:'utf8',timeout:120000,maxBuffer:32*1024*1024});
   assert.equal(run.status,0,run.error?.message||run.stderr);
   const line=run.stdout.split('\n').find(line=>line.startsWith('Research context: '));assert(line,'Missing research evidence');
   const execution=JSON.parse(line.slice('Research context: '.length)).execution;
   assert.equal(execution.account_evidence_schema,1);
   assert.equal(execution.mode,tickMode?'supplied_ticks':'ohlc_bars');
   if(tickMode){assert.equal(execution.supplied_ticks,6000);assert.deepEqual(execution.missing_tick_intervals,[]);assert(execution.account_snapshots.some(row=>row.quote_time!==null&&row.quote_time%1000===123));assert(execution.entry_attempts.some(row=>row.trade_id!==null&&row.attempt_time%1000===123));}
   assert(execution.cash_settlements.length>0,'Execution parity must include closed trades');
   assert(execution.entry_attempts.length>0&&execution.account_snapshots.length>0);
   return JSON.stringify(execution);
  });
  assert.equal(evidence[0],evidence[1],`Execution evidence differs for ${tickMode?'ticks':'OHLC'} ${short?'short':'long'}`);
  console.log(`Execution parity passed: ${tickMode?'ticks':'OHLC'} ${short?'short':'long'}, recorded account/order evidence byte-identical.`);
 }
 for(const seed of [42,731]) {
  const logs=binaries.map((binary,index)=>{
   const log=path.join(root,`${seed}-${index}.jsonl`);
   const run=cp.spawnSync(binary,['evolve','--symbol','EURUSD','--timeframe','M1','--data',data,'--seed',String(seed),'--population','20','--generations','2','--min-trades','1','--audit-log',log],{encoding:'utf8',timeout:120000,maxBuffer:16*1024*1024});
   assert.equal(run.status,0,run.error?.message||run.stderr);
   return fs.readFileSync(log,'utf8').trimEnd().split('\n');
  });
  const headers=logs.map(lines=>JSON.parse(lines[0]));
  assert.notEqual(headers[0].engine_id,headers[1].engine_id,'Comparison requires distinct executable builds');
  for(const header of headers){assert.equal(header.kind,'started');assert.match(header.engine_id,/^[a-f0-9]{64}$/);delete header.engine_id;}
  assert.deepEqual(headers[0],headers[1],`Frozen input mismatch, seed ${seed}`);
  assert.deepEqual(logs[0].slice(1),logs[1].slice(1),`Byte-different candidate evidence or ranking, seed ${seed}`);
  const terminal=JSON.parse(logs[0].at(-1));assert.equal(terminal.kind,'completed');assert.equal(terminal.attempted,60);assert(terminal.ranked.length>0);
  console.log(`Profile parity passed: seed ${seed}, 60 evaluations and nonempty ranking byte-identical.`);
 }
} finally {fs.rmSync(root,{recursive:true,force:true});}
