// Run with: node crates/sqx-web/tests/backup_wire.cjs
// Exercise the actual browser helpers/handlers with DOM and transport doubles.
// Integer-looking floats and integers above JS's safe range must stay verbatim.
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const vm = require('node:vm');
const source = fs.readFileSync(path.join(__dirname,'../assets/workspace.js'),'utf8');
const payload = '{"equity":10000.0,"timestamp":9007199254740993,"price":1.2345678901234567,"name":"€ test"}';
let downloaded, posted;
const nodes = new Map();
const context = vm.createContext({
  Blob, JSON, Error, encodeURIComponent,
  fetch: async()=>({ok:true,text:async()=>payload}),
  document:{createElement:()=>({click(){}})},
  URL:{createObjectURL(blob){downloaded=blob;return 'blob:test';},revokeObjectURL(){}},
  setTimeout(callback){callback();},
  $(id){if(!nodes.has(id))nodes.set(id,{});return nodes.get(id);},
  postJSON:async(url,body)=>{posted={url,body};return {experiment_id:'fixture'};},
  refreshExperiments:async()=>{},refreshResearchRuns:async()=>{},
  announce(message){throw new Error(message);},
});
function section(start,end){const a=source.indexOf(start);assert(a>=0);const b=source.indexOf(end,a);assert(b>a);return source.slice(a,b);}
vm.runInContext(section('function downloadJson(', 'function renderRunHistory('),context);
// Load the production upload handlers, not a reproduction of their logic.
for(const id of ['research-restore','experiment-restore']) {
  const start=source.indexOf("$('"+id+"').onchange=async function()");assert(start>=0);
  const end=source.indexOf('\n};',start);assert(end>start);
  vm.runInContext(source.slice(start,end+3),context);
}
(async()=>{
  const raw=await context.rawBackup('/fixture');
  context.downloadRawJson('backup.json',raw);
  assert.equal(await downloaded.text(),payload,'download must preserve numeric JSON representations');
  for(const id of ['research-restore','experiment-restore']){
    const input={files:[{size:payload.length,text:async()=>payload}],value:'fixture'};
    await nodes.get(id).onchange.call(input);
    assert.equal(posted.body,payload,id+' must transmit the original backup text');
    assert.equal(posted.url,id==='research-restore'?'/api/research/restore':'/api/experiments/restore');
  }
  console.log('Backup wire regression passed: download and both restores preserve exact JSON.');
})().catch(error=>{console.error(error);process.exitCode=1;});
