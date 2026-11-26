// Exercise the production start handler: an acknowledged server mutation with a
// lost response must reuse the original request after a page reload.
const assert = require('node:assert/strict');
const fs = require('node:fs');
const vm = require('node:vm');
const source = fs.readFileSync('crates/sqx-web/assets/workspace.js','utf8');
const start = source.indexOf("$('experiment-search').onclick=async()=>{");
const end = source.indexOf("$('experiment-refresh').onclick", start);
assert(start >= 0 && end > start);
const handler = source.slice(start,end);
const storage = new Map();
const sent = [];
let recover = false, runs = 0, uuids = 0;
function page(size=0.1) {
  const elements = new Map(['experiment-select','experiment-count','experiment-seed','experiment-search','experiment-status'].map(id=>[id,{}]));
  elements.get('experiment-select').value='frozen-experiment';
  elements.get('experiment-count').value='20';
  elements.get('experiment-seed').value='42';
  const context = vm.createContext({
    $: id=>elements.get(id), activeSearch:null, searchStarting:false,
    sessionStorage:{getItem:k=>storage.get(k)||null,setItem:(k,v)=>storage.set(k,v),removeItem:k=>storage.delete(k)},
    genSettings:()=>({
      min_conditions:1,max_conditions:2,sl_min:10,sl_max:30,pt_min:20,pt_max:50,
      size,long:true,short:false,param_ranges:[{indicator:'SMA',param:'period',min:5,max:10}]
    }),
    crypto:{randomUUID:()=>`request-${++uuids}`},
    postJSON:async(_url,body)=>{sent.push(JSON.parse(body));if(!recover)throw new Error('Lost response');return {job_id:7};},
    runSavedSearch:async(id,job)=>{assert.equal(id,'frozen-experiment');assert.equal(job,7);runs++;},
    showExperiment:async()=>{},
  });
  vm.runInContext(handler,context);
  return elements;
}
(async()=>{
  let elements=page();
  await elements.get('experiment-search').onclick();
  assert.equal(storage.size,1);assert.equal(runs,0);
  assert.deepEqual(sent[0].generator,{min_conditions:1,max_conditions:2,sl_points:[10,30],pt_points:[20,50],fixed_size:0.1,long:true,short:false,param_ranges:[["SMA","period",5,10]]});
  elements=page(0.2); // reload retains storage, not JavaScript variables
  elements.get('experiment-count').value='99';elements.get('experiment-seed').value='999';
  recover=true;
  await elements.get('experiment-search').onclick();
  assert.deepEqual(sent[1],sent[0]);assert.equal(uuids,1);assert.equal(runs,1);assert.equal(storage.size,0);
  // A genuinely new search uses the new settings and a new request key.
  await elements.get('experiment-search').onclick();
  assert.equal(sent[2].generator.fixed_size,0.2);assert.equal(sent[2].count,99);assert.equal(sent[2].seed,999);assert.notEqual(sent[2].request_key,sent[1].request_key);
  console.log('Search recovery passed: lost response and reload preserve request identity and settings.');
})().catch(error=>{console.error(error);process.exitCode=1;});
