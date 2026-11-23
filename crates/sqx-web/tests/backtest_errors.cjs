const fs=require('node:fs'),vm=require('node:vm'),assert=require('node:assert/strict');
const source=fs.readFileSync('crates/sqx-web/assets/workspace.js','utf8');
const start=source.indexOf('runBacktest = async function()'),end=source.indexOf('\nfunction renderTrades',start);
assert(start>=0 && end>start);
const status={textContent:''};let calls=0;let invalidSettings=true;
const context=vm.createContext({$:()=>status,readStrategy:()=>({name:'fixture'}),settings:()=>{if(invalidSettings)throw Error('spread must be finite');return {};},selectedSymbol:()=> 'EURUSD',contextKey:()=>'',copy:x=>x,postJSON:async()=>{calls++;throw Error('lost response');},runHistory:[],lastRun:null});
vm.runInContext(source.slice(start,end),context);
(async()=>{
 await context.runBacktest();assert.equal(calls,0);assert.match(status.textContent,/spread must be finite/);
 invalidSettings=false;await context.runBacktest();assert.equal(calls,1);
 assert.match(status.textContent,/may have completed and archived/);assert.match(status.textContent,/before retrying/);
 assert.equal(context.runHistory.length,0);assert.equal(context.lastRun,null);
 context.postJSON=async()=>({error:'invalid strategy'});await context.runBacktest();
 assert.equal(status.textContent,'Run failed: invalid strategy');
 console.log('Backtest errors passed: invalid settings send nothing; lost responses report uncertain completion.');
})().catch(error=>{console.error(error);process.exitCode=1;});
