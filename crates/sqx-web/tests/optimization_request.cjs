const fs=require('fs'),vm=require('vm'),assert=require('assert');
const html=fs.readFileSync(require('path').join(__dirname,'../assets/index.html'),'utf8');
const code=html.slice(html.indexOf('let optimizationRequestPending='),html.indexOf('function parseGeneratorRanges'));
const nodes=Object.fromEntries(['opt-run','opt-new-request','opt-results','opt-count','opt-kind','opt-windows','opt-min-is','opt-min-oos','opt-oos'].map(id=>[id,{hidden:true,value:id==='opt-count'?'40':'123',textContent:'',innerHTML:''}]));
const stored=new Map();let calls=[],symbol='EURUSD',reply;
const context={document:{getElementById:id=>nodes[id]},localStorage:{getItem:k=>stored.get(k)||null,setItem:(k,v)=>stored.set(k,v),removeItem:k=>stored.delete(k)},crypto:{randomUUID:()=>`request-${calls.length}`},selectedSymbol:()=>symbol,generatorInputs:()=>({deposit:10000,seed:123}),refreshResearchRuns:()=>{},postJSON:async(url,body)=>{calls.push({url,body});return reply();},escapeHtml:s=>String(s).replaceAll('<','&lt;'),fmt:v=>String(v),drawChart:()=>{}};
vm.createContext(context);vm.runInContext(code,context);
(async()=>{
 reply=()=>{throw Error('lost response');};await context.runOptimization();assert.equal(calls.length,1);assert.equal(stored.size,1);assert.match(nodes['opt-results'].textContent,/same request/);
 const first=calls[0].body;nodes['opt-count'].value='99';reply=()=>({recovery_required:true,archive:{started_run_id:'saved-1'},message:'Inspect saved status'});await context.runOptimization();assert.equal(calls[1].body,first);assert.equal(stored.size,0);assert.match(nodes['opt-results'].innerHTML,/saved-1/);
 let resolve;reply=()=>new Promise(r=>resolve=r);const running=context.runOptimization();await context.runOptimization();assert.equal(calls.length,3);assert.equal(nodes['opt-run'].disabled,true);resolve({error:'Insufficient candidates',archive:{started_run_id:'failed-study'}});await running;assert.equal(stored.size,0);assert.equal(nodes['opt-run'].disabled,false);assert.match(nodes['opt-results'].innerHTML,/failed-study/);
 reply=()=>({error:'Could not archive study'});await context.runOptimization();assert.equal(stored.size,1);nodes['opt-new-request'].onclick();assert.equal(stored.size,0);assert.match(nodes['opt-results'].textContent,/does not cancel/);
 symbol='';const before=calls.length;await context.runOptimization();assert.equal(calls.length,before);assert.match(nodes['opt-results'].textContent,/market data/);
 console.log('Optimization request recovery UI passed');
})().catch(error=>{console.error(error);process.exitCode=1;});
