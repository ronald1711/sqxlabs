const fs=require('node:fs'),vm=require('node:vm'),assert=require('node:assert/strict');
const source=fs.readFileSync('crates/sqx-web/assets/index.html','utf8');
const start=source.indexOf('function robustnessArchiveNotice('),end=source.indexOf('async function runRobustness()',start);
assert(start>=0 && end>start);
let calls=0,invalid=true;
const context=vm.createContext({escapeHtml:s=>String(s).replaceAll('<','&lt;').replaceAll('>','&gt;'),fmt:n=>String(n),selectedSymbol:()=> 'EURUSD',settings:()=>{if(invalid)throw Error('invalid spread');return {};},postJSON:async()=>{calls++;throw Error('network');}});
vm.runInContext(source.slice(start,end),context);
(async()=>{
 let result=await context.requestRobustness({name:'fixture'});assert.equal(calls,0);assert.match(result.error,/invalid spread/);
 invalid=false;result=await context.requestRobustness({name:'fixture'});assert.equal(calls,1);assert.match(result.error,/may still be running or have saved partial progress/);assert.match(result.error,/before retrying/);
 const failed={archive:{run_id:'saved-id'},interpretation:'Exploratory only',what_if:[{errors:['<invalid cost>']}],mc:{errors:['No trades'],net_p5:null}};
 const html=context.robustnessQuickSummary(failed,'fixture');assert.match(html,/saved-id/);assert.match(html,/Monte Carlo unavailable: No trades/);assert.match(html,/&lt;invalid cost&gt;/);assert.doesNotMatch(html,/MC net p5/);
 const successful=context.robustnessQuickSummary({archive:{run_id:'run-2'},mc:{net_p5:10,net_p50:20,net_p95:30,losing_pct:0},trades:5},'<name>');assert.match(successful,/run-2/);assert.match(successful,/MC net p5 10/);assert.match(successful,/&lt;name&gt;/);
 const failedRun=context.robustnessQuickSummary({archive:{run_id:'failed-run'},error:'invalid strategy'},'fixture');assert.match(failedRun,/failed-run/);assert.match(failedRun,/invalid strategy/);
 const interrupted=context.robustnessQuickSummary({archive:{started_run_id:'<checkpoint>'},error:'Storage unavailable'},'fixture');assert.match(interrupted,/&lt;checkpoint&gt;/);assert.match(interrupted,/resume with the original application build/);assert.match(interrupted,/Storage unavailable/);assert.doesNotMatch(interrupted,/MC net p5/);
 console.log('Robustness UI passed: invalid settings, uncertain response, archived failures and MC errors stay visible.');
})().catch(error=>{console.error(error);process.exitCode=1;});
