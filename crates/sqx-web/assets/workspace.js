/* Research workspace: one strategy draft, explicit assumptions, and session run evidence. */
'use strict';
const $ = id => document.getElementById(id);
const copy = value => JSON.parse(JSON.stringify(value));
function escapeHtml(value) { return String(value == null ? '' : value).replace(/[&<>"']/g, c => ({'&':'&amp;','<':'&lt;','>':'&gt;','"':'&quot;',"'":'&#39;'}[c])); }
let strategyModel = null, undoHistory = [], savedStrategy = '', editorView = 'visual', visualSupported = true;
let dataContextError = '', dataContextRequest = 0;
let dataContext = null, runHistory = [], activeProfile = null, lastRun = null;
function announce(message) { $('workspace-message').textContent = message; $('workspace-message').hidden = !message; }
function showView(view) {
  editorView = view;
  $('visual-editor').hidden = view !== 'visual'; $('code-editor').hidden = view !== 'code';
  $('view-visual').setAttribute('aria-pressed', String(view === 'visual'));
  $('view-code').setAttribute('aria-pressed', String(view === 'code'));
}
function strategyShape(s) {
  if (!s || typeof s !== 'object' || Array.isArray(s) || typeof s.name !== 'string' || !Array.isArray(s.signals) || !s.money_management) throw new Error('A strategy needs a name, signals array and money management.');
  if (typeof s.long !== 'boolean' || typeof s.short !== 'boolean') throw new Error('Long and short must be true or false.');
  for (const sig of s.signals) {
    if (!CATALOG.signal_kinds.includes(sig.kind) || !Array.isArray(sig.conditions) || !['And','Or'].includes(sig.join)) throw new Error('Each signal needs a supported kind, condition list and And/Or join.');
    for (const c of sig.conditions) if (!c.left || !c.right || !CATALOG.comparators.includes(c.cmp)) throw new Error('Each condition needs left/right operands and a supported comparison.');
  }
}
function supportsVisual(s) {
  const operands = (op, left) => {
    if (op.Price) return CATALOG.price_fields.includes(op.Price[0]) && op.Price[1] === 0;
    if (!left) return Number.isFinite(op.Const);
    if (!op.Indicator) return false;
    const spec = CATALOG.indicators.find(x => x.name === op.Indicator[0]);
    return spec && Array.isArray(op.Indicator[1]) && op.Indicator[1].length === spec.params.length && spec.params.every(p => op.Indicator[1].some(x => x.name === p.name && Number.isFinite(x.value)));
  };
  return s.signals.every(sig => sig.fuzzy == null && sig.conditions.every(c => operands(c.left,true) && operands(c.right,false))) &&
    new Set(s.signals.map(x => x.kind)).size === s.signals.length &&
    [s.stop_loss,s.profit_target].every(x => x == null || Number.isFinite(x.Points)) &&
    CATALOG.money_management.some(x => x.type === Object.keys(s.money_management)[0] && x.params.every(p => Number.isFinite(s.money_management[x.type][p.name])));
}
function setStrategy(s, reason = 'Draft updated', remember = true, render = true) {
  strategyShape(s);
  if (remember && strategyModel && JSON.stringify(strategyModel) !== JSON.stringify(s)) { undoHistory.push(copy(strategyModel)); if (undoHistory.length > 40) undoHistory.shift(); }
  strategyModel = copy(s); $('strategy').value = JSON.stringify(s,null,2);
  visualSupported = supportsVisual(s);
  $('editor-notice').hidden = visualSupported;
  $('editor-notice').textContent = visualSupported ? '' : 'This strategy uses advanced rules. Edit it in Strategy JSON; all fields are preserved when running, saving and exporting.';
  $('view-visual').disabled = !visualSupported;
  if (!visualSupported) showView('code');
  else if (render) hydrateVisual(s);
  $('draft-status').textContent = JSON.stringify(s) === savedStrategy ? 'Saved strategy' : reason;
  $('strategy-undo').disabled = !undoHistory.length;
  updateContext();
}
function hydrateVisual(s) {
  $('aw-name').value=s.name; $('aw-long').checked=s.long; $('aw-short').checked=s.short;
  $('aw-sl').value=s.stop_loss ? s.stop_loss.Points : 0; $('aw-pt').value=s.profit_target ? s.profit_target.Points : 0;
  const opts=s.trading_options || {};
  $('aw-eod').checked=!!opts.exit_at_end_of_day; $('aw-friday').checked=!!opts.exit_on_friday; $('aw-maxtrades').value=opts.max_trades_per_day || 0;
  $('aw-close-time').value=Number.isInteger(opts.session_close_utc_minute) ? String(Math.floor(opts.session_close_utc_minute/60)).padStart(2,'0')+':'+String(opts.session_close_utc_minute%60).padStart(2,'0') : '';
  const mm=Object.keys(s.money_management)[0]; $('aw-mm-type').value=mm; renderMmParams();
  document.querySelectorAll('.mm-param').forEach(el => el.value=s.money_management[mm][el.dataset.name]);
  for (const kind of CATALOG.signal_kinds) {
    const section=$('sig-'+kind), signal=s.signals.find(x=>x.kind===kind);
    section.querySelector('.conds').replaceChildren(); section.querySelector('.join').value=signal ? signal.join : 'And';
    for (const condition of signal ? signal.conditions : []) {
      const row=buildConditionRow(); section.querySelector('.conds').append(row);
      row.querySelector('.left-type').value=condition.left.Price ? 'price:'+condition.left.Price[0] : 'ind:'+condition.left.Indicator[0];
      updateLeftParams(row);
      if (condition.left.Indicator) row.querySelectorAll('.param').forEach(el => el.value=condition.left.Indicator[1].find(p=>p.name===el.dataset.name).value);
      row.querySelector('.cmp').value=condition.cmp;
      row.querySelector('.right-type').value=condition.right.Price ? 'price:'+condition.right.Price[0] : 'const';
      updateRightValue(row); if (!condition.right.Price) row.querySelector('.const').value=condition.right.Const;
      updateCmp(row);
    }
    section.hidden=kind.startsWith('Long') ? !s.long : !s.short;
  }
  labelControls();
}
const serializeVisual = buildStrategyJSON;
function commitVisual(render = true) {
  if (!visualSupported || !strategyModel) return;
  const fields=JSON.parse(serializeVisual());
  // Never discard options outside the visual editor (exits, hours, confirmations, etc.).
  const next=Object.assign(copy(strategyModel),fields,{exits:strategyModel.exits || [],trading_options:Object.assign({},strategyModel.trading_options,fields.trading_options)});
  next.trading_options.limit_hours=(strategyModel.trading_options || {}).limit_hours || null;
  setStrategy(next, 'Draft updated', true, render);
}
function markStrategySaved(saved) { savedStrategy=JSON.stringify(saved); $('draft-status').textContent=JSON.stringify(strategyModel)===savedStrategy?'Saved strategy':'Draft changed since save'; }
function readStrategy() { if (editorView==='visual' && Array.from($('visual-editor').querySelectorAll('input[type=number]')).some(el=>!el.value || !el.checkValidity())) throw new Error('Complete the numeric fields with valid values.'); const s=JSON.parse($('strategy').value); strategyShape(s); return s; }
function day(t) { return t ? new Date(t).toISOString().slice(0,10) : '—'; }
function contextKey() { try { return JSON.stringify({strategy:readStrategy(),symbol:selectedSymbol(),costs:settings(),fingerprint:dataContext && dataContext.fingerprint,calendar:dataContext && dataContext.calendar || null,timeframe:dataContext && dataContext.timeframe,normalization:dataContext && dataContext.normalization}); } catch { return ''; } }
function updateContext() {
  let costs; try { costs=settings(); } catch { announce('Correct the conversion-rate JSON before running.'); return; }
  const name=strategyModel ? strategyModel.name : 'Loading strategy';
  const coverage=dataContext ? `${dataContext.timeframe || ''} · ${dataContext.bars.toLocaleString()} bars · ${day(dataContext.first_time)} → ${day(dataContext.last_time)}` : !selectedSymbol() ? 'No market data loaded' : dataContextError || 'Loading data coverage';
  $('research-context').innerHTML=`<div><span>Current strategy</span><strong>${escapeHtml(name)}</strong></div><div><span>${selectedSymbol()?escapeHtml(dataContext ? dataContext.symbol : "Selected dataset")+' · full loaded dataset':'Market data'}</span><strong>${escapeHtml(coverage)}</strong></div><div><span>Simulation costs</span><strong>Spread ${costs.spread} FX pips · Slippage ${costs.slippage} FX pips · Commission ${costs.commission}/lot · ${escapeHtml(costs.account.currency)} · Max leverage ${costs.account.max_leverage}:1</strong></div><button class="secondary" data-go="settings">Edit assumptions</button>${!selectedSymbol()?'<button class="secondary" data-go="data">Import or load market data</button>':''}`;
  if (lastRun) $('run-summary').textContent=(lastRun.key===contextKey() ? 'Latest run matches the current draft and costs. ' : 'Stale result — strategy, symbol or costs have changed. Run again. ')+lastRun.label;
}
async function readNormalizationFiles(files) {
  const selected=Array.from(files || []);
  if (!selected.length) return null;
  const names={'manifest.json':'manifest_json','policy.json':'policy_json','raw.csv':'raw_csv','utc.csv':'utc_csv'};
  if (selected.length!==4 || new Set(selected.map(f=>f.name)).size!==4 || selected.some(f=>!Object.hasOwn(names,f.name))) throw new Error('Select exactly manifest.json, policy.json, raw.csv and utc.csv from one normalization folder.');
  if (selected.reduce((sum,f)=>sum+f.size,0)>20*1024*1024) throw new Error('Normalization files must total at most 20 MB for this importer.');
  const bundle={};
  for (const file of selected) bundle[names[file.name]]=await file.text();
  return bundle;
}
function normalizationSummary(value) {
  if (value?.status==='verified_transformation') return 'Clock transformation checked · '+value.verification.bundle_id+'\n'+value.verification.scope;
  if (value?.status==='invalid') return 'Invalid normalization evidence: '+value.error;
  return 'Clock evidence: not supplied. UTC timestamps are user-declared.';
}
refreshDataInfo = async function() {
  const symbol=selectedSymbol(), request=++dataContextRequest;
  dataContext=null;dataContextError='';updateContext();
  if(!symbol){
    $('data-info').textContent='No market data loaded';
    $('data-quality-output').textContent='Import a CSV or reload a saved dataset in Data before starting research.';
    return;
  }
  let d;
  try{d=await getJSON('/api/data?symbol='+encodeURIComponent(symbol));}
  catch(error){d={error:'Data coverage unavailable: '+error.message};}
  if(request!==dataContextRequest || symbol!==selectedSymbol()) return;
  dataContext=d.error ? null : d;dataContextError=d.error || '';
  const quality=d.data_quality;
  $('data-quality-output').textContent=d.error || (quality ? [normalizationSummary(d.normalization), 'Calendar: '+(quality.calendar || 'Not specified'), 'Bars: '+quality.bars, 'Missing open intervals: '+quality.missing_open_intervals, 'Scheduled closed intervals: '+quality.scheduled_closed_intervals, 'Unclassified intervals: '+quality.unclassified_intervals, 'Irregular steps: '+quality.irregular_steps, 'Incomplete candles: '+quality.incomplete_bars, ...quality.errors].join('\n') : 'Quality report unavailable');
  $('data-info').textContent=d.error || `${d.bars.toLocaleString()} bars`;
  updateContext();
};
getJSON = async function(url) { const r=await fetch(url); if (!r.ok) throw new Error(`Request failed (${r.status}). Please retry.`); return r.json(); };
// Keep navigation and local editing available during requests; prevent duplicate submissions only.
let pendingRequests=0, jobStarted=0, jobTimer=null;
const job=document.createElement('div'); job.id='job-status'; job.hidden=true; job.setAttribute('role','status'); document.body.append(job);
setBusy = function(on) {
  pendingRequests=Math.max(0,pendingRequests+(on?1:-1));
  job.hidden=pendingRequests===0;
  if (on && pendingRequests===1) {
    jobStarted=Date.now();
    const tick=()=>job.textContent=`${pendingRequests} request${pendingRequests===1?'':'s'} running · ${Math.floor((Date.now()-jobStarted)/1000)}s elapsed · You can keep browsing`;
    tick(); jobTimer=setInterval(tick,1000);
  } else if (!pendingRequests) clearInterval(jobTimer);
};
const inflight=new Set();
postJSON = async function(url,body) {
  const requestKey=url+'\n'+body;
  if (inflight.has(requestKey)) return {error:'This request is already running.'};
  const initiator=document.activeElement && document.activeElement.tagName==='BUTTON' ? document.activeElement : null;
  if (initiator) initiator.disabled=true;
  inflight.add(requestKey); setBusy(true);
  try {
    const r=await fetch(url,{method:'POST',headers:{'Content-Type':'application/json'},body});
    if (!r.ok) throw new Error(`Request failed (${r.status}).`);
    const data=await r.json(); if (data.error) announce(data.error); return data;
  } catch(e) { announce('Could not complete the request. '+e.message); return {error:e.message}; }
  finally { inflight.delete(requestKey); setBusy(false); if (initiator) initiator.disabled=false; }
};
const originalConditionRow=buildConditionRow;
buildConditionRow = function() {
  const row=originalConditionRow();
  const search=document.createElement('input'); search.type='search'; search.className='indicator-search'; search.placeholder='Find an indicator or price field'; search.setAttribute('aria-label','Search indicators for this condition');
  search.addEventListener('input',()=>{
    const select=row.querySelector('.left-type'); const current=select.value;
    select.innerHTML=leftOptions();
    Array.from(select.options).forEach(o=>{if (o.value!==current && !o.text.toLowerCase().includes(search.value.toLowerCase())) o.remove();});
    select.value=current;
  });
  row.prepend(search);
  const friendly={Gt:'Is greater than',Gte:'Is at least',Lt:'Is less than',Lte:'Is at most',Eq:'Equals',Neq:'Does not equal',CrossAbove:'Crosses above',CrossBelow:'Crosses below',Rising:'Is rising',Falling:'Is falling'};
  row.querySelectorAll('.cmp option').forEach(o=>o.textContent=friendly[o.value] || o.value);
  row.querySelector('.remove').textContent='Remove';
  row.querySelectorAll('select').forEach(el=>el.setAttribute('aria-label',el.className==='cmp'?'Comparison':el.className==='left-type'?'Indicator or price':'Compare with'));
  return row;
};
function labelControls() {
  document.querySelectorAll('input,select,textarea').forEach((el,i)=>{
    if (!el.id) { let n=i; while (document.getElementById('control-'+n)) n++; el.id='control-'+n; }
    if (el.closest('label') || document.querySelector(`label[for="${el.id}"]`) || el.hasAttribute('aria-label')) return;
    const prev=el.previousElementSibling;
    if (prev && prev.tagName==='LABEL') prev.htmlFor=el.id;
    else el.setAttribute('aria-label',el.dataset.name || el.placeholder || ({'symbol-select':'Market symbol','aw-mm-type':'Position sizing','prj-select':'Saved strategy'}[el.id]) || el.className || el.id);
  });
}
function downloadJson(name,data) { downloadRawJson(name,JSON.stringify(data,null,2)); }
function downloadRawJson(name,text) {
  const a=document.createElement('a'),url=URL.createObjectURL(new Blob([text],{type:'application/json'}));
  a.href=url;a.download=name;a.click();setTimeout(()=>URL.revokeObjectURL(url),1000);
}
async function rawBackup(url) {
  const response=await fetch(url);if(!response.ok)throw new Error('Backup download failed ('+response.status+').');
  const text=await response.text();const check=JSON.parse(text);if(check.error)throw new Error(check.error);
  // Preserve the wire representation: JS number reserialization changes hash inputs.
  return text;
}
function renderRunHistory() {
  $('run-history').replaceChildren();
  runHistory.slice().reverse().forEach(run=>{
    const card=document.createElement('div');card.className='run-card';
    const title=document.createElement('strong');title.textContent=run.strategy.name+' · '+run.created;
    const p=document.createElement('p');p.textContent=run.label;
    const download=document.createElement('button');download.className='secondary';download.textContent='Download run report';download.onclick=()=>downloadJson('sqxlabs-run-'+run.id+'.json',run);
    const restore=document.createElement('button');restore.className='secondary';restore.textContent='Restore inputs';restore.onclick=()=>{
      if (!Array.from($('symbol-select').options).some(o=>o.value===run.symbol)) {announce('Load '+run.symbol+' before restoring this run.');return;}
      setStrategy(run.strategy,'Restored run inputs'); $('symbol-select').value=run.symbol;
      ['deposit','spread','slippage','commission'].forEach(k=>$('set-'+k).value=run.costs[k]);
      $('set-intrabar').value=run.costs.intrabar_policy || 'adverse_first';$('set-compare-paths').checked=!!run.costs.compare_paths;$('set-allow-gaps').checked=!!run.costs.allow_data_gaps;
      if(run.costs.account){const a=run.costs.account;for(const [field,key] of Object.entries({currency:'currency',leverage:'max_leverage',stopout:'stop_out_ratio','min-lots':'min_lots','lot-step':'lot_step','max-lots':'max_lots','long-financing':'long_financing_per_lot','short-financing':'short_financing_per_lot'})) $('set-'+field).value=a[key];$('set-rates').value=JSON.stringify(a.conversion_rates,null,2);}
      refreshDataInfo();announce('Inputs restored. A new run uses the currently loaded data; the old report remains unchanged.');
    };
    card.append(title,p,download,document.createTextNode(' '),restore);$('run-history').append(card);
  });
}
function confirmationCoverageNotice(execution) {
  const skipped=execution?.skipped_leading_confirmation_bucket;
  if(skipped===null || skipped===undefined)return '';
  const date=new Date(skipped);
  const label=typeof skipped==='number' && Number.isFinite(skipped) && Number.isFinite(date.getTime()) ? date.toISOString() : 'unknown time';
  return 'Confirmation history starts inside a candle. The leading candle ('+label+') was excluded from confirmation and indicator warmup. Completeness of later candles is not established by this check.';
}

runBacktest = async function() {
  let strategy;
  try { strategy=readStrategy(); } catch(e) { announce('Correct the strategy before running: '+e.message);showView('code');return; }
  let costs;
  try {costs=settings();}catch(error){$('bt-status').textContent='Correct the simulation settings: '+error.message;return;}
  const symbol=selectedSymbol(),key=contextKey();
  if (!symbol) {announce('Import or select market data first.');return;}
  $('bt-status').textContent='Running simulation…';
  let d;
  try {d=await postJSON('/api/backtest',JSON.stringify(Object.assign(copy(strategy),costs,{symbol})));}
  catch(error){$('bt-status').textContent='The run response could not be received. The server may have completed and archived the run; refresh Saved research runs before retrying.';return;}
  if (d.error) {$('bt-status').textContent='Run failed: '+d.error;return;}
  const run={id:Date.now(),created:new Date().toISOString(),strategy:copy(strategy),costs:copy(costs),symbol,key,result:d,validation:'Not independently validated',label:`${d.symbol} · ${d.trades} trades · Spread ${costs.spread} FX pips · Slippage ${costs.slippage} FX pips · Commission ${costs.commission}/lot`};
  run.key=JSON.stringify({strategy,symbol,costs,fingerprint:d.run_context.data_fingerprint,calendar:d.run_context.calendar || null,timeframe:d.run_context.timeframe});
  runHistory.push(run);lastRun=run;
  $('bt-status').textContent='Simulation completed · research archived';updateContext();renderRunHistory();refreshResearchRuns();
  const m=d.metrics,metrics=[['Net profit',m.net_profit],['Profit factor',m.profit_factor],['Max drawdown %',m.max_drawdown_pct],['Trades',m.number_of_trades]];
  $('bt-results').innerHTML='<span class="status-pill">Development backtest · not independently validated</span><div class="metrics-grid">'+metrics.map(([name,v])=>`<div class="metric"><span>${name}</span><strong>${fmt(v,name==='Trades'?0:2)}</strong></div>`).join('')+'</div><h3>Equity</h3><div id="chart"></div><details><summary>All metrics</summary><div class="table-scroll"><table>'+Object.entries(m).map(([k,v])=>`<tr><th>${escapeHtml(k.replaceAll('_',' '))}</th><td>${typeof v==='number'?fmt(v,3):'—'}</td></tr>`).join('')+'</table></div></details><details><summary>Execution assumptions</summary><p>Bar simulation. Entries fill at the next open. Signal exits fill at the next open. Intrabar outcomes use the engine’s configured OHLC path assumption. These results do not guarantee live fills.</p><p>Costs and dataset details are recorded in the downloadable run report.</p></details><details><summary>Strategy rules</summary><pre>'+escapeHtml(d.pseudocode)+'</pre></details>';
  const coverageNotice=confirmationCoverageNotice(d.run_context?.execution);
  if(coverageNotice){const notice=document.createElement('p');notice.className='research-notice';notice.textContent=coverageNotice;$('bt-results').prepend(notice);}
  drawChart(d.equity);
  if (d.trade_details) renderTrades(d.trade_details,d.run_context?.execution);
  const entries=document.createElement('details');entries.innerHTML=entryAttemptsHtml(d.run_context?.execution);$('bt-results').append(entries);
  if(d.path_sensitivity && d.path_sensitivity.length){const scenarios=document.createElement('details');scenarios.innerHTML='<summary>Complete-run path sensitivity</summary><p>Each row re-runs the entire strategy under a different OHLC path assumption. This is not an exhaustive worst/best bound.</p><div class="table-scroll"><table><tr><th>Policy</th><th>Net profit</th><th>Trades</th><th>Drawdown %</th></tr>'+d.path_sensitivity.map(s=>'<tr><td>'+escapeHtml(s.policy)+'</td><td>'+fmt(s.metrics.net_profit,2)+'</td><td>'+s.trades+'</td><td>'+fmt(s.metrics.max_drawdown_pct,2)+'</td></tr>').join('')+'</table></div>';$('bt-results').append(scenarios);}
  if(d.ambiguous_bars && d.ambiguous_bars.length){const ambiguity=document.createElement('details');ambiguity.innerHTML='<summary>OHLC ambiguity · '+d.ambiguous_bars.length+' candles</summary><p>The high-first and low-first scenarios differ on these candles. Neither scenario proves the true tick path or a global worst/best bound. Chosen policy: '+escapeHtml(d.run_context.intrabar_policy)+'</p><pre>'+escapeHtml(JSON.stringify(d.ambiguous_bars,null,2))+'</pre>';$('bt-results').append(ambiguity);}
  if(d.rejected_orders && d.rejected_orders.length){const rejected=document.createElement('details');rejected.innerHTML='<summary>Rejected orders · '+d.rejected_orders.length+'</summary><pre>'+escapeHtml(JSON.stringify(d.rejected_orders,null,2))+'</pre>';$('bt-results').append(rejected);}
};
function signalEvidenceHtml(signal) {
  if(!signal)return '<p class="muted">No primary-signal operand trace is attached. A time-only exit has no signal operands; older runs may lack this evidence.</p>';
  const value=v=>typeof v==='number'&&Number.isFinite(v)?escapeHtml(String(v)):'Undefined / not recorded';
  const outcome=v=>v===true?'True':v===false?'False':'Not recorded';
  const operand=op=>{
    if(op&&Object.prototype.hasOwnProperty.call(op,'Const'))return 'Constant '+value(op.Const);
    if(Array.isArray(op?.Price))return escapeHtml(op.Price[0])+' (shift '+escapeHtml(op.Price[1])+')';
    if(Array.isArray(op?.Indicator))return escapeHtml(op.Indicator[0])+' ('+(op.Indicator[1]||[]).map(p=>escapeHtml(p.name)+'='+value(p.value)).join(', ')+')';
    return 'Operand not recorded';
  };
  const comparisons={Gt:'greater than',Gte:'at least',Lt:'less than',Lte:'at most',Eq:'equals',Neq:'differs from',CrossAbove:'crosses above',CrossBelow:'crosses below',Rising:'rising',Falling:'falling'};
  const rows=(signal.conditions||[]).map(e=>{
    const c=e.condition||{},unary=['Rising','Falling'].includes(c.cmp),previous=unary||['CrossAbove','CrossBelow'].includes(c.cmp);
    return '<tr><td>'+operand(c.left)+' '+escapeHtml(comparisons[c.cmp]||c.cmp)+(unary?'':' '+operand(c.right))+'</td><td>'+value(e.left)+'</td><td>'+(unary?'Not used':value(e.right))+'</td><td>'+(previous?value(e.previous_left):'Not used')+'</td><td>'+(previous&&!unary?value(e.previous_right):'Not used')+'</td><td>'+outcome(e.matched)+'</td></tr>';
  }).join('');
  const aggregation=signal.fuzzy!==null&&signal.fuzzy!==undefined?'Fuzzy threshold '+value(signal.fuzzy)+' of conditions (overrides AND/OR)':signal.join==='And'?'All conditions (AND)':signal.join==='Or'?'Any condition (OR)':'Combination not recorded';
  return '<details><summary>Signal values · '+escapeHtml(signal.kind)+' · '+outcome(signal.matched)+'</summary><p>Source candle index '+value(signal.bar_index)+'. '+aggregation+'. Values are frozen from this run; these are not current market quotes.</p><div class="table-scroll" tabindex="0"><table><thead><tr><th>Condition</th><th>Left</th><th>Right</th><th>Previous left</th><th>Previous right</th><th>Result</th></tr></thead><tbody>'+rows+'</tbody></table></div></details>';
}

function terminalEntryHtml(execution) {
  const entry=execution?.unexecuted_entry;
  if(entry===undefined)return '<p class="muted">Terminal entry evidence was not recorded.</p>';
  if(entry===null)return '<p class="muted">No entry signal remained queued at the dataset boundary.</p>';
  const time=typeof entry.signal_bar_open_time==='number'&&Number.isFinite(new Date(entry.signal_bar_open_time).getTime())?new Date(entry.signal_bar_open_time).toISOString():'Not recorded';
  return '<details><summary>Unexecuted final entry · '+escapeHtml(entry.direction)+'</summary><p>Signal candle '+escapeHtml(entry.signal_bar_index)+' opened '+escapeHtml(time)+'. No following opening quote was available. This signal was not sized, checked for margin or filled. Reason: '+escapeHtml(entry.reason)+'.</p>'+signalEvidenceHtml(entry.signal)+'</details>';
}

function entryAttemptsHtml(execution) {
  const attempts=execution?.entry_attempts,terminal=terminalEntryHtml(execution);
  if(!Array.isArray(attempts))return '<summary>Entry decisions · evidence unavailable</summary><p>This run does not contain entry-attempt evidence. Missing evidence does not mean no signals were blocked.</p>'+terminal;
  if(!attempts.length)return '<summary>Entry decisions · no attempts</summary><p>No queued entry signal reached an opening execution check. This is not a count of every indicator condition. Any final queued entry is shown separately below.</p>'+terminal;
  const labels={nonpositive_balance:'Balance is not positive',atr_not_ready:'ATR is not yet ready or positive',session_closed:'Trading session is closed',outside_trading_hours:'Outside permitted trading hours',daily_entry_limit:'Daily entry limit reached',confirmation_not_satisfied:'Confirmation is unavailable or not satisfied',invalid_volume:'Requested size is outside lot constraints',insufficient_margin:'Insufficient funds for margin and entry costs'};
  const date=value=>typeof value==='number'&&Number.isFinite(new Date(value).getTime())?escapeHtml(new Date(value).toISOString()):'Not recorded';
  const number=value=>typeof value==='number'&&Number.isFinite(value)?escapeHtml(String(value)):'Not reached / not recorded';
  const margin=a=>{
    if(a.margin_check===undefined)return '<p class="muted">Margin calculation was not recorded in this run.</p>';
    if(a.margin_check===null)return '<p class="muted">Margin admission was not reached; earlier gates or lot constraints prevented it.</p>';
    const m=a.margin_check;
    const fields=[['Balance',m.balance],['Normalized lots',m.normalized_lots],['Proposed fill price',m.proposed_fill_price],['Quote-to-account rate',m.quote_to_account_rate],['Margin base-to-account rate',m.margin_base_to_account_rate],['Maximum leverage',m.max_leverage],['Required margin',m.required_margin],['Spread and entry slippage reserve',m.spread_and_slippage_cost],['Commission reserve',m.commission],['Equity after entry costs',m.equity_after_entry_costs],['Free margin after entry',m.free_margin_after_entry]];
    return '<details><summary>Margin admission · '+(m.accepted===true?'Accepted':m.accepted===false?'Rejected':'Outcome not recorded')+'</summary><p>Money amounts use the account currency. This is an entry admission estimate, not a cash posting or a record of later margin levels. The proposed price does not prove an order filled.</p><dl>'+fields.map(([label,value])=>'<dt>'+label+'</dt><dd>'+number(value)+'</dd>').join('')+'</dl></details>';
  };
  const protection=a=>{
    if(a.initial_protection===undefined)return '<p class="muted">Initial protection was not recorded in this run.</p>';
    if(a.initial_protection===null)return a.trade_id!==null&&a.trade_id!==undefined?'<p class="error">Filled trade lacks its initial protection record.</p>':'<p>No initial protection was installed: this entry was not filled.</p>';
    if(a.trade_id===null||a.trade_id===undefined)return '<p class="error">Initial protection has no matching filled trade.</p>';
    const p=a.initial_protection,level=value=>value===null?'Not configured':number(value),enabled=value=>value===true?'Enabled':value===false?'Disabled':'Not recorded';
    return '<details><summary>Protection at entry</summary><p>These levels were installed after entry admission. A missing level is not a zero-price order. Later effective stop changes appear in the trade explanation.</p><dl><dt>Initial stop loss</dt><dd>'+level(p.stop_loss)+'</dd><dt>Initial profit target</dt><dd>'+level(p.profit_target)+'</dd><dt>Trailing stop</dt><dd>'+enabled(p.trailing_enabled)+'</dd><dt>Break-even rule</dt><dd>'+enabled(p.break_even_enabled)+'</dd><dt>Dynamic price distance</dt><dd>'+(p.dynamic_distance===null?'No finite active distance':number(p.dynamic_distance))+'</dd><dt>Reference candle index</dt><dd>'+number(p.reference_bar_index)+'</dd><dt>Reference candle opened UTC</dt><dd>'+date(p.reference_bar_open_time)+'</dd></dl><p class="muted">The reference is the preceding completed candle used for ATR-based levels. This records installed model levels, not broker order acknowledgements.</p></details>';
  };
  const confirmation=a=>{
    if(a.confirmation===undefined)return '<p class="muted">Confirmation evidence not recorded.</p>';
    if(a.confirmation===null)return '<p class="muted">No confirmation filter was configured.</p>';
    const c=a.confirmation;
    if(c.source_bar_open_time==null)return '<p>Confirmation '+escapeHtml(c.timeframe)+': no closed source candle was available.</p>';
    return '<details><summary>Confirmation · '+escapeHtml(c.timeframe)+'</summary><p>Source candle opened '+date(c.source_bar_open_time)+'. Available from '+date(c.available_from)+'. The engine selected the most recent closed bucket at the execution candle opening; this does not certify internal data coverage.</p>'+signalEvidenceHtml(c.signal)+'</details>';
  };
  const filled=attempts.filter(a=>a.trade_id!==null&&a.trade_id!==undefined).length,blocked=attempts.filter(a=>(a.blocked_by||[]).length>0).length;
  const rows=attempts.map(a=>{
    const reasons=a.blocked_by||[],hasTrade=a.trade_id!==null&&a.trade_id!==undefined;
    const outcome=reasons.length?reasons.map(r=>escapeHtml(labels[r]||r)).join('<br>'):hasTrade?'Opened trade '+number(a.trade_id):'Outcome not recorded';
    return '<tr><td>'+number(a.signal_bar_index)+'</td><td>'+date(a.signal_bar_open_time)+'</td><td>'+date(a.attempt_time)+'</td><td>'+escapeHtml(a.direction)+'</td><td>'+outcome+'</td><td>'+number(a.requested_lots)+'</td><td>'+number(a.filled_lots)+'</td><td>'+number(a.fill_price)+'</td></tr><tr><td colspan="8" style="text-align:left"><details style="max-width:min(860px,calc(100vw - 90px))"><summary>Why this entry decision? · Candle '+number(a.signal_bar_index)+'</summary>'+signalEvidenceHtml(a.signal)+confirmation(a)+margin(a)+protection(a)+'</details></td></tr>';
  }).join('');
  return '<summary>Entry decisions · '+attempts.length+' attempts · '+filled+' filled · '+blocked+' blocked</summary><p>The source timestamp labels the signal candle opening. Its completed values are evaluated for execution at the following opening quote. A blocked check is not a trade; sizing may not have been reached.</p><p class="muted">This covers queued entry attempts. Signals while a position is already open are not included; a final queued entry without a next quote is shown separately. Expand Signal values to inspect recorded primary conditions.</p><div class="table-scroll" tabindex="0"><table><thead><tr><th>Signal candle</th><th>Source candle UTC</th><th>Attempt UTC</th><th>Direction</th><th>Outcome / reasons</th><th>Requested lots</th><th>Filled lots</th><th>Fill price</th></tr></thead><tbody>'+rows+'</tbody></table></div>'+terminal;
}

function stopHistoryHtml(tradeId,execution) {
  const records=execution?.stop_changes;
  if(!Array.isArray(records))return '<p class="muted">Stop-change evidence was not recorded in this run.</p>';
  const changes=records.filter(row=>row.trade_id===tradeId);
  if(!changes.length)return '<p>No effective stop changes were recorded for this trade.</p>';
  const number=value=>typeof value==='number'&&Number.isFinite(value)?escapeHtml(String(value)):'Not recorded';
  const stop=value=>value===null?'No finite stop':number(value);
  const time=value=>typeof value==='number'&&Number.isFinite(new Date(value).getTime())?escapeHtml(new Date(value).toISOString()):'Not recorded';
  const source=row=>row.source==='observed_tick'?'Observed tick':row.source==='ohlc_high_first'?'Modelled OHLC · high first · step '+number(row.path_step):row.source==='ohlc_low_first'?'Modelled OHLC · low first · step '+number(row.path_step):'Unknown source: '+escapeHtml(row.source);
  return '<details><summary>Stop history · '+changes.length+' changes</summary><p>Only effective changes on the selected execution path are shown. OHLC timestamps label candles; path steps show model order, not exact intrabar times. Tick timestamps identify supplied observations. This is not a complete broker order log.</p><div class="table-scroll" tabindex="0"><table><thead><tr><th>Recorded UTC</th><th>Price source</th><th>Reached price</th><th>Previous stop</th><th>New stop</th><th>Previous reason</th><th>New reason</th></tr></thead><tbody>'+changes.map(row=>'<tr><td>'+time(row.time)+'</td><td>'+source(row)+'</td><td>'+number(row.observed_price)+'</td><td>'+stop(row.previous_stop)+'</td><td>'+stop(row.new_stop)+'</td><td>'+escapeHtml(row.previous_reason)+'</td><td>'+escapeHtml(row.new_reason)+'</td></tr>').join('')+'</tbody></table></div></details>';
}

function stopOutEvidenceHtml(trade,execution) {
  const records=execution?.stop_out_decisions;
  if(!Array.isArray(records))return trade.exit_reason==='StopOut'?'<p class="muted">Stop-out calculation was not recorded in this run.</p>':'';
  const matches=records.filter(row=>row.trade_id===trade.id);
  if(!matches.length&&trade.exit_reason!=='StopOut')return '';
  if(matches.length!==1||trade.exit_reason!=='StopOut')return '<p class="error">Stop-out evidence does not match the recorded exit. Inspect the archived run.</p>';
  const row=matches[0],number=value=>typeof value==='number'&&Number.isFinite(value)?escapeHtml(String(value)):'Not recorded';
  const time=typeof row.time==='number'&&Number.isFinite(new Date(row.time).getTime())?escapeHtml(new Date(row.time).toISOString()):'Not recorded';
  const fields=[['Balance',row.balance],['Unrealized profit before exit slippage',row.unrealized_profit_account],['Commission reserve',row.commission_reserve],['Accrued financing',row.financing_accrued],['Equity before exit slippage',row.equity_before_slippage],['Required margin',row.required_margin],['Stop-out ratio',row.stop_out_ratio],['Minimum equity (margin × ratio)',row.minimum_equity],['Calculated threshold price',row.threshold_price],['Liquidation price before slippage',row.liquidation_price_before_slippage],['Actual model fill price',row.fill_price],['Quote-to-account conversion rate',row.quote_to_account_rate],['Margin base-to-account rate',row.margin_base_to_account_rate]];
  return '<details><summary>Why was margin insufficient?</summary><p>Recorded time: '+time+'. Money amounts use '+escapeHtml(execution?.account_currency||'the account currency (not recorded)')+'. The model closes the position when equity reaches the required minimum.</p><dl>'+fields.map(([label,value])=>'<dt>'+label+'</dt><dd>'+number(value)+'</dd>').join('')+'</dl><p class="muted">A continuous OHLC path can close at a calculated threshold. A gap or observed tick can pass that threshold, and exit slippage can worsen the fill. Candle timestamps are not exact intrabar times. These are model calculations, not a broker margin confirmation.</p></details>';
}

function financingEvidenceHtml(tradeId,execution) {
  const records=execution?.financing_accruals;
  if(!Array.isArray(records))return '<p class="muted">Financing accrual evidence was not recorded in this run.</p>';
  const events=records.filter(row=>row.trade_id===tradeId);
  if(!events.length)return '<p>No nonzero financing accruals were recorded for this trade.</p>';
  const number=value=>typeof value==='number'&&Number.isFinite(value)?escapeHtml(String(value)):'Not recorded';
  const timestamp=value=>typeof value==='number'&&Number.isFinite(new Date(value).getTime())?escapeHtml(new Date(value).toISOString()):'Not recorded';
  const day=value=>Number.isInteger(value)?timestamp(value*86400000):'Not recorded';
  return '<details><summary>Financing history · '+events.length+' accruals</summary><p>Amounts use '+escapeHtml(execution?.account_currency||'the account currency (not recorded)')+'. The rollover day and the quote time when the cost was recognized may differ after a data gap. These are accrued charges, settled at trade close; negative values are credits.</p><div class="table-scroll" tabindex="0"><table><thead><tr><th>Rollover UTC day</th><th>Recognized at UTC</th><th>Lots</th><th>Daily charge / lot</th><th>Day multiplier</th><th>Charge / credit</th><th>Accrued before</th><th>Accrued after</th></tr></thead><tbody>'+events.map(row=>'<tr><td>'+day(row.rollover_utc_day)+'</td><td>'+timestamp(row.applied_at)+'</td><td>'+number(row.lots)+'</td><td>'+number(row.daily_charge_per_lot)+'</td><td>'+number(row.multiplier)+'</td><td>'+number(row.charge)+'</td><td>'+number(row.accrued_before)+'</td><td>'+number(row.accrued_after)+'</td></tr>').join('')+'</tbody></table></div></details>';
}

function cashSettlementHtml(tradeId,execution) {
  const records=execution?.cash_settlements;
  if(!Array.isArray(records))return '<p class="muted">Cash settlement evidence was not recorded in this run.</p>';
  const matches=records.filter(row=>row.trade_id===tradeId);
  if(matches.length!==1)return '<p class="error">Cash settlement cannot be explained: expected one record for this trade, found '+matches.length+'.</p>';
  const row=matches[0],number=value=>typeof value==='number'&&Number.isFinite(value)?escapeHtml(String(value)):'Not recorded';
  const account=escapeHtml(row.account_currency||'Account currency not recorded'),quote=escapeHtml(row.quote_currency||'Quote currency not recorded');
  const fields=[['Balance before ('+account+')',row.balance_before],['Gross profit ('+quote+')',row.gross_profit_quote],['Quote-to-account conversion rate',row.quote_to_account_rate],['Gross profit ('+account+')',row.gross_profit_account],['Commission charged ('+account+')',row.commission],['Financing charge / credit ('+account+')',row.financing],['Net balance change ('+account+')',row.net_change],['Balance after ('+account+')',row.balance_after]];
  return '<details><summary>How did this trade change the balance?</summary><p>Gross profit minus commission and financing gives the net balance change. Negative financing is a credit. Spread and slippage are already reflected in fill prices; do not subtract them again.</p><dl>'+fields.map(([label,value])=>'<dt>'+label+'</dt><dd>'+number(value)+'</dd>').join('')+'</dl><p class="muted">This model books these costs at trade close. Recorded arithmetic is not independent broker verification.</p></details>';
}

function accountSnapshotsHtml(execution,page=0) {
  const snapshots=execution?.account_snapshots;
  if(!Array.isArray(snapshots))return '<summary>Account observations</summary><p>Account observations were not recorded in this run.</p>';
  if(!snapshots.length)return '<summary>Account observations · none</summary><p>No account observations are available.</p>';
  const pages=Math.ceil(snapshots.length/25),current=Math.max(0,Math.min(pages-1,Number.isInteger(page)?page:0)),start=current*25;
  const number=value=>typeof value==='number'&&Number.isFinite(value)?escapeHtml(String(value)):'Not recorded';
  const time=value=>typeof value==='number'&&Number.isFinite(new Date(value).getTime())?escapeHtml(new Date(value).toISOString()):'Not recorded';
  const rows=snapshots.slice(start,start+25).map((row,index)=>{
    const phase=row.phase==='bar_close'?'After candle':row.phase==='terminal_liquidation'?'After final liquidation':'Unknown phase';
    const position=row.trade_id===null?'Flat':row.trade_id===undefined?'Not recorded':'Trade '+number(row.trade_id);
    const fields=[['Liquidation price',row.liquidation_price],['Quote-to-account rate',row.quote_to_account_rate],['Margin base-to-account rate',row.margin_base_to_account_rate],['Gross unrealized profit',row.gross_unrealized],['Commission reserve',row.commission_reserve],['Accrued financing',row.financing_accrued]];
    return '<tr><td>'+time(row.bar_open_time)+'</td><td>'+phase+' · '+position+'</td><td>'+number(row.balance)+'</td><td>'+number(row.equity)+'</td><td>'+number(row.required_margin)+'</td><td>'+number(row.free_margin)+'</td></tr><tr><td colspan="6" style="text-align:left"><details style="max-width:min(860px,calc(100vw - 90px))"><summary>Account calculation · observation '+(start+index+1)+'</summary><p>Observed tick UTC: '+(row.quote_time===null?'No tick timestamp; candle-based observation':time(row.quote_time))+'.</p><dl>'+fields.map(([label,value])=>'<dt>'+label+'</dt><dd>'+(value===null?'Not applicable':number(value))+'</dd>').join('')+'</dl></details></td></tr>';
  }).join('');
  return '<summary>Account observations · '+snapshots.length+' samples</summary><p>Amounts use '+escapeHtml(execution.account_currency||'the account currency (not recorded)')+'. Equity includes open profit minus commission and financing reserves; free margin is equity minus required margin. Samples are recorded after each candle and any final liquidation, not at every intrabar price.</p><p class="muted">Candle UTC labels the candle opening. An observed tick time, when available, is shown in the calculation. The final pre-liquidation mark can differ from the settled ending balance.</p><p role="status">Observations '+(start+1)+'–'+Math.min(start+25,snapshots.length)+' of '+snapshots.length+'</p><button type="button" data-account-page="'+(current-1)+'" '+(current===0?'disabled':'')+'>Previous observations</button> <button type="button" data-account-page="'+(current+1)+'" '+(current===pages-1?'disabled':'')+'>Next observations</button><div class="table-scroll" tabindex="0"><table><thead><tr><th>Candle UTC</th><th>Phase / position</th><th>Balance</th><th>Equity</th><th>Required margin</th><th>Free margin</th></tr></thead><tbody>'+rows+'</tbody></table></div>';
}

function tradeLedgerHtml(trades,execution) {
  const timestamp=value=>typeof value==='number'&&Number.isFinite(new Date(value).getTime())?escapeHtml(new Date(value).toISOString()):'Not recorded';
  const number=value=>typeof value==='number'&&Number.isFinite(value)?escapeHtml(String(value)):'Not recorded';
  const evidence=execution?.scheduled_exit_decisions;
  const rows=trades.map(t=>{
    const decisions=Array.isArray(evidence)?evidence.filter(e=>e.trade_id===t.id):null;
    let explanation;
    if(decisions===null)explanation='Close-derived exit evidence was not recorded in this run.';
    else if(!decisions.length)explanation='No close-derived signal/time exit request is recorded for this trade. The recorded exit reason is '+escapeHtml(t.exit_reason)+'.';
    else explanation=decisions.map(e=>'<p>Decision source: candle '+number(e.decision_bar_index)+' opened '+timestamp(e.decision_bar_open_time)+'. The decision uses its completed values; this timestamp is not the decision instant.</p><p>Triggered rules: '+(e.reasons||[]).map(escapeHtml).join(', ')+'. Reporting reason: '+escapeHtml(e.chosen_reason)+'.</p><p>'+(e.executed_time==null?'Not executed: no following executable opening quote was recorded.':'Executed at '+timestamp(e.executed_time)+' on the next executable opening quote.')+'</p>'+signalEvidenceHtml(e.signal)).join('');
    if(t.exit_reason==='EndOfData')explanation+='<p>End-of-data liquidation is a reporting convention, not an executed live exit signal.</p>';
    return '<tr><td>'+number(t.id)+'</td><td>'+escapeHtml(t.direction)+'</td><td>'+timestamp(t.open_time)+'</td><td>'+timestamp(t.close_time)+'</td><td>'+number(t.size)+'</td><td>'+number(t.open_price)+'</td><td>'+number(t.close_price)+'</td><td>'+number(t.pnl)+'</td><td>'+number(t.commission)+'</td><td>'+number(t.financing)+'</td><td>'+escapeHtml(t.exit_reason)+'</td></tr><tr><td colspan="11" style="text-align:left"><details style="max-width:min(860px,calc(100vw - 90px))"><summary>Why this exit? · Trade '+number(t.id)+'</summary>'+explanation+stopHistoryHtml(t.id,execution)+stopOutEvidenceHtml(t,execution)+cashSettlementHtml(t.id,execution)+financingEvidenceHtml(t.id,execution)+'</details></td></tr>';
  }).join('');
  return '<summary>Trade ledger · '+trades.length+' trades</summary><p>Net P&amp;L includes commission and financing; money amounts use '+escapeHtml(execution?.account_currency||'the account currency (not recorded)')+'. Bar-mode timestamps label candles and do not establish exact intrabar fill times. Tick-mode execution uses observed quote timestamps.</p><p class="muted">These explanations cover queued signal/time exits. Primary signal values are shown where recorded; confirmation evidence appears under Entry decisions. A complete intrabar order lifecycle is not yet available.</p><div class="table-scroll" tabindex="0"><table><thead><tr><th>ID</th><th>Direction</th><th>Entry UTC</th><th>Exit UTC</th><th>Lots</th><th>Entry price</th><th>Exit price</th><th>Net P&amp;L</th><th>Commission</th><th>Financing</th><th>Exit reason</th></tr></thead><tbody>'+rows+'</tbody></table></div>';
}
function renderTrades(trades,execution) {
  const detail=document.createElement('details');detail.innerHTML=tradeLedgerHtml(trades,execution);$('bt-results').append(detail);
  const account=document.createElement('details');account.innerHTML=accountSnapshotsHtml(execution);$('bt-results').append(account);
  account.addEventListener('click',event=>{
    const button=event.target.closest('button[data-account-page]');if(!button||button.disabled)return;
    const direction=button.textContent;account.innerHTML=accountSnapshotsHtml(execution,Number(button.dataset.accountPage));
    const buttons=Array.from(account.querySelectorAll('button[data-account-page]'));
    (buttons.find(b=>!b.disabled&&b.textContent===direction)||buttons.find(b=>!b.disabled))?.focus();
  });
}
// The shared chart has labels and accessible values; no interpolated timestamps.
drawChart = function(equity,id='chart') {
  const box=$(id);if (!box || !equity || equity.length<2) return;
  let min=Infinity,max=-Infinity; for(const point of equity){min=Math.min(min,point[1]);max=Math.max(max,point[1]);}
  const range=max-min || 1,w=760,h=220;
  const points=equity.map((p,i)=>`${i?'L':'M'}${60+i/(equity.length-1)*(w-80)},${15+(max-p[1])/range*(h-45)}`).join(' ');
  box.innerHTML=`<svg role="img" aria-label="Equity from ${escapeHtml(day(equity[0][0]))} to ${escapeHtml(day(equity.at(-1)[0]))}; minimum ${min.toFixed(2)}, maximum ${max.toFixed(2)}" viewBox="0 0 ${w} ${h}" width="100%"><text x="0" y="20" fill="currentColor" font-size="11">${max.toFixed(0)}</text><text x="0" y="${h-30}" fill="currentColor" font-size="11">${min.toFixed(0)}</text><path d="${points}" fill="none" stroke="var(--acc)" stroke-width="2"/><text x="60" y="${h-5}" fill="currentColor" font-size="11">${day(equity[0][0])}</text><text x="${w-100}" y="${h-5}" fill="currentColor" font-size="11">${day(equity.at(-1)[0])}</text></svg>`;
};
function initializeWorkspace() {
  const names={LongEntry:'Enter a long position',ShortEntry:'Enter a short position',LongExit:'Exit a long position',ShortExit:'Exit a short position'};
  Object.entries(names).forEach(([kind,label])=>$('sig-'+kind).querySelector('h3').textContent=label);
  setStrategy(JSON.parse($('strategy').value),'Example strategy',false);
  $('visual-editor').addEventListener('input',e=>{if(e.target.matches('input:not([type=search])') && e.target.value){try{commitVisual(false);}catch(err){announce(err.message);}}});
  $('visual-editor').addEventListener('change',()=>{try {commitVisual();} catch(e){announce(e.message);}});
  $('visual-editor').addEventListener('click',e=>{if(e.target.closest('.remove,.add-cond'))commitVisual();});
  $('strategy').addEventListener('input',()=>{
    try {const raw=$('strategy').value, cursor=$('strategy').selectionStart;setStrategy(readStrategy());$('strategy').value=raw;$('strategy').setSelectionRange(cursor,cursor);announce('');} catch(e){$('view-visual').disabled=true;$('draft-status').textContent='Invalid strategy JSON';announce(e.message);}
  });
  $('view-code').onclick=()=>showView('code');$('view-visual').onclick=()=>showView('visual');
  $('strategy-undo').onclick=()=>{if(undoHistory.length)setStrategy(undoHistory.pop(),'Undid edit',false);};
  $('btn-backtest').onclick=runBacktest;
  document.querySelectorAll('#tab-settings input,#tab-settings select,#set-rates').forEach(el=>el.addEventListener('input',updateContext));
  $('symbol-select').addEventListener('change',updateContext);
  $('catalog-search').oninput=()=>{const q=$('catalog-search').value.toLowerCase();document.querySelectorAll('#catalog .chip').forEach(el=>el.hidden=!el.textContent.toLowerCase().includes(q));};
  labelControls();
}
// Credentials belong to the selected profile, including unsaved inputs.
const originalSelectProfile=selectProfile;
selectProfile=function(label) {
  const id=profileId(label);
  if (activeProfile!==null && activeProfile!==id) {$('ai-key').value='';$('ai-save-status').textContent='Provider changed; unsaved key cleared.';}
  activeProfile=id;originalSelectProfile(label);
};
// File selection is a local preview; import remains an explicit action.
$('dm-file').onchange=async function() {
  const file=this.files[0];if (!file)return;
  if(file.size>20*1024*1024){announce('Choose a CSV smaller than 20 MB for the browser importer.');return;}
  const text=await file.text();$('dm-csv').value=text;
  const lines=text.trim().split(/\r?\n/);$('csv-preview').hidden=false;$('csv-preview').textContent=`${file.name} · ${lines.length.toLocaleString()} lines\nPreview only — confirm the timestamp format and OHLCV columns before importing.\n\n`+lines.slice(0,6).join('\n');
};
document.addEventListener('click',e=>{
  const go=e.target.closest('[data-go]');if(go)document.querySelector(`.nav-btn[data-tab="${go.dataset.go}"]`).click();
  const prompt=e.target.closest('[data-prompt]');if(prompt)$('ai-q').value=prompt.dataset.prompt+'\nStrategy: '+$('strategy').value;
});
window.addEventListener('unhandledrejection',e=>{announce('An operation could not finish. '+(e.reason.message || e.reason));});
loadInfo();loadCatalog();refreshHome();

// Selection follows strategy content rather than row position or sort order.
const selectedStrategies=new Map();
const originalRenderResults=renderResults;
renderResults=function() {
  const available=new Set(RESULTS.map(row=>JSON.stringify(row.strategy)));
  for(const key of selectedStrategies.keys()) if(!available.has(key)) selectedStrategies.delete(key);
  originalRenderResults();
  $('res-overview').textContent='Select candidates to compare or combine. Validation evidence is not recorded for historical rows.';
  $('res-portfolio').textContent=`Build portfolio (${selectedStrategies.size} selected)`;
  document.querySelectorAll('#res-grid tr').forEach((row,index)=>{
    const cell=document.createElement(index?'td':'th');
    if (!index) cell.textContent='Select';
    else {
      const open=row.querySelector('[data-ri]');if (!open)return;
      const item=RESULTS[Number(open.dataset.ri)],key=JSON.stringify(item.strategy);
      const input=document.createElement('input');input.type='checkbox';input.checked=selectedStrategies.has(key);input.setAttribute('aria-label','Select '+item.name);
      input.onchange=()=>{if(input.checked)selectedStrategies.set(key,copy(item));else selectedStrategies.delete(key);$('res-portfolio').textContent=`Build portfolio (${selectedStrategies.size} selected)`;};cell.append(input);
    }
    row.prepend(cell);
  });
};
$('compare-selected').onclick=()=>{
  const rows=Array.from(selectedStrategies.values());
  if(rows.length<2 || rows.length>5){announce('Select 2 to 5 strategies for a side-by-side comparison.');return;}
  const metrics=[['Net profit','net_profit'],['Profit factor','profit_factor'],['Trades','trades'],['Drawdown %','dd_pct']];
  $('comparison').innerHTML='<div class="table-scroll"><table><thead><tr><th>Recorded metric</th>'+rows.map(r=>'<th>'+escapeHtml(r.name)+'</th>').join('')+'</tr></thead><tbody>'+metrics.map(([label,key])=>'<tr><th>'+label+'</th>'+rows.map(r=>'<td>'+fmt(r[key],key==='trades'?0:2)+'</td>').join('')+'</tr>').join('')+'<tr><th>Independent validation</th>'+rows.map(()=>'<td>Not recorded</td>').join('')+'</tr></tbody></table></div>';
};
let selectedPortfolioPending=false;
const selectedPortfolioKey='sqx.pendingSelectedPortfolio.v1';
$('res-portfolio-new').onclick=function(){
  if(selectedPortfolioPending)return;
  localStorage.removeItem(selectedPortfolioKey);this.hidden=true;
  $('res-portfolio-out').textContent='The next build will use the current selection and settings. This does not cancel earlier work; inspect Builder → Saved research for its status.';
};
async function buildSelectedPortfolio(){
  if(selectedPortfolioPending)return;
  const out=$('res-portfolio-out'),button=$('res-portfolio'),fresh=$('res-portfolio-new');
  selectedPortfolioPending=true;button.disabled=true;
  try{
    let request;const pending=localStorage.getItem(selectedPortfolioKey);
    if(pending){request=JSON.parse(pending);if(!request||typeof request.request_key!=='string'||!Array.isArray(request.strategies))throw Error('Stored selection request is invalid. Inspect Saved research before starting a new study.');}
    else{
      const rows=Array.from(selectedStrategies.values()),symbol=selectedSymbol();
      if(rows.length<2||rows.length>20)throw Error('Select 2 to 20 strategies to build a portfolio.');
      if(!symbol)throw Error('Load and select market data before building a portfolio.');
      request=Object.assign({symbol,strategies:rows.map(row=>row.strategy),selection_context:rows,request_key:crypto.randomUUID()},settings());
      localStorage.setItem(selectedPortfolioKey,JSON.stringify(request));
    }
    const costs=request;fresh.hidden=false;out.textContent=pending?'Retrying the saved selection and costs…':'Evaluating and saving the selected strategies…';
    let d;try{d=await postJSON('/api/portfolio/databank',JSON.stringify(request));}catch(error){out.textContent='The response could not be received. Retry to locate the same selection, or inspect Builder → Saved research before starting another study.';return;}
    const id=d.archive&&d.archive.started_run_id;
    const saved=id?'<p>Saved portfolio study: '+escapeHtml(id)+'. Open Builder → Saved research to inspect, back up or resume. Repeatability does not establish independent correctness.</p>':'';
    if(id){localStorage.removeItem(selectedPortfolioKey);fresh.hidden=true;}
    if(d.recovery_required){out.innerHTML=saved+'<p>'+escapeHtml(d.message||'This study already exists.')+'</p>';return;}
    if(d.error){out.innerHTML=saved+'<p>'+escapeHtml(d.error)+'</p>';return;}
    const symbol=d.symbol||'Selected market';
  out.innerHTML=saved+'<h3>Equal-weight portfolio · '+escapeHtml(symbol)+'</h3><p>'+escapeHtml(d.components.join(', '))+'</p><p>Net '+fmt(d.metrics.net_profit,2)+' · Drawdown '+fmt(d.metrics.max_drawdown_pct,2)+'% · Spread '+costs.spread+' FX pips · Slippage '+costs.slippage+' FX pips · Commission '+costs.commission+'/lot</p><div id="res-portfolio-chart"></div><details><summary>Return correlation</summary><p>Undefined values mean insufficient variation or an invalid return denominator; they are not evidence of diversification.</p><div class="table-scroll"><table><tr><th>Strategy</th>'+d.components.map(n=>'<th>'+escapeHtml(n)+'</th>').join('')+'</tr>'+d.correlation.map((row,i)=>'<tr><th>'+escapeHtml(d.components[i])+'</th>'+row.map(v=>'<td>'+fmt(v,2)+'</td>').join('')+'</tr>').join('')+'</table></div></details>';
  if(d.no_trade_components&&d.no_trade_components.length)out.innerHTML+='<p>No trades in this period for: '+escapeHtml(d.no_trade_components.join(', '))+'. These components remain included at their assigned weights.</p>';
  const notice=document.createElement('p');notice.textContent=researchNotice(d.research_context);out.prepend(notice);
  drawChart(d.equity,'res-portfolio-chart');

  }catch(error){out.textContent=error.message||String(error);fresh.hidden=false;}
  finally{selectedPortfolioPending=false;button.disabled=false;}
}
$('res-portfolio').onclick=buildSelectedPortfolio;


async function refreshResearchRuns() {
  try {
    const response=await getJSON('/api/research/runs');
    if(response.error)throw new Error(response.error);
    $('research-runs').replaceChildren();
    if(!response.runs.length){$('research-runs').textContent='No saved research yet. Run a backtest to create an archived result.';return;}
    const pipelineRoots=new Set(response.runs.filter(run=>run.kind==='cli_pipeline_started').map(run=>run.run_id));
    for(const run of response.runs){
      if(['cli_pipeline_event','cli_pipeline'].includes(run.kind)&&pipelineRoots.has(run.started_run_id))continue;
      const card=document.createElement('div');card.className='run-card';
      const name=document.createElement('strong');name.textContent=run.name+' · '+run.symbol;
      const mcpGeneration=['mcp_generation','mcp_generation_started','mcp_generation_trial','mcp_generation_trial_started'].includes(run.kind);
      const cliPipeline=['cli_pipeline_started','cli_pipeline_event','cli_pipeline'].includes(run.kind);
      const researchNoun=run.workflow_kind==='portfolio'?'portfolio study':run.workflow_kind==='backtest'?'backtest':run.workflow_kind==='generation'?'search':run.workflow_kind==='walk_forward'?'walk-forward study':run.workflow_kind==='robustness'?'robustness study':run.workflow_kind==='monte_carlo'?'Monte Carlo study':'pipeline';
      const researchTitle=researchNoun[0].toUpperCase()+researchNoun.slice(1);
      const pipelineLabel=researchTitle+(run.kind==='cli_pipeline_started'?' input checkpoint':run.kind==='cli_pipeline_event'?' step record':' completion record');
      const metadata=document.createElement('p');metadata.textContent='Run '+run.run_id.slice(0,12)+' · Dataset '+run.dataset_id.slice(0,12)+' · '+(cliPipeline?pipelineLabel:mcpGeneration?'MCP generation evidence ('+run.kind+')':run.kind==='cli_backtest_started'?'CLI input checkpoint':run.kind==='cli_backtest'?'CLI execution record':run.kind==='assistant_trial_started'?'AI trial input checkpoint':run.kind==='assistant_trial'?'AI trial execution':run.kind==='assistant_candidates'?'AI session evidence':run.kind==='pipeline'?'Pipeline report':run.kind==='robustness'?'Stress-test report':'Net '+fmt(run.metrics?.net_profit,2))+' · Not independently validated';
      const download=document.createElement('button');download.className='secondary';download.textContent='Download complete backup';
      const pipelineRoot=run.kind==='cli_pipeline_started';
      if(mcpGeneration||cliPipeline)download.textContent='Download this record (not whole search)';
      if(pipelineRoot)download.textContent='Download whole '+researchNoun+' backup';
      download.onclick=async()=>{try{const raw=await rawBackup((pipelineRoot?'/api/pipeline/backup?id=':'/api/research/run?id=')+encodeURIComponent(run.run_id));downloadRawJson('sqxlabs-research-'+run.run_id+'.json',raw);}catch(e){announce(e.message);}};
      const replay=document.createElement('button');replay.className='secondary';replay.textContent='Verify replay';
      if(run.kind==='cli_backtest'||run.kind==='cli_backtest_started'){replay.disabled=true;replay.textContent='Dashboard replay unavailable for CLI records';}
      if(run.kind==='assistant_candidates'){replay.disabled=true;replay.textContent='Replay unavailable for AI sessions';}
      if(cliPipeline){replay.disabled=true;replay.textContent='Use CLI for pipeline recovery';replay.title='A single record is not a complete pipeline backup. Use export-pipeline with the start record ID; replay-pipeline requires the original executable.';}
      if(mcpGeneration){replay.disabled=true;replay.textContent='Replay unavailable for MCP generation';}
      const restartTrial=run.kind==='assistant_trial_started';
      if(restartTrial){replay.textContent='Restart frozen trial';replay.title='Runs this backtest from the beginning using saved inputs. Does not resume the AI conversation.';}
      replay.onclick=async()=>{
        replay.disabled=true;
        $('research-status').textContent=restartTrial?'Restarting one frozen backtest…':'Replaying archived inputs…';
        try {
          const result=await postJSON('/api/research/replay',JSON.stringify({run_id:run.run_id,...(restartTrial?{resume:true}:{})}));
          if(result.error)throw new Error(result.error);
          if(restartTrial){
            if(result.resumed!==true||!result.run_id)throw new Error('The server did not confirm a saved trial result.');
            await refreshResearchRuns();
            $('research-status').textContent=result.execution_valid===true?'Trial restarted and saved. The AI conversation was not resumed.':'Trial result saved with execution errors. Treat it as invalid; the AI conversation was not resumed.';
          }else{
            $('research-status').textContent=result.matches?(result.execution_valid===false?'Replay matches the recorded execution errors. This trial remains invalid.':'Replay verified: the archived inputs reproduce the recorded report exactly.'):'Replay differs from the recorded report. Treat this run as unresolved.';
          }
        }catch(error){$('research-status').textContent='Could not complete saved research action: '+error.message;}
        finally{replay.disabled=false;}
      };
      if(pipelineRoot){
        let inspected=null;
        replay.disabled=false;replay.textContent='Inspect '+researchNoun;replay.title='Check saved progress before choosing replay or recovery.';
        replay.onclick=async()=>{
          replay.disabled=true;
          try{
            if(!inspected){
              const status=await getJSON('/api/pipeline/status?id='+encodeURIComponent(run.run_id));
              if(status.error)throw new Error(status.error);
              if(!['incomplete','completion_recorded'].includes(status.status)||typeof status.engine_matches!=='boolean')throw new Error('Missing pipeline status confirmation.');
              inspected=status;
              metadata.textContent=researchTitle+' '+run.run_id.slice(0,12)+' · '+status.event_count+' saved steps · '+(status.status==='incomplete'?'Incomplete':status.execution_valid===true?'Completion recorded':'Completion recorded with errors')+' · Not independently validated';
              replay.textContent=status.engine_matches?(status.status==='incomplete'?'Resume '+researchNoun:'Verify '+researchNoun+' replay'):'Original application build required';
              $('research-status').textContent=status.engine_matches?'Saved links checked. Replay recomputes saved work and compares it before continuing; this can take several minutes.':'Saved links checked. Download and inspection are available, but replay requires the original application build. A backup does not install that build.';
              return;
            }
            if(!inspected.engine_matches)return;
            $('research-status').textContent='Recomputing the saved '+researchNoun+'. If this connection is lost, inspect it again before recovery…';
            const result=await postJSON('/api/pipeline/replay',JSON.stringify({run_id:run.run_id,resume:inspected.status==='incomplete'}));
            if(result.error)throw new Error(result.error);
            if(result.matches!==true||!result.completion_id)throw new Error(researchTitle+' completion was not confirmed.');
            await refreshResearchRuns();
            $('research-status').textContent=result.execution_valid===true?researchTitle+' reproduced and saved. This verifies repeatability, not strategy robustness.':researchTitle+' reproduced with execution errors. Treat this result as invalid.';
          }catch(error){inspected=null;replay.textContent='Inspect '+researchNoun;$('research-status').textContent=researchTitle+' action incomplete: '+error.message;}
          finally{replay.disabled=inspected?.engine_matches===false;}
        };
      }
      card.append(name,metadata,download,document.createTextNode(' '),replay);$('research-runs').append(card);
    }
  } catch(e){$('research-status').textContent='Could not load saved research: '+e.message;}
}
$('research-refresh').onclick=refreshResearchRuns;
$('research-restore').onchange=async function(){
  const file=this.files[0];if(!file)return;
  if(file.size>100*1024*1024){announce('This backup exceeds the browser restore limit of 100 MB.');return;}
  try{
    const raw=await file.text();const parsed=JSON.parse(raw);
    const pipeline=parsed?.start?.run?.request?.research_kind==='cli_pipeline_started';
    const response=await postJSON(pipeline?'/api/pipeline/restore':'/api/research/restore',raw);
    if(response.error)throw new Error(response.error);
    $('research-status').textContent=pipeline?'Whole pipeline restored and archive links checked. No calculations were run; replay requires the original application build.':'Research backup restored and integrity checked. No existing run was overwritten.';
    await refreshResearchRuns();
  }catch(e){announce('Could not restore research: '+e.message);}
};
refreshResearchRuns();


let activeSearch = null;
let searchStarting = false;
function exposureLabel(audit) {
  if (!audit) return 'Exposure audit was not recorded. ';
  if (audit.classification==='known_reuse_or_disclosed_exposure') return 'EXPLORATORY ONLY — known data reuse or disclosed prior exposure. ';
  if (audit.observation_extent_complete!==true) return 'EXPOSURE AUDIT INCOMPLETE — candle durations are missing or unverified in current or retained evidence. ';
  return 'No known overlap in this registry; independence is not proven. ';
}
async function showExperiment() {
  const id=$('experiment-select').value;
  $('experiment-trials').replaceChildren();$('experiment-final').replaceChildren();$('experiment-jobs').replaceChildren();
  if(!id)return;
  const state=await getJSON('/api/experiments/status?id='+encodeURIComponent(id));
  if(state.error)throw new Error(state.error);
  if($('experiment-select').value!==id)return;
  $('experiment-context').textContent=state.state+' · '+state.trials_attempted+' attempted candidates · Final period '+new Date(state.holdout_start_time).toISOString()+' to '+new Date(state.holdout_end_time).toISOString()+' · Deposit '+state.plan.config.initial_deposit+' '+state.plan.config.account.currency+' · Prior exposure: '+(state.plan.prior_exposure_disclosed?'yes / unsure':'not declared')+'. '+state.scope;
  const audit=state.current_exposure_audit;
  const exposure=document.createElement('div');
  exposure.textContent=exposureLabel(audit)+audit.same_market_experiment_attempts+' experiment attempts on this market; '+audit.same_market_archived_backtest_records+' archived backtest records; '+(audit.same_market_archived_assistant_records||0)+' archived AI records; '+(audit.same_market_archived_mcp_generation_records||0)+' archived MCP generation records (not independent trial counts); '+(audit.same_market_archived_pipeline_records||0)+' archived pipeline reports; '+(audit.same_market_archived_crosscheck_records||0)+' archived crosscheck records (including checkpoints); '+(audit.same_market_archived_robustness_records||0)+' archived stress-test reports; '+(audit.same_market_archived_comparison_records||0)+' saved paper comparisons; '+(audit.same_market_evolution_generation_claims||0)+' claimed evolution generations. '+audit.scope;
  $('experiment-trials').append(exposure);
  for(const overlap of audit.known_overlaps){const line=document.createElement('p');line.textContent=overlap.source+' · '+overlap.id.slice(0,16)+' · '+new Date(overlap.start).toISOString()+' → '+new Date(overlap.end).toISOString();$('experiment-trials').append(line);}
  $('experiment-trial').disabled=state.state!=='development';$('experiment-search').disabled=state.state!=='development'||!!activeSearch||searchStarting;
  for(const job of state.jobs||[]) {
    const row=document.createElement('p');row.textContent='Search '+job.job_id+' · Seed '+job.seed+' · '+job.completed+'/'+job.total+' saved · '+job.state+' ';
    if(job.state==='resumable'){const resume=document.createElement('button');resume.textContent='Resume saved search';resume.disabled=!!activeSearch;resume.onclick=()=>runSavedSearch(id,job.job_id);row.append(resume);}
    $('experiment-jobs').append(row);
  }
  for(const item of state.events.filter(e=>e.event.kind==='trial_started')) {
    const attempt=item.event,finished=state.events.find(e=>e.event.kind==='trial_finished'&&e.event.trial===attempt.trial);
    const row=document.createElement('p');
    row.textContent='Candidate '+attempt.trial+' · '+attempt.strategy.name+' · '+(finished?'Net '+fmt(finished.event.report.metrics.net_profit,2)+' · '+finished.event.report.metrics.trades+' trades · '+(finished.event.report.evidence_sufficient?'trade threshold met':'insufficient evidence / invalid execution'):'interrupted before recording a result');
    const open=document.createElement('button');open.className='secondary';open.textContent='Open rules';open.onclick=()=>setStrategy(attempt.strategy,'Opened frozen experiment candidate');row.append(document.createTextNode(' '),open);
    if(state.state==='development'&&finished&&finished.event.report.evidence_sufficient) {
      const reveal=document.createElement('button');reveal.textContent='Commit candidate & reveal final test';
      reveal.onclick=()=>experimentAction('reveal',{trial:attempt.trial});row.append(document.createTextNode(' '),reveal);
    }
    $('experiment-trials').append(row);
  }
  const final=state.events.find(e=>e.event.kind==='holdout_finished');
  if(final){const report=final.event.report;const result=document.createElement('p');const committedAudit=report.exposure_at_commitment;result.textContent=('Exposure at commitment: '+exposureLabel(committedAudit))+'Final test · Candidate '+report.trial+' · Net '+fmt(report.metrics.net_profit,2)+' · Drawdown '+fmt(report.metrics.max_drawdown_pct,2)+'% · '+report.metrics.trades+' trades · '+(report.evidence_sufficient?'trade threshold met':'insufficient evidence / invalid execution')+'. '+report.interpretation;$('experiment-final').append(result);}
  else if(state.state==='committed') {const commit=state.events.find(e=>e.event.kind==='holdout_committed');const resume=document.createElement('button');resume.textContent='Resume committed final test';resume.onclick=()=>experimentAction('reveal',{trial:commit.event.trial});$('experiment-final').append(resume);}
}
async function refreshExperiments(selected) {
  const previous=selected||$('experiment-select').value;
  const result=await getJSON('/api/experiments');if(result.error)throw new Error(result.error);
  $('experiment-select').replaceChildren();for(const id of result.experiments){const option=document.createElement('option');option.value=id;option.textContent=id.slice(0,16);$('experiment-select').append(option);}
  if(result.experiments.includes(previous))$('experiment-select').value=previous;
  await showExperiment();
}
async function experimentAction(action,extra) {
  $('experiment-status').textContent='Recording and running '+action+'…';
  try {const request=Object.assign({experiment_id:$('experiment-select').value},extra);const result=await postJSON('/api/experiments/'+action,JSON.stringify(request));if(result.error)throw new Error(result.error);await showExperiment();$('experiment-status').textContent=action==='reveal'?'Final test recorded. This experiment is frozen.':'Development attempt(s) recorded; final-test candles were excluded.';}
  catch(e){$('experiment-status').textContent=e.message;await showExperiment().catch(()=>{});}
}
$('experiment-create').onclick=async()=>{
  try {
    const date=$('experiment-start').value,disclosure=$('experiment-exposure').value;
    if(!date||!disclosure)throw new Error('Choose a final-test start and disclose prior data exposure.');
    const result=await postJSON('/api/experiments/create',JSON.stringify(Object.assign({},settings(),{symbol:selectedSymbol(),holdout_start_time:Date.parse(date+'Z'),min_development_trades:Number($('experiment-min-dev').value),min_holdout_trades:Number($('experiment-min-final').value),prior_exposure_disclosed:disclosure==='true'})));
    if(result.error)throw new Error(result.error);await refreshExperiments(result.experiment_id);$('experiment-status').textContent='Dataset, costs, split and thresholds frozen. Later changes in Settings do not alter this experiment.';
  }catch(e){$('experiment-status').textContent=e.message;}
};
$('experiment-trial').onclick=()=>{try{return experimentAction('trial',{strategy:readStrategy()});}catch(e){$('experiment-status').textContent=e.message;}};
async function runSavedSearch(id,jobId) {
  if(activeSearch)return;
  const run={id,jobId,stop:false};activeSearch=run;
  $('experiment-stop').hidden=false;$('experiment-stop').disabled=false;
  try {
    await showExperiment();
    while(!run.stop) {
      const job=await postJSON('/api/experiments/jobs/step',JSON.stringify({experiment_id:id,job_id:jobId}));
      if(job.error)throw new Error(job.error);
      $('experiment-status').textContent='Search '+jobId+' ('+id.slice(0,8)+'): '+job.completed+'/'+job.total+' candidates saved.';
      if(job.state==='complete')break;
    }
    if(run.stop)$('experiment-status').textContent='Search paused after the current candidate. Saved progress can be resumed, also after reopening the app.';
  }catch(e){$('experiment-status').textContent=e.message+' Refresh to see saved progress before resuming.';}
  finally{activeSearch=null;$('experiment-stop').hidden=true;await showExperiment().catch(()=>{});}
}
$('experiment-stop').onclick=()=>{if(activeSearch){activeSearch.stop=true;$('experiment-stop').disabled=true;$('experiment-status').textContent='Finishing and saving the current candidate…';}};
$('experiment-search').onclick=async()=>{
  if(activeSearch||searchStarting)return;
  searchStarting=true;$('experiment-search').disabled=true;
  let pendingKey;
  try {
    const id=$('experiment-select').value,count=Number($('experiment-count').value),seed=Number($('experiment-seed').value);
    if(!id)throw new Error('Select an experiment first.');
    pendingKey='sqxlabs.pending-search.'+id;
    const pending=sessionStorage.getItem(pendingKey);
    let request;
    if(pending) {
      request=JSON.parse(pending);
      if(request.experiment_id!==id||typeof request.request_key!=='string')throw new Error('Stored search request is invalid.');
      $('experiment-status').textContent='Recovering the previously requested search with its original seed and candidate count…';
    }else{
      if(!Number.isSafeInteger(count)||count<1||count>2000||!Number.isSafeInteger(seed)||seed<0)throw new Error('Choose 1–2000 candidates and a non-negative integer seed.');
      request={experiment_id:id,count,seed,request_key:crypto.randomUUID()};
      // Persist before sending: a lost response must not create another search on retry.
      sessionStorage.setItem(pendingKey,JSON.stringify(request));
    }
    const job=await postJSON('/api/experiments/jobs/start',JSON.stringify(request));
    if(job.error)throw new Error(job.error);
    sessionStorage.removeItem(pendingKey);
    await runSavedSearch(id,job.job_id);
  }catch(e){$('experiment-status').textContent=e.message+' If the start was sent, retry Start saved search to recover that same request; its original settings are retained.';}
  finally{searchStarting=false;await showExperiment().catch(()=>{});}
};
$('experiment-refresh').onclick=()=>refreshExperiments().catch(e=>{$('experiment-status').textContent=e.message;});
$('experiment-select').onchange=()=>showExperiment().catch(e=>{$('experiment-status').textContent=e.message;});
refreshExperiments().catch(e=>{$('experiment-status').textContent=e.message;});

$('experiment-backup').onclick=async()=>{
  try {const id=$('experiment-select').value;if(!id)throw new Error('Select an experiment first.');const raw=await rawBackup('/api/experiments/backup?id='+encodeURIComponent(id));downloadRawJson('sqxlabs-experiment-'+id+'.json',raw);$('experiment-status').textContent='Backup includes frozen data, settings, all recorded attempts and final-test commitment. Retain the matching engine build separately.';}
  catch(e){$('experiment-status').textContent=e.message;}
};
$('experiment-restore').onchange=async function(){
  const file=this.files[0];if(!file)return;
  try {if(file.size>100*1024*1024)throw new Error('This backup exceeds the 100 MB restore limit.');const raw=await file.text();JSON.parse(raw);const result=await postJSON('/api/experiments/restore',raw);if(result.error)throw new Error(result.error);await refreshExperiments(result.experiment_id);$('experiment-status').textContent='Experiment restored and integrity checked. Existing history and final-test commitments were retained.';}
  catch(e){$('experiment-status').textContent='Restore failed: '+e.message;}finally{this.value='';}
};

function comparisonTolerances() {
  const out={};
  for(const [key,id] of [['time_ms','paper-time'],['price','paper-price'],['size','paper-size'],['money','paper-money']]){
    const raw=$(id).value.trim(),value=Number(raw);
    if(!raw || !Number.isFinite(value) || value<0 || (key==='time_ms'&&!Number.isSafeInteger(value)))throw Error('Enter a valid nonnegative tolerance for '+key+'.');
    out[key]=value;
  }
  return out;
}
function comparisonReportHtml(record) {
  const result=record.result,report=result.comparison.report;
  let html='<h3>'+ (report.matches_within_tolerance?'Closed trades agree within the chosen tolerances':'Correspondence not established')+'</h3><p>This is not approval for live trading. Source declarations have not been authenticated.</p>';
  const source=JSON.parse(record.source_json),scope=result.comparison.observed_scope;
  html+='<p>Imported source (user-declared): '+escapeHtml(source.source_label)+'</p>';
  html+='<p>'+escapeHtml(scope.market)+' · Quote currency '+escapeHtml(scope.quote_currency)+' · Account currency '+escapeHtml(scope.account_currency)+' · Contract units per lot '+escapeHtml(String(scope.contract_size))+'</p>';
  const time=value=>{const date=new Date(value);return Number.isFinite(date.getTime())?date.toISOString():String(value)+' milliseconds';};
  html+='<p>Observation period (UTC, inclusive): '+escapeHtml(time(scope.start_ms))+' → '+escapeHtml(time(scope.end_ms))+'</p>';
  const differing=report.comparisons.filter(pair=>pair.differences.length).length;
  html+='<p>'+report.comparisons.length+' paired trades · '+differing+' pairs with differences · '+report.missing_expected_ids.length+' missing · '+report.unexpected_tickets.length+' unexpected</p>';
  html+='<p>Reference '+escapeHtml(result.run_id)+' · Source '+escapeHtml(record.source_id)+'</p>';
  html+='<p>Tolerances: '+escapeHtml(JSON.stringify(report.tolerances))+'</p>';
  html+='<p>Missing expected trade IDs: '+escapeHtml(report.missing_expected_ids.join(', ')||'None')+' · Unexpected tickets: '+escapeHtml(report.unexpected_tickets.join(', ')||'None')+'</p>';
  if(result.end_of_data_trade_ids.length)html+='<p class="neg">Backtest reporting liquidations (not live signal orders): '+escapeHtml(result.end_of_data_trade_ids.join(', '))+'</p>';
  html+='<div class="table-scroll"><table><tr><th>Expected trade</th><th>Paper ticket</th><th>Field</th><th>Expected</th><th>Observed</th></tr>';
  for(const pair of report.comparisons){
    if(!pair.differences.length)html+='<tr><td>'+pair.expected_trade_id+'</td><td>'+escapeHtml(pair.ticket)+'</td><td colspan="3">Within tolerances</td></tr>';
    for(const field of pair.differences)html+='<tr><td>'+pair.expected_trade_id+'</td><td>'+escapeHtml(pair.ticket)+'</td><td>'+escapeHtml(field)+'</td><td>'+escapeHtml(String(pair.expected[field]))+'</td><td>'+escapeHtml(String(pair.observed[field]))+'</td></tr>';
  }
  return html+'</table></div>';
}
async function refreshComparisons() {
  const [runs,list]=await Promise.all([getJSON('/api/research/runs'),getJSON('/api/comparisons')]);
  if(runs.error||list.error)throw Error(runs.error||list.error);
  const select=$('paper-run'),previous=select.value;select.replaceChildren();
  const placeholder=document.createElement('option');placeholder.value='';placeholder.textContent='Select an archived backtest';select.append(placeholder);
  for(const run of runs.runs.filter(r=>r.kind==='backtest')){const option=document.createElement('option');option.value=run.run_id;option.textContent=run.name+' · '+run.symbol+' · '+run.run_id.slice(0,12);select.append(option);}
  if(Array.from(select.options).some(o=>o.value===previous))select.value=previous;
  const box=$('paper-list');box.replaceChildren();
  if(!list.comparisons.length)box.textContent='No saved paper comparisons.';
  for(const row of list.comparisons){
    const card=document.createElement('div');card.className='run-card';
    const title=document.createElement('p');title.textContent=(row.source_label||'Source label unavailable')+' · '+row.comparison_id.slice(0,12)+' · '+(row.matches_within_tolerance?'Within selected tolerances':'Correspondence not established')+' · Differing pairs '+(row.differing_pairs??'?')+' · Missing '+row.missing_trades+' · Unexpected '+row.unexpected_trades;
    const inspect=document.createElement('button');inspect.textContent='Inspect differences';inspect.onclick=async()=>{try{const data=await getJSON('/api/comparison?id='+encodeURIComponent(row.comparison_id));if(data.error)throw Error(data.error);$('paper-report').innerHTML=comparisonReportHtml(data.record);}catch(e){$('paper-status').textContent=e.message;}};
    const backup=document.createElement('button');backup.className='secondary';backup.textContent='Download comparison backup';backup.onclick=async()=>{try{const raw=await rawBackup('/api/comparison?id='+encodeURIComponent(row.comparison_id));downloadRawJson('sqxlabs-comparison-'+row.comparison_id+'.json',raw);}catch(e){$('paper-status').textContent=e.message;}};
    card.append(title,inspect,document.createTextNode(' '),backup);box.append(card);
  }
}
$('paper-refresh').onclick=()=>refreshComparisons().catch(e=>{$('paper-status').textContent=e.message;});
$('paper-workspace').addEventListener('toggle',()=>{if($('paper-workspace').open)refreshComparisons().catch(e=>{$('paper-status').textContent=e.message;});});
$('paper-import').onclick=async()=>{
  const button=$('paper-import');button.disabled=true;let sent=false;
  try{
    const run_id=$('paper-run').value,file=$('paper-source').files[0];
    if(!run_id||!file)throw Error('Select a reference backtest and a paper-trade file.');
    if(file.size>4*1024*1024)throw Error('The paper-trade file exceeds 4 MiB.');
    const tolerances=comparisonTolerances(),source_json=await file.text();JSON.parse(source_json);
    $('paper-status').textContent='Comparing and saving…';sent=true;
    const result=await postJSON('/api/comparison/import',JSON.stringify({run_id,source_json,tolerances}));sent=false;
    if(result.error)throw Error(result.error);
    $('paper-status').textContent='Comparison saved. Inspect the differences; this does not approve live trading.';
    await refreshComparisons();
    const data=await getJSON('/api/comparison?id='+encodeURIComponent(result.comparison_id));if(data.error)throw Error(data.error);$('paper-report').innerHTML=comparisonReportHtml(data.record);
  }catch(e){$('paper-status').textContent=sent?'Response lost; the comparison may already be saved. Refresh comparisons before retrying.':e.message;}
  finally{button.disabled=false;}
};
$('paper-restore').onchange=async function(){
  const file=this.files[0];if(!file)return;
  try{if(file.size>100*1024*1024)throw Error('Backup exceeds 100 MiB.');const raw=await file.text();JSON.parse(raw);const result=await postJSON('/api/comparison/restore',raw);if(result.error)throw Error(result.error);$('paper-status').textContent='Comparison restored; source integrity and derived results verified.';await refreshComparisons();}
  catch(e){$('paper-status').textContent='Restore failed: '+e.message;}finally{this.value='';}
};

function evolutionReportHtml(result) {
  const status=result.status||{},latest=Object.prototype.hasOwnProperty.call(result,'latest')?result.latest:result;
  let header='<h4>Saved search results</h4><p>'+escapeHtml(status.committed_generations??0)+' generations saved · '+(status.complete?'Search complete':'Search not complete')+'</p><p class="muted">Exploratory results on the supplied data. Repeated candidates are not independent evidence; these results do not establish out-of-sample performance.</p>';
  const retention=result.engine_retention?.status;
  const retentionText=retention==='verified'?'The original engine copy is saved and its file integrity was verified.':retention==='missing'?'The original engine copy is missing from this archive. Keep the matching installed version before updating.':retention==='unverified'?'The saved engine copy could not be verified. Inspect the archive before relying on recovery.':'Engine-copy availability has not been checked in this response. Inspect the saved search to check it.';
  header+='<p>'+retentionText+' Search JSON backups do not include the engine. A saved copy does not guarantee compatibility with a different computer or validate trading results.</p>';
  const metadata=result.metadata,context=metadata?.context,config=metadata?.backtest_config,quality=context?.data_quality;
  const show=value=>value===null||value===undefined?'Not recorded':escapeHtml(value);
  const date=value=>Number.isSafeInteger(value)&&Number.isFinite(new Date(value).getTime())?new Date(value).toISOString():'Not recorded';
  if(metadata){
    const range=metadata.observation_range||[];
    header+='<details open><summary>Frozen research context</summary><p>These are the saved settings, not the current controls.</p><p>Market '+show(metadata.instrument?.name)+' · Timeframe '+show(context?.timeframe)+' · Seed '+show(context?.seed)+'</p><p>First / last bar timestamps (UTC): '+date(range[0])+' → '+date(range[1])+'</p>';
    if(config)header+='<p>Account '+show(config.account?.currency)+' · Initial balance '+show(config.initial_deposit)+' · Maximum leverage '+show(config.account?.max_leverage)+':1</p><p>Spread '+show(config.spread_points)+' FX pips · Slippage '+show(config.slippage_points)+' FX pips · Round-trip commission '+show(config.commission_per_lot)+' per lot · Intrabar policy '+show(config.intrabar_policy)+'</p>';
    header+='<p>Ranking objective '+show(metadata.fitness)+' · Minimum trades '+show(metadata.genetic_config?.min_trades)+'</p>';
    if(quality)header+='<p>Data audit: '+show(quality.bars)+' bars · '+show(quality.incomplete_bars)+' incomplete · '+show(quality.missing_open_intervals)+' missing open intervals · '+show(quality.unclassified_intervals)+' unclassified intervals · '+show(quality.irregular_steps)+' irregular steps. Calendar: '+show(quality.calendar)+'. Gap allowance: '+(context.allow_data_gaps===true?'explicitly enabled':'not enabled')+'.</p>';
    else header+='<p>Data-quality evidence was not recorded for this job. This is not a clean-data result.</p>';
    if(context?.limitations)header+='<p>'+escapeHtml(context.limitations)+'</p>';
    header+='</details>';
  }else header+='<p>Frozen context is unavailable in this response; refresh and inspect the saved job.</p>';
  if(!latest||!Array.isArray(latest.evaluations))return header+'<p>No evaluated generation has been saved yet.</p>';
  const evaluations=latest.evaluations,invalid=evaluations.filter(e=>(e.errors||[]).length>0).length,eligible=evaluations.filter(e=>e.eligible&&!(e.errors||[]).length).length;
  const number=value=>typeof value==='number'&&Number.isFinite(value)?escapeHtml(value.toLocaleString('en-US',{maximumFractionDigits:2})):'Undefined';
  const rows=evaluations.map(e=>{
    const errors=e.errors||[],m=e.metrics||{},label=errors.length?'Invalid':e.eligible?'Eligible for ranking':'Below selection criteria';
    const reason=errors.length?errors.join('; '):e.eligible?'Eligible does not mean selected for breeding.':'Does not meet the minimum-trade or finite-fitness requirement.';
    const performance=key=>errors.length?'Not valid':number(m[key]);
    return '<tr><td>'+escapeHtml(e.candidate_index)+'</td><td>'+escapeHtml(e.strategy?.name||'Unnamed strategy')+'</td><td>'+label+'<br><small>'+escapeHtml(reason)+'</small></td><td>'+performance('number_of_trades')+'</td><td>'+performance('net_profit')+'</td><td>'+performance('profit_factor')+'</td><td>'+performance('max_drawdown_pct')+'</td></tr>';
  }).join('');
  return header+'<p>Generation '+escapeHtml(latest.generation)+' (zero-based): '+evaluations.length+' evaluated · '+eligible+' eligible · '+invalid+' invalid · '+(evaluations.length-eligible-invalid)+' below criteria.</p><p class="muted">Profit is in the frozen account currency. Undefined ratios are not zero. Candidate order is the evaluation order, not a ranking.</p><div style="overflow-x:auto" tabindex="0" role="region" aria-label="Candidate results; scroll horizontally for all columns"><table style="min-width:760px"><thead><tr><th>Candidate</th><th>Strategy</th><th>Outcome</th><th>Trades</th><th>Net profit</th><th>Profit factor</th><th>Drawdown %</th></tr></thead><tbody>'+rows+'</tbody></table></div><details><summary>Full saved evidence and ranking</summary><pre style="white-space:pre-wrap;overflow-wrap:anywhere">'+escapeHtml(JSON.stringify(result,null,2))+'</pre></details>';
}

// Never automatically repeat an advance after an uncertain response.
async function advanceSavedEvolution(jobId, sequence, button) {
  button.disabled=true;let sent=false;
  try {
    $('evolution-status').textContent='Evaluating one saved generation…';sent=true;
    const result=await postJSON('/api/evolution/advance',JSON.stringify({job_id:jobId,expected_sequence:sequence}));sent=false;
    if(result.error)throw Error(result.error);
    $('evolution-report').innerHTML=evolutionReportHtml(result);
    $('evolution-status').textContent='Generation saved. Review its results before continuing.';
    try{await refreshEvolutionJobs();}catch(e){$('evolution-status').textContent='Generation saved, but the search list could not refresh. Refresh saved searches before continuing.';}
  } catch(e) {
    $('evolution-status').textContent=sent?'Response lost; this generation may still be running or already saved. Refresh saved searches before continuing.':e.message;
  }
  // A refresh obtains a new sequence and creates a new action button.
}
async function refreshEvolutionJobs() {
  const data=await getJSON('/api/evolution/jobs');if(data.error)throw Error(data.error);
  const box=$('evolution-list');box.replaceChildren();
  if(!data.jobs.length){box.textContent='No saved genetic searches. Existing jobs started through the CLI or API appear here.';return;}
  for(const row of data.jobs) {
    const status=row.status,card=document.createElement('div');card.className='run-card';
    const title=document.createElement('p');title.textContent=row.metadata.request_key+' · '+status.job_id.slice(0,12)+' · '+status.committed_generations+' generations saved · '+(status.complete?'Complete':row.compatible_engine?'Ready to continue':'Resume with the original engine executable');
    const inspect=document.createElement('button');inspect.className='secondary';inspect.textContent='Inspect saved results';
    inspect.onclick=async()=>{try{const result=await getJSON('/api/evolution/status?id='+encodeURIComponent(status.job_id));if(result.error)throw Error(result.error);$('evolution-report').innerHTML=evolutionReportHtml(result);}catch(e){$('evolution-status').textContent=e.message;}};
    const advance=document.createElement('button');advance.textContent='Run one generation';advance.disabled=status.complete||!row.compatible_engine;
    advance.onclick=()=>advanceSavedEvolution(status.job_id,status.committed_generations,advance);
    const backup=document.createElement('button');backup.className='secondary';backup.textContent='Download search archive';
    backup.onclick=async()=>{try{const raw=await rawBackup('/api/evolution/backup?id='+encodeURIComponent(status.job_id));downloadRawJson('sqxlabs-evolution-'+status.job_id+'.json',raw);}catch(e){$('evolution-status').textContent=e.message;}};
    const replay=document.createElement('button');replay.className='secondary';replay.textContent='Verify saved replay';replay.disabled=!row.compatible_engine||status.committed_generations===0;
    replay.onclick=async()=>{replay.disabled=true;try{$('evolution-status').textContent='Recomputing every saved generation; progress will not be advanced…';const result=await postJSON('/api/evolution/replay',JSON.stringify({job_id:status.job_id}));if(result.error)throw Error(result.error);$('evolution-status').textContent=result.evaluations+' candidate evaluations match checkpoint '+result.checkpoint_id+'. This proves repeatability, not independent strategy validation.';}catch(e){$('evolution-status').textContent='Replay verification not established: '+e.message;}finally{replay.disabled=false;}};
    card.append(title,inspect,document.createTextNode(' '),advance,document.createTextNode(' '),backup,document.createTextNode(' '),replay);box.append(card);
  }
}
$('evolution-refresh').onclick=()=>refreshEvolutionJobs().catch(e=>{$('evolution-status').textContent=e.message;});
$('evolution-workspace').addEventListener('toggle',()=>{if($('evolution-workspace').open)refreshEvolutionJobs().catch(e=>{$('evolution-status').textContent=e.message;});});

async function createSavedEvolution() {
  const button=$('evolution-create'),key='sqxlabs.pending-evolution';button.disabled=true;let sent=false;
  try {
    let raw=localStorage.getItem(key),request;
    if(raw){request=JSON.parse(raw);if(typeof request.request_key!=='string')throw Error('Invalid pending request.');}
    else {
      const integer=(id,min,max)=>{const text=$(id).value.trim(),n=Number(text);if(!text||!Number.isSafeInteger(n)||n<min||n>max)throw Error('Enter a whole number between '+min+' and '+max+' for '+id.replace('evolution-','')+'.');return n;};
      const seed=integer('evolution-seed',0,Number.MAX_SAFE_INTEGER),population_size=integer('evolution-population',2,2000),generations=integer('evolution-generations',0,1000),min_trades=integer('evolution-min-trades',1,Number.MAX_SAFE_INTEGER);
      const symbol=selectedSymbol();if(!symbol)throw Error('Select a market first.');
      request=Object.assign({},settings(),genSettings(),{symbol,seed,request_key:crypto.randomUUID(),ga:{population_size,generations,min_trades,elite_count:Math.min(5,population_size-1),crossover_prob:0.8,mutation_prob:0.3}});
      raw=JSON.stringify(request);localStorage.setItem(key,raw);
    }
    // Recover an already-created job before resolving mutable live data again.
    const existing=await getJSON('/api/evolution/jobs');if(existing.error)throw Error(existing.error);
    let job=existing.jobs.find(row=>row.metadata.request_key===request.request_key);
    if(!job){sent=true;const result=await postJSON('/api/evolution/start',raw);sent=false;if(result.error)throw Error(result.error);job=result;}
    localStorage.removeItem(key);
    $('evolution-status').textContent='Saved search '+job.status.job_id.slice(0,12)+' is available. Refresh and inspect it before running a generation.';
    await refreshEvolutionJobs();
  }catch(e){$('evolution-status').textContent=(sent?'Creation response lost; the job may already exist. ':e.message+' ')+'Use Create or recover to retain the original request and settings.';}
  finally{button.disabled=false;}
}
$('evolution-create').onclick=createSavedEvolution;
$('evolution-discard').onclick=()=>{localStorage.removeItem('sqxlabs.pending-evolution');$('evolution-status').textContent='Pending creation request cleared. Existing saved jobs were not deleted; refresh the list before creating another search.';};

$('evolution-restore').onchange=async function(){
  const file=this.files[0];if(!file)return;this.disabled=true;let sent=false;
  try{
    if(file.size>100*1024*1024)throw Error('Archive exceeds 100 MiB.');
    const raw=await file.text();JSON.parse(raw);sent=true;
    const result=await postJSON('/api/evolution/restore',raw);sent=false;if(result.error)throw Error(result.error);
    $('evolution-status').textContent='Search archive restored. Existing newer generations were retained. Matching engine required to resume; this is not independent validation.';
    await refreshEvolutionJobs();
  }catch(e){$('evolution-status').textContent=sent?'Restore response lost. Refresh saved searches before retrying the same archive.':e.message;}
  finally{this.disabled=false;this.value='';}
};
