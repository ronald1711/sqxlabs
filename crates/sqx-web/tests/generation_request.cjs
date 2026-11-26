const fs=require('fs'),vm=require('vm'),assert=require('assert');
const html=fs.readFileSync(require('path').join(__dirname,'../assets/index.html'),'utf8');
const code=html.slice(html.indexOf('function showSearchAssessments('),html.indexOf('document.getElementById("btn-backtest").onclick'));
const nodes=Object.fromEntries(['btn-generate','gen-new-request','gen-results','count','seed'].map(id=>[id,{hidden:true,value:id==='count'?'40':'123',textContent:'',innerHTML:''}]));
nodes['gen-results'].querySelectorAll=()=>[];
nodes['gen-requirements']={checked:true};
nodes['gen-req-stress']={checked:false};
nodes['gen-req-mc']={checked:false};
for(const [id,value] of Object.entries({trades:20,profit:0,pf:1,dd:10})){
 nodes['gen-req-'+id]={value:String(value),valueAsNumber:value,checkValidity:()=>true};
}
const stored=new Map();let calls=[],symbol='EURUSD',reply,refreshes=0,reviews=0,reviewAccepted=true;
const context={window:{reviewGenerationData:async()=>{reviews++;return reviewAccepted?{dataset_id:'frozen-dataset',as_of_ms:180000,data_quality:{unclassified_intervals:1},exploratory_consent:true}:null;}},document:{getElementById:id=>nodes[id]},localStorage:{getItem:k=>stored.get(k)||null,setItem:(k,v)=>stored.set(k,v),removeItem:k=>stored.delete(k)},crypto:{randomUUID:()=>`request-${calls.length}`},selectedSymbol:()=>symbol,generatorInputs:()=>({deposit:10000,seed:123}),refreshResearchRuns:()=>{},refreshDatabank:()=>{refreshes++;},researchNotice:()=>'Exploratory',postJSON:async(url,body)=>{calls.push({url,body});return reply();},escapeHtml:s=>String(s).replaceAll('<','&lt;'),cls:()=>'pos',fmt:v=>String(v),drawChart:()=>{}};
vm.createContext(context);vm.runInContext(code,context);
(async()=>{
 reviewAccepted=false;await context.runGeneration();assert.equal(calls.length,0);assert.equal(stored.size,0);reviewAccepted=true;
 reply=()=>{throw Error('lost response');};await context.runGeneration();assert.equal(calls.length,1);assert.equal(stored.size,1);assert.match(nodes['gen-results'].textContent,/same request/);
 const first=calls[0].body;assert.deepStrictEqual(JSON.parse(first).requirements,{min_trades:20,min_net_profit:0,min_profit_factor:1,max_drawdown_pct:10});nodes['gen-req-trades'].valueAsNumber=99;assert.equal(JSON.parse(first).data_review.dataset_id,'frozen-dataset');assert.equal(JSON.parse(first).allow_data_gaps,true);const reviewCount=reviews;nodes['count'].value='99';reply=()=>({recovery_required:true,archive:{started_run_id:'saved-1'},message:'Inspect saved status'});await context.runGeneration();assert.equal(calls[1].body,first);assert.equal(reviews,reviewCount);assert.equal(stored.size,0);assert.match(nodes['gen-results'].textContent,/saved-1/);
 let resolve;reply=()=>new Promise(r=>resolve=r);const running=context.runGeneration();await Promise.resolve();await context.runGeneration();assert.equal(calls.length,3);assert.equal(nodes['btn-generate'].disabled,true);resolve({error:'Insufficient candidates',archive:{started_run_id:'failed-study'}});await running;assert.equal(stored.size,0);assert.equal(nodes['btn-generate'].disabled,false);assert.match(nodes['gen-results'].textContent,/failed-study/);
 reply=()=>({error:'Could not archive study'});await context.runGeneration();assert.equal(stored.size,1);nodes['gen-new-request'].onclick();assert.equal(stored.size,0);assert.match(nodes['gen-results'].textContent,/does not cancel/);
 reply=()=>({results:[],archive:{started_run_id:'complete-empty'},research_context:{}});await context.runGeneration();assert.equal(stored.size,0);assert.equal(refreshes,1);assert.match(nodes['gen-results'].innerHTML,/complete-empty/);assert.equal(nodes['gen-new-request'].hidden,true);
 reply=()=>({results:[{name:'<candidate>',profit_factor:2,net_profit:10,trades:5,dd_pct:1}],archive:{started_run_id:'complete-selected'},research_context:{}});await context.runGeneration();assert.equal(refreshes,2);assert.match(nodes['gen-results'].innerHTML,/&lt;candidate>/);assert.match(nodes['gen-results'].innerHTML,/complete-selected/);
 const element=tag=>({
   tag,children:[],textContent:'',
   setAttribute(){},
   append(...children){this.children.push(...children);},
   appendChild(child){this.append(child);return child;},
   replaceChildren(...children){this.children=children;},
   createCaption(){return this.appendChild(element('caption'));},
   createTHead(){return this.appendChild(element('thead'));},
   createTBody(){return this.appendChild(element('tbody'));},
   insertRow(){return this.appendChild(element('tr'));},
   insertCell(){return this.appendChild(element('td'));}
 });
 context.document.createElement=element;
 context.document.createTextNode=text=>({textContent:text,children:[]});
 let rendered;
 nodes['gen-results'].prepend=section=>{rendered=section;};
 const candidate=(id,name,value)=>({
   candidate_index:id,name,passed:false,errors:[],
   checks:[{metric:'net_profit',decision:'rejected',measured:value,threshold:0,comparison:'at_least'}]
 });
 reply=()=>({results:[],archive:{started_run_id:'comparison-study'},assessments:[candidate(1,'Alpha',-100),{...candidate(2,'Beta',-200),checks:[...candidate(2,'Beta',-200).checks,{period:'Validatie',metric:'net_profit',decision:'passed',measured:10,threshold:0,comparison:'at_least'}]}],research_context:{}});
 await context.runGeneration();
 const descendants=node=>[node,...node.children.flatMap(descendants)];
 const compare=descendants(rendered).find(node=>node.tag==='button');
 const selections=descendants(rendered).filter(node=>node.type==='checkbox');
 assert.equal(compare.disabled,true);
 for(const selection of selections){selection.checked=true;selection.onchange();}
 assert.equal(compare.disabled,false);
 compare.onclick();
 const comparison=descendants(rendered).find(node=>node.tag==='table');
 const compared=descendants(comparison).map(node=>node.textContent);
 for(const expected of ['Alpha','Beta','-100 · Afgewezen','-200 · Afgewezen','Niet uitgevoerd','Validatie · Nettoresultaat','10 · Geslaagd']){
   assert(compared.includes(expected),expected);
 }
 selections[0].checked=false;selections[0].onchange();
 assert.equal(compare.disabled,true);
 assert(!descendants(rendered).includes(comparison));
 symbol='';const before=calls.length;await context.runGeneration();assert.equal(calls.length,before);assert.match(nodes['gen-results'].textContent,/market data/);
 console.log('Generation request recovery UI passed');
})().catch(error=>{console.error(error);process.exitCode=1;});
