const fs=require('node:fs'),vm=require('node:vm'),assert=require('node:assert/strict');
const html=fs.readFileSync(require('node:path').join(__dirname,'../assets/index.html'),'utf8');
const code=html.slice(html.indexOf('function fmtExact('),html.indexOf('document.getElementById("btn-backtest").onclick'));
assert.ok(code.length>0,'slice covers fmtExact..runGeneration');

// Faithful `fmt` used by the rendered assessment text.
const fmt=(v,d)=>(v==null||Number.isNaN(v))?'-':(v===Infinity?'inf':Number(v).toFixed(d));

function makeNode(value){return {value:String(value),valueAsNumber:Number(value),checkValidity:()=>true,textContent:'',innerHTML:'',hidden:false,checked:false};}
const nodes={
  'gen-requirements':{checked:true},
  'gen-req-trades':makeNode(20),'gen-req-profit':makeNode(0),'gen-req-pf':makeNode(1),'gen-req-dd':makeNode(10),
  'gen-req-stress':{checked:false},'gen-req-mc':{checked:false},
  'gen-req-wf':{checked:true},
  'gen-req-wf-windows':makeNode(2),'gen-req-wf-oos':makeNode(0.3),'gen-req-wf-min-is':makeNode(2),'gen-req-wf-min-oos':makeNode(2),'gen-req-wf-profit':makeNode(0),
  'gen-new-request':{textContent:'',hidden:false,onclick:null},'btn-generate':{disabled:false,onclick:null},'gen-results':{textContent:'',innerHTML:'',querySelectorAll:()=>[]},
};
const ctx=vm.createContext({
  document:{getElementById:id=>nodes[id]},
  fmt,cls:()=>'pos',escapeHtml:s=>String(s),
  getJSON:()=>{throw Error('not called in this test');},setStrategy:()=>{},recordedExecution:()=>({}),drawChart:()=>{},
});
vm.runInContext(code,ctx);

(async()=>{
  // --- 1. Toggle: on includes walk_forward, off keeps the field absent. ---
  const on=ctx.readSearchRequirements();
  assert.deepStrictEqual(JSON.parse(JSON.stringify(on.walk_forward)),{windows:2,oos_fraction:0.3,min_is_trades:2,min_oos_trades_per_window:2,min_total_oos_net_profit:0});
  nodes['gen-req-wf'].checked=false;
  const off=ctx.readSearchRequirements();
  assert.equal('walk_forward' in off,false,'absent toggle must not emit walk_forward');
  assert.equal(Object.keys(off).length,4,'legacy requirements stay byte-identical in shape');
  // Non-integer window count is rejected before search.
  nodes['gen-req-wf'].checked=true;nodes['gen-req-wf-windows']=makeNode(2.5);
  assert.throws(()=>ctx.readSearchRequirements(),/geheel getal/);
  nodes['gen-req-wf-windows']=makeNode(2);

  // --- 1b. A checked mandatory gate can never be dropped while assessment is off. ---
  nodes['gen-requirements'].checked=false;
  nodes['gen-req-wf'].checked=true;
  assert.throws(()=>ctx.readSearchRequirements(),/Kandidaten automatisch toetsen/,'an armed gate without assessment must be rejected, not silently dropped');
  ctx.syncSearchRequirementToggles();
  assert.equal(nodes['gen-req-wf'].checked,false,'assessment off must not leave a gate armed');
  assert.equal(nodes['gen-req-wf'].disabled,true,'dependent gates are disabled while assessment is off');
  assert.equal(ctx.readSearchRequirements(),undefined,'no assessment means no requirements, never a silent gate');
  // Arming a gate through the real request route re-enables assessment instead of losing the gate.
  nodes['gen-req-wf'].checked=true;
  nodes['gen-req-wf'].onchange.call(nodes['gen-req-wf']);
  assert.equal(nodes['gen-requirements'].checked,true,'arming walk-forward must switch assessment back on');
  assert.equal(nodes['gen-req-wf'].disabled,false,'a gate becomes editable once assessment is on');
  assert.deepStrictEqual(JSON.parse(JSON.stringify(ctx.readSearchRequirements().walk_forward)),{windows:2,oos_fraction:0.3,min_is_trades:2,min_oos_trades_per_window:2,min_total_oos_net_profit:0});
  nodes['gen-requirements'].checked=false;
  nodes['gen-requirements'].onchange.call(nodes['gen-requirements']);
  assert.equal(nodes['gen-req-wf'].checked,false,'switching assessment off clears the dependent gate');
  nodes['gen-requirements'].checked=true;
  nodes['gen-requirements'].onchange.call(nodes['gen-requirements']);
  nodes['gen-req-wf'].checked=true;

  // --- 2. Per-window archive: IS+OOS net/trades, bounds/times, thresholds, reason; no red reject on a negative net. ---
  const element=tag=>({
    tag,children:[],textContent:'',
    setAttribute(){},
    append(...children){this.children.push(...children);},
    appendChild(child){this.children.push(child);return child;},
    replaceChildren(...children){this.children=children;},
    createCaption(){return this.appendChild(element('caption'));},
    createTHead(){return this.appendChild(element('thead'));},
    createTBody(){return this.appendChild(element('tbody'));},
    insertRow(){return this.appendChild(element('tr'));},
    insertCell(){return this.appendChild(element('td'));}
  });
  ctx.document.createElement=element;
  ctx.document.createTextNode=text=>({textContent:text,children:[]});
  let rendered=null;const box={prepend:s=>{rendered=s;},querySelectorAll:()=>[]};
  const window=(index,isStart,isEnd,oosStart,oosEnd,t0,t1,t2,t3,starting,oosNet,oosTrades,isNet)=>{
    const final=starting+oosNet;
    return {index,is_start:isStart,is_end:isEnd,is_first_bar_time:t0,is_last_bar_time:t1,oos_start:oosStart,oos_end:oosEnd,oos_first_bar_time:t2,oos_last_bar_time:t3,
      starting_equity:starting,oos_final_equity:final,is_trades:4,is_net_profit:isNet,is_profit_factor:0.5,is_eligible:true,is_exclusion_reason:null,
      oos_trades:oosTrades,oos_net_profit:oosNet,thresholds:{min_is_trades:2,min_oos_trades_per_window:2,comparison:'at_least'},
      is_decision:'passed',oos_decision:'passed',decision:'passed',reason:'passed',errors:[]};
  };
  const wfCandidate={
    candidate_index:0,name:'WF Candidate',passed:true,errors:[],
    checks:[
      {metric:'trades',decision:'passed',measured:20,threshold:20,comparison:'at_least'},
      {metric:'walk_forward_total_oos_net_profit',decision:'passed',measured:50,threshold:50,comparison:'at_least'}
    ],
    walk_forward:{method:'fixed_candidate_segmented_v1',version:1,scope:'Segmented walk-forward stability check over already-seen search data; not an independent final test.',candidate_index:0,strategy_name:'WF Candidate',
      frozen_specification:{windows:2,oos_fraction:0.3,min_is_trades:2,min_oos_trades_per_window:2,min_total_oos_net_profit:50},
      allowed_start_index:0,allowed_end_index:160,allowed_start_time:0,allowed_end_time:9540000,planned_windows:2,min_total_oos_net_profit:50,oos_total_net_profit:50,
      windows:[window(0,0,56,56,80,0,3300000,3360000,4740000,10000,-20,2,-20),window(1,80,112,112,160,4800000,6660000,6720000,9540000,9980,70,2,70)]
    }
  };
  ctx.showSearchAssessments(box,[wfCandidate],'study-1');
  const descendants=node=>[node,...(node.children||[]).flatMap(descendants)];
  const all=descendants(rendered);
  const texts=()=>all.map(node=>node.textContent).filter(Boolean);
  const has=t=>assert.ok(texts().some(x=>String(x).includes(t)),'missing rendered text: '+t);
  has('Walk-forward stabiliteit · fixed_candidate_segmented_v1 v1');
  has('Verkennend — gebruikt al geziene zoekdata; geen onafhankelijke eindtest.');
  has('Gezamenlijke out-of-sample nettowinst gemeten: 50.00 (rekeningvaluta) · drempel ≥ 50.00');
  has('Gezamenlijke out-of-sample nettowinst: 50 ≥ 50 · Geslaagd');
  has('-20.00');has('70.00');
  has('IS trades ≥ 2 · OOS trades ≥ 2');
  has('[0–56)');has('[56–80)');has('[80–112)');has('[112–160)');
  has('1970-01-01T00:00:00.000Z');has('1970-01-01T00:55:00.000Z');
  has('10000.00');has('9980.00');
  // Each window decision is a badge, not a bare word at the far right of the row.
  const badges=all.filter(node=>node.tag==='span'&&node.className==='status-pill');
  assert.equal(badges.filter(node=>node.textContent==='Geslaagd').length,2,'both window decisions must render as status badges');
  assert.equal(all.filter(node=>node.tag==='td'&&node.textContent==='Geslaagd').length,0,'the decision must not stay a bare cell');
  // The decision now sits directly next to the OOS net it judges.
  const decisionRow=all.find(node=>node.tag==='tr'&&node.children.some(cell=>cell.children&&cell.children.some(grand=>grand.className==='status-pill')));
  assert.ok(decisionRow,'a rendered window row must exist');
  const decisionIndex=decisionRow.children.findIndex(cell=>cell.children&&cell.children[0]&&cell.children[0].className==='status-pill');
  assert.equal(decisionIndex,9,'the decision badge must sit directly after OOS net and before the local thresholds');
  assert.equal(decisionRow.children[decisionIndex-1].textContent,'-20.00','the cell left of the badge must be the OOS net it judges');
  // The −20 window must render as a plain value in a *passed* row, never as a red rejection.
  const lossCells=all.filter(node=>node.tag==='td'&&node.textContent==='-20.00');
  assert.ok(lossCells.length>=1,'negative OOS net should appear as a table cell');
  for(const cell of lossCells)assert.notEqual(cell.className,'neg','negative OOS net must not carry a red reject class');
  // Same row carries the window decision 'Geslaagd' — a valid losing window is not rejected.
  assert.ok(texts().includes('Geslaagd'),'passed status must be rendered');

  // --- 2b. The joint gate keeps stored precision: 0.004 stays visible, a missing value stays unavailable. ---
  const preciseCandidate=JSON.parse(JSON.stringify(wfCandidate));
  preciseCandidate.walk_forward.min_total_oos_net_profit=0.004;
  preciseCandidate.walk_forward.oos_total_net_profit=0.004;
  let preciseBox={prepend:s=>{rendered=s;}};
  ctx.showSearchAssessments(preciseBox,[preciseCandidate],undefined);
  const preciseText=()=>descendants(rendered).map(node=>node.textContent).filter(Boolean).join('\n');
  assert.ok(preciseText().includes('Gezamenlijke out-of-sample nettowinst gemeten: 0.004 (rekeningvaluta) · drempel ≥ 0.004'),'a 0.004 joint threshold must not round to 0.00:\n'+preciseText());
  preciseCandidate.walk_forward.oos_total_net_profit=null;
  ctx.showSearchAssessments(preciseBox,[preciseCandidate],undefined);
  assert.ok(preciseText().includes('Gezamenlijke out-of-sample nettowinst gemeten: onbeschikbaar · drempel ≥ 0.004'),'a missing measured total must read onbeschikbaar, never 0:\n'+preciseText());

  // --- 3. Five statuses are all displayable. ---
  const statusCandidate=decision=>({candidate_index:1,name:'Status '+decision,passed:decision==='passed',errors:[],checks:[{metric:'walk_forward_total_oos_net_profit',decision,measured:decision==='passed'||decision==='rejected'?1:null,threshold:0,comparison:'at_least'}],walk_forward:null});
  let statusBox={prepend:s=>{rendered=s;}};
  ctx.showSearchAssessments(statusBox,['passed','rejected','insufficient_evidence','technical_error','not_run'].map(statusCandidate),undefined);
  const statusTexts=descendants(rendered).map(node=>node.textContent).filter(Boolean).join('\n');
  for(const [decision,label] of [['passed','Geslaagd'],['rejected','Afgewezen'],['insufficient_evidence','Onvoldoende bewijs'],['technical_error','Technische fout'],['not_run','Niet uitgevoerd']]){
    assert.ok(statusTexts.includes(label),'status label missing for '+decision+': '+label);
  }
  // A null measurement is 'onbeschikbaar', never a fabricated zero.
  assert.ok(statusTexts.includes('onbeschikbaar'),'null measured must read onbeschikbaar');
  assert.ok(!statusTexts.includes(': 0 ≥ 0'),'missing evidence must not fabricate a zero');
  // A skipped gate that archived an empty window list renders no panel and never throws.
  ctx.showSearchAssessments(statusBox,[{candidate_index:9,name:'Skipped gate',passed:false,errors:[],checks:[{metric:'walk_forward_total_oos_net_profit',decision:'not_run',measured:null,threshold:50,comparison:'at_least'}],walk_forward:{method:'fixed_candidate_segmented_v1',version:1,windows:[]}}],undefined);
  const skippedText=descendants(rendered).map(node=>node.textContent).filter(Boolean).join('\n');
  assert.ok(!skippedText.includes('Walk-forward stabiliteit'),'an empty archived window list must not render a walk-forward panel');
  assert.ok(skippedText.includes('Niet uitgevoerd'),'a skipped gate still reports not run');

  // --- 4. Reopening a study re-reads the same archived details without recomputation. ---
  let reopenBox={prepend:s=>{rendered=s;}};
  ctx.showSearchAssessments(reopenBox,[wfCandidate],'study-1');
  const reopened=descendants(rendered).map(node=>node.textContent).filter(Boolean).join('\n');
  assert.ok(reopened.includes('Walk-forward stabiliteit · fixed_candidate_segmented_v1 v1'),'reopened study must show the archived walk-forward panel');
  assert.ok(reopened.includes('-20.00')&&reopened.includes('70.00')&&reopened.includes('drempel ≥ 50.00'),'reopened study must show the same per-window details');

  // --- 5. A study archived before the gate existed stays readable as-is. ---
  const legacyCandidate={candidate_index:4,name:'Legacy Candidate',passed:true,errors:[],
    checks:[
      {metric:'trades',decision:'passed',measured:20,threshold:20,comparison:'at_least'},
      {metric:'net_profit',decision:'passed',measured:120,threshold:0,comparison:'at_least'},
      {metric:'profit_factor',decision:'passed',measured:1.4,threshold:1,comparison:'at_least'},
      {metric:'drawdown_pct',decision:'passed',measured:3.2,threshold:10,comparison:'at_most'}
    ]};
  let legacyBox={prepend:s=>{rendered=s;}};
  ctx.showSearchAssessments(legacyBox,[legacyCandidate],'study-legacy');
  const legacyText=descendants(rendered).map(node=>node.textContent).filter(Boolean).join('\n');
  assert.ok(legacyText.includes('Legacy Candidate'),'legacy assessment must still render its candidate');
  assert.ok(legacyText.includes('Geslaagd'),'legacy assessment keeps its passed status');
  assert.ok(!legacyText.includes('Walk-forward stabiliteit'),'a study recorded before the gate must not show a walk-forward panel');
  assert.ok(!legacyText.includes('Gezamenlijke out-of-sample nettowinst'),'no fabricated walk-forward threshold for an old study');

  console.log('Walk-forward UI passed: toggle, per-window IS/OOS net + bounds/times + local thresholds + reason, five statuses, null (not zero) measurements, no red reject on a losing window, identical re-read on reopen, and legacy studies without the gate.');
})().catch(error=>{console.error(error);process.exitCode=1;});
