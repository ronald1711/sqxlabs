const assert=require('node:assert/strict');
const fs=require('node:fs');
const vm=require('node:vm');
const source=fs.readFileSync('crates/sqx-web/assets/index.html','utf8');
const start=source.indexOf('function parseGeneratorRanges('),end=source.indexOf('var RESULTS =',start);
assert(start>=0&&end>start);
const fields=new Map([
 ['gen-ranges',{value:''}],['gen-minc',{value:'1'}],['gen-maxc',{value:'3'}],
 ['gen-slmin',{value:'0'}],['gen-slmax',{value:'200'}],['gen-ptmin',{value:'0'}],['gen-ptmax',{value:'400'}],
 ['gen-size',{value:'0.1'}],['gen-long',{checked:true}],['gen-short',{checked:true}],
]);
const context=vm.createContext({document:{getElementById:id=>fields.get(id)},settings:()=>({deposit:4321})});
vm.runInContext(source.slice(start,end),context);
const parse=context.parseGeneratorRanges;
assert.deepEqual(JSON.parse(JSON.stringify(parse('RSI.period = 5..10\n\nMACD.fast = +2e0..3.5'))),[
 {indicator:'RSI',param:'period',min:5,max:10},{indicator:'MACD',param:'fast',min:2,max:3.5}
]);
for(const invalid of ['garbage','RSI.period=1..','RSI.period=5..2','RSI.period=1..2 trailing','RSI.period=1e999..2e999','RSI.period=1..2\nRSI.period=3..4'])assert.throws(()=>parse(invalid));
assert.throws(()=>parse('\nwrong'),/line 2/);
const out={textContent:''};
fields.get('gen-ranges').value='RSI.period=2..1';
assert.equal(context.generatorInputs(out),null);assert.match(out.textContent,/line 1/);
fields.get('gen-ranges').value='RSI.period=2..5';fields.get('gen-size').value='0.1junk';
assert.equal(context.generatorInputs(out),null);assert.match(out.textContent,/finite number/);
fields.get('gen-size').value='0.1';
assert.equal(context.generatorInputs(out).deposit,4321);
assert.equal(context.generatorInputs(out).param_ranges.length,1);
// Exercise the actual Generate handler: invalid settings must prevent any POST.
let posts=0;
fields.set('gen-results',out);fields.set('count',{value:'20'});fields.set('seed',{value:'42'});fields.set('btn-generate',{});
fields.get('gen-ranges').value='invalid';
context.selectedSymbol=()=> 'EURUSD';context.postJSON=async()=>{posts++;throw Error('must not send');};
const handlerStart=source.indexOf('document.getElementById("btn-generate").onclick = async function() {');
const handlerEnd=source.indexOf('\n};',handlerStart)+3;
vm.runInContext(source.slice(handlerStart,handlerEnd),context);
(async()=>{await fields.get('btn-generate').onclick();assert.equal(posts,0);assert.match(out.textContent,/Invalid research settings/);console.log('Generator input regression passed: malformed lines are reported before requests.');})().catch(e=>{console.error(e);process.exitCode=1;});

// Use the production account/cost reader, not a replica of its parsing rules.
const costStart=source.indexOf('function settings()'),costEnd=source.indexOf('async function runRobustness()',costStart);
assert(costStart>=0&&costEnd>costStart);
const costFields=new Map();
for(const [id,value] of Object.entries({'set-deposit':'10000','set-spread':'12.5','set-slippage':'3','set-commission':'7','set-intrabar':'adverse_first','set-currency':'USD','set-leverage':'30','set-stopout':'0.5','set-min-lots':'0.01','set-lot-step':'0.01','set-max-lots':'100','set-long-financing':'0','set-short-financing':'0','set-rates':'[]'}))costFields.set(id,{value});
costFields.set('set-compare-paths',{checked:false});costFields.set('set-allow-gaps',{checked:false});
const costContext=vm.createContext({document:{getElementById:id=>costFields.get(id)}});
vm.runInContext(source.slice(costStart,costEnd),costContext);
assert.equal(costContext.settings().spread,12.5);
for(const value of ['','12oops','Infinity']) {costFields.get('set-spread').value=value;assert.throws(()=>costContext.settings(),/finite number/);}
costFields.get('set-spread').value='0';assert.equal(costContext.settings().spread,0);
