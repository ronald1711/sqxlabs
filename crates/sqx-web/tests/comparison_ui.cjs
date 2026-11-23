const fs=require('node:fs'),vm=require('node:vm'),assert=require('node:assert/strict');
const source=fs.readFileSync('crates/sqx-web/assets/workspace.js','utf8');
const start=source.indexOf('function comparisonTolerances()'),end=source.indexOf('async function refreshComparisons()',start);assert(start>=0&&end>start);
const fields=Object.fromEntries(['paper-time','paper-price','paper-size','paper-money'].map(id=>[id,{value:'0'}]));
const ctx=vm.createContext({$:id=>fields[id],escapeHtml:s=>String(s).replaceAll('&','&amp;').replaceAll('<','&lt;').replaceAll('>','&gt;')});
vm.runInContext(source.slice(start,end),ctx);
assert.equal(ctx.comparisonTolerances().time_ms,0);
for(const value of ['', '-1','NaN','Infinity','0.5','9007199254740992']){fields['paper-time'].value=value;assert.throws(()=>ctx.comparisonTolerances());}
fields['paper-time'].value='10';fields['paper-price'].value='0.001';assert.equal(ctx.comparisonTolerances().price,0.001);
const record={source_json:JSON.stringify({source_label:'<SYNTHETIC TEST>'}),source_id:'source',result:{run_id:'run',end_of_data_trade_ids:[7],comparison:{observed_scope:{market:'EURUSD',quote_currency:'USD',account_currency:'USD',contract_size:100000,start_ms:0,end_ms:59999},report:{matches_within_tolerance:false,tolerances:{time_ms:0},missing_expected_ids:[9],unexpected_tickets:['<extra>'],comparisons:[{expected_trade_id:7,ticket:'<script>',differences:['pnl'],expected:{pnl:10},observed:{pnl:-5}}]}}}};
const html=ctx.comparisonReportHtml(record);assert.match(html,/Correspondence not established/);assert.match(html,/&lt;SYNTHETIC TEST&gt;/);assert.match(html,/1 pairs with differences/);assert.match(html,/1970-01-01T00:00:00.000Z/);assert.match(html,/Account currency USD/);assert.match(html,/Missing expected trade IDs: 9/);assert.match(html,/&lt;script&gt;/);assert.doesNotMatch(html,/<script>/);assert.match(html,/&lt;extra&gt;/);assert.match(html,/not live signal orders/);assert.match(html,/>10<\/td>/);assert.match(html,/>-5<\/td>/);
record.result.comparison.report.matches_within_tolerance=true;
const matched=ctx.comparisonReportHtml(record);assert.match(matched,/agree within the chosen tolerances/);assert.match(matched,/not approval for live trading/);
console.log('Comparison UI passed: strict tolerances, explicit discrepancies, escaped imported text and qualified matching.');
