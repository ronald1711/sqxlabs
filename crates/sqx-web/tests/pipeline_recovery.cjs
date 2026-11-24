const assert=require('node:assert/strict'),fs=require('node:fs'),vm=require('node:vm');
const source=fs.readFileSync(require('node:path').join(__dirname,'../assets/workspace.js'),'utf8');
const start=source.indexOf('async function refreshResearchRuns('),end=source.indexOf("$('research-refresh').onclick",start);
const node=()=>({children:[],append(...children){this.children.push(...children)},replaceChildren(){this.children=[]}});
const nodes={'research-status':node(),'research-runs':node()};
const id='a'.repeat(64);let posts=[],downloads=[],status={status:'incomplete',event_count:4,engine_matches:true},result={matches:true,completion_id:'done',execution_valid:true};
const runs=[{kind:'cli_pipeline_started',run_id:id},{kind:'cli_pipeline_event',run_id:'b'.repeat(64),started_run_id:id},{kind:'cli_pipeline',run_id:'c'.repeat(64),started_run_id:id}].map(r=>({...r,dataset_id:'d'.repeat(64),name:'Pipeline',symbol:'EURUSD'}));
const ctx=vm.createContext({$:id=>nodes[id],document:{createElement:node,createTextNode:text=>({textContent:text})},
 getJSON:async url=>url==='/api/research/runs'?{runs}:status,
 postJSON:async(url,body)=>{posts.push({url,body:JSON.parse(body)});return result;},
 rawBackup:async url=>{downloads.push(url);return '{"exact":9007199254740993}';},
 downloadRawJson:(name,raw)=>downloads.push(raw),announce:message=>{throw Error(message);}
});
vm.runInContext(source.slice(start,end),ctx);
const card=()=>nodes['research-runs'].children[0],button=()=>card().children.at(-1);
(async()=>{
 await ctx.refreshResearchRuns();assert.equal(nodes['research-runs'].children.length,1);
 assert.equal(button().textContent,'Inspect pipeline');
 await card().children[2].onclick();assert.equal(downloads[0],'/api/pipeline/backup?id='+id);assert.equal(downloads[1],'{"exact":9007199254740993}');
 await button().onclick();assert.equal(posts.length,0);assert.equal(button().textContent,'Resume pipeline');assert.match(card().children[1].textContent,/Incomplete/);
 await button().onclick();assert.equal(posts[0].url,'/api/pipeline/replay');assert.equal(posts[0].body.resume,true);assert.match(nodes['research-status'].textContent,/repeatability/);
 status={status:'completion_recorded',event_count:8,engine_matches:true,execution_valid:false};
 await button().onclick();assert.match(card().children[1].textContent,/with errors/);assert.equal(button().textContent,'Verify pipeline replay');
 result={matches:true,completion_id:'done',execution_valid:false};await button().onclick();assert.equal(posts[1].body.resume,false);assert.match(nodes['research-status'].textContent,/invalid/);
 status={status:'incomplete',event_count:4,engine_matches:false};await button().onclick();assert.equal(button().disabled,true);assert.match(button().textContent,/Original application build/);
 await ctx.refreshResearchRuns();status={error:'conflicting events'};await button().onclick();assert.equal(posts.length,2);assert.match(nodes['research-status'].textContent,/conflicting events/);assert.equal(button().textContent,'Inspect pipeline');
 console.log('Pipeline dashboard recovery: grouped records, exact backup bytes, explicit inspection, resume/replay, build mismatch and errors passed.');
})().catch(error=>{console.error(error);process.exitCode=1;});
