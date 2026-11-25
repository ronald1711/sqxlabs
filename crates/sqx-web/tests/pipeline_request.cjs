const fs=require('fs'),vm=require('vm'),assert=require('assert');
const html=fs.readFileSync(require('path').join(__dirname,'../assets/index.html'),'utf8');
const code=html.slice(html.indexOf('let pipelineRequestPending='),html.indexOf('let optimizationRequestPending='));
const nodes=Object.fromEntries(['proj-run','proj-new-request','proj-log','proj-count','proj-keep'].map(id=>[id,{hidden:true,value:id==='proj-count'?'40':'123',textContent:'',innerHTML:''}]));
const stored=new Map();let calls=[],symbol='EURUSD',reply;
const context={document:{getElementById:id=>nodes[id]},localStorage:{getItem:k=>stored.get(k)||null,setItem:(k,v)=>stored.set(k,v),removeItem:k=>stored.delete(k)},crypto:{randomUUID:()=>`request-${calls.length}`},selectedSymbol:()=>symbol,generatorInputs:()=>({deposit:10000,seed:123}),refreshResearchRuns:()=>{},researchNotice:()=>'Exploratory',postJSON:async(url,body)=>{calls.push({url,body});return reply();},escapeHtml:s=>String(s).replaceAll('<','&lt;'),fmt:v=>String(v),drawChart:()=>{}};
vm.createContext(context);vm.runInContext(code,context);
(async()=>{
 reply=()=>{throw Error('lost response');};await context.runPipeline();assert.equal(calls.length,1);assert.equal(stored.size,1);assert.match(nodes['proj-log'].textContent,/same request/);
 const first=calls[0].body;nodes['proj-count'].value='99';reply=()=>({recovery_required:true,archive:{started_run_id:'saved-1'},message:'Inspect saved status'});await context.runPipeline();assert.equal(calls[1].body,first);assert.equal(stored.size,0);assert.match(nodes['proj-log'].textContent,/saved-1/);
 let resolve;reply=()=>new Promise(r=>resolve=r);const running=context.runPipeline();await context.runPipeline();assert.equal(calls.length,3);assert.equal(nodes['proj-run'].disabled,true);resolve({error:'Insufficient candidates',archive:{started_run_id:'failed-study'}});await running;assert.equal(stored.size,0);assert.equal(nodes['proj-run'].disabled,false);assert.match(nodes['proj-log'].textContent,/failed-study/);
 reply=()=>({error:'Could not archive study'});await context.runPipeline();assert.equal(stored.size,1);nodes['proj-new-request'].onclick();assert.equal(stored.size,0);assert.match(nodes['proj-log'].textContent,/does not cancel/);
 symbol='';const before=calls.length;await context.runPipeline();assert.equal(calls.length,before);assert.match(nodes['proj-log'].textContent,/market data/);
 console.log('Pipeline request recovery UI passed');
})().catch(error=>{console.error(error);process.exitCode=1;});
