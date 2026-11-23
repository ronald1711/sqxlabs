const fs=require('node:fs'),vm=require('node:vm'),assert=require('node:assert/strict');
const source=fs.readFileSync('crates/sqx-web/assets/workspace.js','utf8');
const start=source.indexOf('async function advanceSavedEvolution('),end=source.indexOf('async function refreshEvolutionJobs()',start);assert(start>=0&&end>start);
const fields={'evolution-status':{},'evolution-report':{}};let calls=[],fail=true,refreshes=0;
const ctx=vm.createContext({refreshEvolutionJobs:async()=>{refreshes++;},evolutionReportHtml:JSON.stringify,$:id=>fields[id],postJSON:async(path,body)=>{calls.push({path,body:JSON.parse(body)});if(fail)throw Error('connection lost');return {status:{committed_generations:4},evaluations:[]};}});
vm.runInContext(source.slice(start,end),ctx);
(async()=>{
 const button={disabled:false};await ctx.advanceSavedEvolution('saved-job',3,button);
 assert.equal(calls.length,1);assert.equal(refreshes,0);assert.deepEqual(calls[0].body,{job_id:'saved-job',expected_sequence:3});assert.equal(button.disabled,true);assert.match(fields['evolution-status'].textContent,/may still be running or already saved/);assert.match(fields['evolution-status'].textContent,/Refresh/);
 fail=false;const refreshed={disabled:false};await ctx.advanceSavedEvolution('saved-job',3,refreshed);
 assert.equal(calls.length,2);assert.equal(refreshes,1);assert.equal(refreshed.disabled,true);assert.equal(JSON.parse(fields['evolution-report'].innerHTML).status.committed_generations,4);
 console.log('Evolution recovery passed: sequence bound requests, no automatic retry and explicit refresh after uncertainty.');
})().catch(e=>{console.error(e);process.exitCode=1;});
