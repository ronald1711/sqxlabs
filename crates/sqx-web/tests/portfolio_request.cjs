const fs=require('fs'),vm=require('vm'),assert=require('assert');
const html=fs.readFileSync(require('path').join(__dirname,'../assets/index.html'),'utf8');
const code=html.slice(html.indexOf('let portfolioRequestPending ='),html.indexOf('async function loadCatalog()'));
const nodes=Object.fromEntries(['pf-build','pf-new-request','pf-results','pf-count','pf-seed'].map(id=>[id,{hidden:true,value:id==='pf-count'?'40':'123',textContent:'',innerHTML:''}]));
const stored=new Map();let calls=[],symbol='EURUSD',reply;
const context={document:{getElementById:id=>nodes[id]},localStorage:{getItem:k=>stored.get(k)||null,setItem:(k,v)=>stored.set(k,v),removeItem:k=>stored.delete(k)},crypto:{randomUUID:()=>`request-${calls.length}`},selectedSymbol:()=>symbol,settings:()=>({deposit:10000}),postJSON:async(url,body)=>{calls.push({url,body});return reply();},escapeHtml:s=>String(s).replaceAll('<','&lt;'),fmt:v=>String(v),drawChart:()=>{}};
vm.createContext(context);vm.runInContext(code,context);
(async()=>{
 reply=()=>{throw Error('lost response');};await context.buildPortfolio();assert.equal(calls.length,1);assert.equal(stored.size,1);assert.match(nodes['pf-results'].textContent,/same request/);
 const first=calls[0].body;nodes['pf-count'].value='99';reply=()=>({recovery_required:true,archive:{started_run_id:'saved-1'},message:'Inspect saved status'});await context.buildPortfolio();assert.equal(calls[1].body,first);assert.equal(stored.size,0);assert.match(nodes['pf-results'].innerHTML,/saved-1/);
 let resolve;reply=()=>new Promise(r=>resolve=r);const running=context.buildPortfolio();await context.buildPortfolio();assert.equal(calls.length,3);assert.equal(nodes['pf-build'].disabled,true);resolve({error:'Insufficient candidates',archive:{started_run_id:'failed-study'}});await running;assert.equal(stored.size,0);assert.equal(nodes['pf-build'].disabled,false);assert.match(nodes['pf-results'].innerHTML,/failed-study/);
 reply=()=>({error:'Could not archive study'});await context.buildPortfolio();assert.equal(stored.size,1);nodes['pf-new-request'].onclick();assert.equal(stored.size,0);assert.match(nodes['pf-results'].textContent,/does not cancel/);
 symbol='';const before=calls.length;await context.buildPortfolio();assert.equal(calls.length,before);assert.match(nodes['pf-results'].textContent,/market data/);
 console.log('Portfolio request recovery UI passed');
})().catch(error=>{console.error(error);process.exitCode=1;});
