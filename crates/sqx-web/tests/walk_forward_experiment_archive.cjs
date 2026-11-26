const fs=require('node:fs'),vm=require('node:vm'),assert=require('node:assert/strict'),path=require('node:path');
const html=fs.readFileSync(path.join(__dirname,'../assets/index.html'),'utf8');
const workspace=fs.readFileSync(path.join(__dirname,'../assets/workspace.js'),'utf8');

// The renderer under test is the real showExperiment route plus the shared assessment panel.
const assessmentsCode=html.slice(html.indexOf('function showSearchAssessments('),html.indexOf('function readSearchRequirements('));
const experimentCode=workspace.slice(workspace.indexOf('function exposureLabel('),workspace.indexOf('async function refreshExperiments('));
assert.ok(assessmentsCode.length>0,'index.html slice must cover showSearchAssessments');
assert.ok(experimentCode.includes('async function showExperiment()'),'workspace.js slice must cover showExperiment');
assert.ok(experimentCode.includes('function showFrozenExperimentSettings('),'workspace.js slice must cover the frozen settings panel');

function makeElement(tag){
  return {
    tag,children:[],textContent:'',className:'',hidden:false,disabled:false,value:'',style:{},
    setAttribute(){},addEventListener(){},
    append(...children){this.children.push(...children);},
    appendChild(child){this.children.push(child);return child;},
    prepend(child){this.children.unshift(child);},
    replaceChildren(...children){this.children=children;},
    createCaption(){return this.appendChild(makeElement('caption'));},
    createTHead(){return this.appendChild(makeElement('thead'));},
    createTBody(){return this.appendChild(makeElement('tbody'));},
    insertRow(){return this.appendChild(makeElement('tr'));},
    insertCell(){return this.appendChild(makeElement('td'));},
    querySelectorAll(){return [];}
  };
}
const nodes={};
for(const id of ['experiment-trials','experiment-final','experiment-jobs','experiment-context','experiment-select','experiment-trial','experiment-search','experiment-status'])nodes[id]=makeElement('div');
nodes['experiment-select'].value='exp-1';
let statusCalls=[];
const fmt=(v,d)=>(v==null||Number.isNaN(v))?'-':(v===Infinity?'inf':Number(v).toFixed(d));
const ctx=vm.createContext({
  document:{getElementById:id=>nodes[id],createElement:makeElement,createTextNode:text=>({textContent:String(text),children:[]})},
  $:id=>nodes[id],
  fmt,
  getJSON:async url=>{statusCalls.push(url);return ctx.currentState;},
  setStrategy(){},drawChart(){},console,
  activeSearch:null,searchStarting:false,
  window:{confirm:()=>true}
});
ctx.currentState=null;
vm.runInContext(assessmentsCode,ctx);
vm.runInContext(experimentCode,ctx);

const descendants=node=>[node,...(node.children||[]).flatMap(descendants)];
const textOf=node=>descendants(node).map(n=>n.textContent).filter(Boolean).join('\n');
const panels=()=>descendants(nodes['experiment-trials']).filter(
  node=>node.tag==='details'&&node.children[0]&&String(node.children[0].textContent).startsWith('Walk-forward stabiliteit')
);
const candidateCard=name=>{
  const cards=descendants(nodes['experiment-trials']).filter(node=>node.tag==='details'&&node.children[0]);
  const found=cards.find(node=>node.children[0].textContent===name);
  assert.ok(found,'candidate card not found: '+name+' among '+JSON.stringify(cards.map(card=>card.children[0].textContent)));
  return found;
};

// Contract shape of /api/experiments/status: frozen plan.requirements.walk_forward and, per
// finished trial, report.assessment.walk_forward. Copied from the integration evidence.
const window=(index,isStart,isEnd,oosStart,oosEnd,t0,t1,t2,t3,starting,oosNet,oosTrades,isNet,isTrades)=>({
  index,is_start:isStart,is_end:isEnd,is_first_bar_time:t0,is_last_bar_time:t1,oos_start:oosStart,oos_end:oosEnd,oos_first_bar_time:t2,oos_last_bar_time:t3,
  starting_equity:starting,oos_final_equity:starting+oosNet,is_trades:isTrades,is_net_profit:isNet,is_profit_factor:0.18,is_eligible:true,is_exclusion_reason:null,
  oos_trades:oosTrades,oos_net_profit:oosNet,thresholds:{comparison:'at_least',min_is_trades:1,min_oos_trades_per_window:1},
  is_decision:'passed',oos_decision:'passed',decision:'passed',reason:'passed',errors:[]
});
const walkForward=trial=>({
  method:'fixed_candidate_segmented_v1',version:1,
  scope:'Segmented walk-forward stability check over already-seen search data; one frozen candidate evaluated identically in every window. Not an independent final test.',
  candidate_index:trial,strategy_name:'experiment-42-'+trial,
  frozen_specification:{windows:2,oos_fraction:0.3,min_is_trades:1,min_oos_trades_per_window:1,min_total_oos_net_profit:-100000},
  allowed_start_index:0,allowed_end_index:960,allowed_start_time:1704672000000,allowed_end_time:1704729540000,
  planned_windows:2,min_total_oos_net_profit:-100000,
  oos_total_net_profit:trial===3?417.7691368017355:-923.2214874270194,
  windows:trial===3
    ?[window(0,0,336,336,480,1704672000000,1704692100000,1704692160000,1704700740000,10000,8.536802601302185,7,109.8261710021697,18),
      window(1,480,816,816,960,1704700800000,1704720900000,1704720960000,1704729540000,10008.536802601302,409.23233420043334,7,-84.33059219609798,19)]
    :[window(0,0,336,336,480,1704672000000,1704692100000,1704692160000,1704700740000,10000,-140.9837374854028,6,-447.99442495913047,15),
      window(1,480,816,816,960,1704700800000,1704720900000,1704720960000,1704729540000,9859.016262514593,-782.2377499416166,20,-660.6708624445367,20)]
});
const audit={classification:'known_reuse_or_disclosed_exposure',observation_extent_complete:true,scope:'Reserved within this experiment only.',
  same_market_experiment_attempts:1,same_market_archived_backtest_records:0,same_market_archived_assistant_records:0,
  same_market_archived_mcp_generation_records:0,same_market_archived_pipeline_records:0,same_market_archived_crosscheck_records:0,
  same_market_archived_robustness_records:0,same_market_archived_comparison_records:0,same_market_evolution_generation_claims:0,known_overlaps:[]};
const execution=()=>({trades:[],equity:[],errors:[]});
function report(trial,{walkForward:archive,evidence=true}={}){
  const result={
    metrics:{net_profit:trial*10,trades:20,max_drawdown_pct:3.18},minimum_trades:1,evidence_sufficient:evidence,execution:execution(),
    assessment:{candidate_index:trial,name:'experiment-42-'+trial,passed:evidence,errors:[],checks:[
      {metric:'trades',decision:'passed',measured:20,threshold:1,comparison:'at_least'},
      {metric:'net_profit',decision:'passed',measured:120,threshold:-1000000,comparison:'at_least'},
      {metric:'profit_factor',decision:'passed',measured:1.4,threshold:0,comparison:'at_least'},
      {metric:'drawdown_pct',decision:'passed',measured:3.2,threshold:100,comparison:'at_most'}
    ]},
    validation:{metrics:{net_profit:5,trades:3},evidence_sufficient:true,execution:execution(),
      checks:[{metric:'trades',decision:'passed',measured:3,threshold:1,comparison:'at_least'}]}
  };
  if(archive)result.assessment.walk_forward=archive;
  return result;
}
function status({requirements={min_trades:1,min_net_profit:-1000000,min_profit_factor:0,max_drawdown_pct:100,walk_forward:{windows:2,oos_fraction:0.3,min_is_trades:1,min_oos_trades_per_window:1,min_total_oos_net_profit:-100000}},trials}={}){
  const events=[];
  for(const entry of trials){
    events.push({event:{kind:'trial_started',trial:entry.trial,strategy:{name:'experiment-42-'+entry.trial}}});
    events.push({event:{kind:'trial_finished',trial:entry.trial,report:report(entry.trial,entry.archive===undefined?{}:{walkForward:entry.archive})}});
  }
  const finalTrial=trials[trials.length-1].trial;
  events.push({event:{kind:'holdout_finished',report:{
    trial:finalTrial,metrics:{net_profit:1234.56,max_drawdown_pct:9.87,trades:77},evidence_sufficient:true,
    execution:execution(),interpretation:'Synthetic interpretation.',exposure_at_commitment:audit
  }}});
  return {
    state:'revealed',experiment_id:'exp-1',trials_attempted:trials.length,
    development_start_time:1704672000000,validation_start_time:1704693600000,holdout_start_time:1704700800000,holdout_end_time:1704729600000,
    market:{symbol:'EURUSD',timeframe:'M1',calendar:null,calendar_version:null},
    plan:{
      config:{account:{currency:'USD',long_financing_per_lot:0,short_financing_per_lot:0},initial_deposit:10000,spread_points:0,slippage_points:0,commission_per_lot:0},
      dataset_id:'dataset-1',engine_id:'engine-1',min_development_trades:1,min_holdout_trades:1,prior_exposure_disclosed:true,
      requirements
    },
    scope:'Reserved within this experiment only. Data access and testing outside this protocol are not prevented or certified.',
    current_exposure_audit:audit,jobs:[],events
  };
}

const frozenSettings='Walk-forward: 2 vensters · out-of-sample aandeel 0.30 · minimaal 1 trainingstrades en 1 testtrades per venster · gezamenlijke out-of-sample nettowinst ≥ -100000.00 USD';
const snapshot=()=>textOf(nodes['experiment-trials'])+'\n'+textOf(nodes['experiment-final']);

(async()=>{
  // --- 1. Reopening a saved final-test study shows the frozen settings and both window archives. ---
  ctx.currentState=status({trials:[
    {trial:0,archive:undefined},                                 // gate not requested -> no archive
    {trial:1,archive:walkForward(1)},                            // archived negative total, still a valid outcome
    {trial:2,archive:{method:'fixed_candidate_segmented_v1',version:1,windows:[]}}, // skipped gate with an empty archive
    {trial:3,archive:walkForward(3)}                             // revealed candidate
  ]});
  nodes['experiment-select'].value='exp-1';
  const archiveBefore=JSON.stringify(ctx.currentState);
  await ctx.showExperiment();
  assert.deepStrictEqual(statusCalls,['/api/experiments/status?id=exp-1'],'reopening reads the saved status once, no recomputation');
  const all=textOf(nodes['experiment-trials']);
  assert.ok(all.includes(frozenSettings),'frozen walk-forward settings must be shown from plan.requirements:\n'+frozenSettings);
  assert.ok(all.includes('Basiseisen: minimaal 1 trades'),'the existing frozen settings list must stay in place');
  assert.equal(panels().length,2,'exactly the two archived trials must carry a window panel');
  assert.ok(panels()[0].children[0].textContent==='Walk-forward stabiliteit · fixed_candidate_segmented_v1 v1','panel keeps the archived method and version');
  const trialOne=textOf(panels()[0]),trialThree=textOf(panels()[1]);
  assert.ok(trialOne.includes('Gezamenlijke out-of-sample nettowinst: -923.22 (rekeningvaluta) · drempel ≥ -100000.00'),'negative archived total must be shown as saved');
  assert.ok(trialThree.includes('Gezamenlijke out-of-sample nettowinst: 417.77 (rekeningvaluta) · drempel ≥ -100000.00'),'archived total and joint threshold must be shown');
  for(const range of ['[0–336)','[336–480)','[480–816)','[816–960)'])assert.ok(trialThree.includes(range),'archived window bounds missing: '+range);
  assert.ok(trialThree.includes('IS trades ≥ 1 · OOS trades ≥ 1'),'local IS/OOS minimum trades must be shown');

  // --- 2. No panel for a legacy, unrequested or skipped gate. ---
  assert.ok(!textOf(candidateCard('experiment-42-0 (#0)')).includes('Walk-forward stabiliteit'),'a trial without an archived gate must not show a walk-forward panel');
  assert.ok(!textOf(candidateCard('experiment-42-2 (#2)')).includes('Walk-forward stabiliteit'),'an empty/skipped archive must not show an empty panel');

  // --- 3. The walk-forward panel carries no final-test data. ---
  for(const panel of panels()){
    const text=textOf(panel);
    assert.ok(!text.includes('Eindtest'),'walk-forward panel must stay separate from the final test');
    assert.ok(!text.includes('1234.56')&&!text.includes('9.87'),'final-test metrics must never leak into the walk-forward panel');
  }
  const finalText=textOf(nodes['experiment-final']);
  assert.ok(finalText.includes('Final test · Candidate 3 · Net 1234.56'),'the final test renders separately: '+finalText);

  // --- 4. Reopening twice renders the identical archive and never mutates it. ---
  const first=snapshot();
  await ctx.showExperiment();
  assert.equal(snapshot(),first,'reopening must render the same frozen settings and windows');
  assert.equal(JSON.stringify(ctx.currentState),archiveBefore,'rendering must not modify the archived status');

  // --- 5. A legacy study without a walk-forward gate stays readable and panel-free. ---
  ctx.currentState=status({requirements:{min_trades:20,min_net_profit:0,min_profit_factor:1,max_drawdown_pct:10},trials:[{trial:0,archive:undefined},{trial:1,archive:undefined}]});
  await ctx.showExperiment();
  const legacy=textOf(nodes['experiment-trials']);
  assert.ok(!legacy.includes('Walk-forward:'),'a legacy study must not invent frozen walk-forward settings');
  assert.equal(panels().length,0,'a legacy study must not show a walk-forward window panel');

  console.log('Walk-forward experiment archive passed: showExperiment renders frozen plan.requirements.walk_forward and the saved per-window archive, unchanged on reopen, without final-test data, and no panel for legacy/unrequested/skipped gates.');
})().catch(error=>{console.error(error);process.exitCode=1;});
