const fs=require('fs'),vm=require('vm'),assert=require('assert'),path=require('path');
const source=fs.readFileSync(path.join(__dirname,'../assets/workspace.js'),'utf8');const code=source.slice(source.indexOf('let selectedPortfolioPending='),source.indexOf('async function refreshResearchRuns()'));
const nodes=Object.fromEntries(['res-portfolio','res-portfolio-new','res-portfolio-out'].map(id=>[id,{hidden:true,textContent:'',innerHTML:'',prepend:()=>{}}]));
const stored=new Map(),selection=new Map([['a',{name:'A',strategy:{name:'A'}}],['b',{name:'B',strategy:{name:'B'}}]]);let calls=[],reply,spread=2;
const context={$:id=>nodes[id],selectedStrategies:selection,localStorage:{getItem:k=>stored.get(k)||null,setItem:(k,v)=>stored.set(k,v),removeItem:k=>stored.delete(k)},crypto:{randomUUID:()=>`selection-${calls.length}`},selectedSymbol:()=> 'series-fixture-id',settings:()=>({spread,slippage:1,commission:2}),postJSON:async(url,body)=>{calls.push({url,body});return reply();},escapeHtml:s=>String(s).replaceAll('<','&lt;'),fmt:v=>String(v),document:{createElement:()=>({})},researchNotice:()=>'',drawChart:()=>{}};
vm.createContext(context);vm.runInContext(code,context);
(async()=>{
 reply=()=>{throw Error('lost response');};await context.buildSelectedPortfolio();const frozen=calls[0].body;assert.equal(stored.size,1);assert.match(nodes['res-portfolio-out'].textContent,/same selection/);
 selection.clear();spread=99;reply=()=>({recovery_required:true,archive:{started_run_id:'original-study'}});await context.buildSelectedPortfolio();assert.equal(calls[1].body,frozen);assert.equal(stored.size,0);assert.match(nodes['res-portfolio-out'].innerHTML,/original-study/);
 selection.set('a',{name:'A',strategy:{name:'A'}});selection.set('b',{name:'B',strategy:{name:'B'}});
 let resolve;reply=()=>new Promise(r=>resolve=r);const pending=context.buildSelectedPortfolio();await context.buildSelectedPortfolio();assert.equal(calls.length,3);resolve({error:'Invalid component',archive:{started_run_id:'failed-composition'}});await pending;assert.equal(nodes['res-portfolio'].disabled,false);assert.equal(stored.size,0);assert.match(nodes['res-portfolio-out'].innerHTML,/failed-composition/);
 reply=()=>({error:'Storage unavailable'});await context.buildSelectedPortfolio();assert.equal(stored.size,1);nodes['res-portfolio-new'].onclick();assert.equal(stored.size,0);assert.match(nodes['res-portfolio-out'].textContent,/does not cancel/);
 spread=3;reply=()=>new Promise(r=>resolve=r);const completed=context.buildSelectedPortfolio();spread=99;
 resolve({symbol:'EURUSD',archive:{started_run_id:'completed-study'},components:['A','B'],metrics:{net_profit:10,max_drawdown_pct:0},correlation:[[null,null],[null,null]],equity:[],no_trade_components:['A'],research_context:{}});await completed;
 assert.match(nodes['res-portfolio-out'].innerHTML,/Equal-weight portfolio · EURUSD/);assert.doesNotMatch(nodes['res-portfolio-out'].innerHTML,/series-fixture-id/);assert.match(nodes['res-portfolio-out'].innerHTML,/Spread 3 FX pips/);assert.match(nodes['res-portfolio-out'].innerHTML,/No trades in this period for: A/);assert.equal(stored.size,0);
 console.log('Selected portfolio request recovery UI passed');
})().catch(error=>{console.error(error);process.exitCode=1;});
