const fs=require('node:fs'),vm=require('node:vm'),assert=require('node:assert/strict');
const source=fs.readFileSync('crates/sqx-web/assets/workspace.js','utf8'),start=source.indexOf('function evolutionReportHtml('),end=source.indexOf('// Never automatically repeat',start);
assert(start>=0&&end>start);const ctx=vm.createContext({escapeHtml:s=>String(s).replaceAll('&','&amp;').replaceAll('<','&lt;').replaceAll('>','&gt;')});vm.runInContext(source.slice(start,end),ctx);
const result={status:{committed_generations:1,complete:false},latest:{generation:0,evaluations:[
 {candidate_index:0,strategy:{name:'<script>'},errors:[],eligible:true,metrics:{number_of_trades:10,net_profit:0,profit_factor:null,max_drawdown_pct:2}},
 {candidate_index:1,errors:['<bad data>'],eligible:false,metrics:{net_profit:123456}},
 {candidate_index:2,errors:[],eligible:false,metrics:{number_of_trades:0}}
]}};
const html=ctx.evolutionReportHtml(result);
assert.match(html,/3 evaluated · 1 eligible · 1 invalid · 1 below criteria/);assert.match(html,/&lt;script&gt;/);assert.doesNotMatch(html,/<script>/);assert.match(html,/&lt;bad data&gt;/);assert.match(html,/>Undefined<\/td>/);assert.match(html,/>0<\/td>/);assert.match(html,/>Not valid<\/td>/);assert.match(html,/not independent evidence/);assert.match(html,/evaluation order, not a ranking/);
assert.match(ctx.evolutionReportHtml({status:{committed_generations:0},latest:null}),/No evaluated generation/);
assert.match(ctx.evolutionReportHtml({...result.latest,status:result.status}),/3 evaluated/);
console.log('Evolution report passed: invalid results, undefined ratios, counts, escaping and exploratory qualification.');

result.metadata={instrument:{name:'<EURUSD>'},observation_range:[0,60000],context:{seed:42,timeframe:'M1',allow_data_gaps:true,data_quality:{bars:2,incomplete_bars:0,missing_open_intervals:0,unclassified_intervals:1,irregular_steps:0,calendar:null}},backtest_config:{initial_deposit:4321,account:{currency:'USD',max_leverage:30},spread_points:5,slippage_points:2,commission_per_lot:7,intrabar_policy:'AdverseFirst'},fitness:'ProfitFactor',genetic_config:{min_trades:10}};
const frozen=ctx.evolutionReportHtml(result);assert.match(frozen,/saved settings, not the current controls/);assert.match(frozen,/&lt;EURUSD&gt;/);assert.match(frozen,/Initial balance 4321/);assert.match(frozen,/Spread 5 FX pips/);assert.match(frozen,/Slippage 2 FX pips/);assert.match(frozen,/1970-01-01T00:00:00.000Z/);assert.match(frozen,/1 unclassified intervals/);assert.match(frozen,/Gap allowance: explicitly enabled/);
result.metadata.context=null;assert.match(ctx.evolutionReportHtml(result),/not a clean-data result/);
