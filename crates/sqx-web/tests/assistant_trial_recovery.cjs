const assert=require('node:assert/strict'),fs=require('node:fs'),vm=require('node:vm');
const source=fs.readFileSync(require('node:path').join(__dirname,'../assets/workspace.js'),'utf8');
const start=source.indexOf('async function refreshResearchRuns('),end=source.indexOf("$('research-refresh').onclick",start);
assert(start>=0&&end>start);
const node=()=>({children:[],append(...children){this.children.push(...children)},replaceChildren(){this.children=[]}});
const nodes={'research-status':node(),'research-runs':node()};
let result,posted,fail=false;
const kinds=['assistant_trial_started','assistant_trial','assistant_candidates'];
const ctx=vm.createContext({
 $:id=>nodes[id],document:{createElement:node,createTextNode:text=>({textContent:text})},
 getJSON:async()=>({runs:kinds.map((kind,i)=>({kind,run_id:String(i).repeat(64),dataset_id:'d'.repeat(64),name:'Fixture',symbol:'EURUSD'}))}),
 postJSON:async(url,body)=>{posted={url,body:JSON.parse(body)};if(fail)throw new Error('connection lost');return result;},
});
vm.runInContext(source.slice(start,end),ctx);
const button=i=>nodes['research-runs'].children[i].children.at(-1);
(async()=>{
 await ctx.refreshResearchRuns();
 assert.equal(button(0).textContent,'Restart frozen trial');assert.match(button(0).title,/Does not resume/);
 assert.equal(button(1).textContent,'Verify replay');assert.equal(button(2).disabled,true);
 result={resumed:true,run_id:'r'.repeat(64),execution_valid:true};await button(0).onclick();
 assert.equal(posted.body.resume,true);assert.match(nodes['research-status'].textContent,/restarted and saved/);
 result={matches:true,execution_valid:false};await button(1).onclick();
 assert.equal(posted.body.resume,undefined);assert.match(nodes['research-status'].textContent,/remains invalid/);
 result={resumed:true,run_id:'r'.repeat(64),execution_valid:false};await button(0).onclick();
 assert.match(nodes['research-status'].textContent,/saved with execution errors/);
 result={resumed:true};await button(0).onclick();assert.match(nodes['research-status'].textContent,/did not confirm/);
 fail=true;await button(1).onclick();assert.match(nodes['research-status'].textContent,/connection lost/);assert.equal(button(1).disabled,false);
 console.log('AI trial recovery UI passed: explicit restart, replay, invalid results, missing receipt and transport failure.');
})().catch(error=>{console.error(error);process.exitCode=1;});
