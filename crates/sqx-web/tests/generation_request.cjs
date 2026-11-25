const fs=require('fs'),vm=require('vm'),assert=require('assert');
const html=fs.readFileSync(require('path').join(__dirname,'../assets/index.html'),'utf8');
const code=html.slice(html.indexOf('let generationRequestPending='),html.indexOf('document.getElementById("btn-backtest").onclick'));
const nodes=Object.fromEntries(['btn-generate','gen-new-request','gen-results','count','seed'].map(id=>[id,{hidden:true,value:id==='count'?'40':'123',textContent:'',innerHTML:''}]));
nodes['gen-results'].querySelectorAll=()=>[];
const stored=new Map();let calls=[],symbol='EURUSD',reply,refreshes=0;
const context={document:{getElementById:id=>nodes[id]},localStorage:{getItem:k=>stored.get(k)||null,setItem:(k,v)=>stored.set(k,v),removeItem:k=>stored.delete(k)},crypto:{randomUUID:()=>`request-${calls.length}`},selectedSymbol:()=>symbol,generatorInputs:()=>({deposit:10000,seed:123}),refreshResearchRuns:()=>{},refreshDatabank:()=>{refreshes++;},researchNotice:()=>'Exploratory',postJSON:async(url,body)=>{calls.push({url,body});return reply();},escapeHtml:s=>String(s).replaceAll('<','&lt;'),cls:()=>'pos',fmt:v=>String(v),drawChart:()=>{}};
vm.createContext(context);vm.runInContext(code,context);
(async()=>{
 reply=()=>{throw Error('lost response');};await context.runGeneration();assert.equal(calls.length,1);assert.equal(stored.size,1);assert.match(nodes['gen-results'].textContent,/same request/);
 const first=calls[0].body;nodes['count'].value='99';reply=()=>({recovery_required:true,archive:{started_run_id:'saved-1'},message:'Inspect saved status'});await context.runGeneration();assert.equal(calls[1].body,first);assert.equal(stored.size,0);assert.match(nodes['gen-results'].textContent,/saved-1/);
 let resolve;reply=()=>new Promise(r=>resolve=r);const running=context.runGeneration();await context.runGeneration();assert.equal(calls.length,3);assert.equal(nodes['btn-generate'].disabled,true);resolve({error:'Insufficient candidates',archive:{started_run_id:'failed-study'}});await running;assert.equal(stored.size,0);assert.equal(nodes['btn-generate'].disabled,false);assert.match(nodes['gen-results'].textContent,/failed-study/);
 reply=()=>({error:'Could not archive study'});await context.runGeneration();assert.equal(stored.size,1);nodes['gen-new-request'].onclick();assert.equal(stored.size,0);assert.match(nodes['gen-results'].textContent,/does not cancel/);
 reply=()=>({results:[],archive:{started_run_id:'complete-empty'},research_context:{}});await context.runGeneration();assert.equal(stored.size,0);assert.equal(refreshes,1);assert.match(nodes['gen-results'].innerHTML,/complete-empty/);assert.equal(nodes['gen-new-request'].hidden,true);
 reply=()=>({results:[{name:'<candidate>',profit_factor:2,net_profit:10,trades:5,dd_pct:1}],archive:{started_run_id:'complete-selected'},research_context:{}});await context.runGeneration();assert.equal(refreshes,2);assert.match(nodes['gen-results'].innerHTML,/&lt;candidate>/);assert.match(nodes['gen-results'].innerHTML,/complete-selected/);
 symbol='';const before=calls.length;await context.runGeneration();assert.equal(calls.length,before);assert.match(nodes['gen-results'].textContent,/market data/);
 console.log('Generation request recovery UI passed');
})().catch(error=>{console.error(error);process.exitCode=1;});
