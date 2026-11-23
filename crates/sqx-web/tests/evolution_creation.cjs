const fs=require('node:fs'),vm=require('node:vm'),assert=require('node:assert/strict');
const source=fs.readFileSync('crates/sqx-web/assets/workspace.js','utf8'),start=source.indexOf('async function createSavedEvolution()'),end=source.indexOf("$('evolution-create').onclick",start);assert(start>=0&&end>start);
const fields=Object.fromEntries(['create','status','seed','population','generations','min-trades'].map(id=>['evolution-'+id,{value:({'seed':'42',population:'6',generations:'1','min-trades':'2'})[id]}]));
const storage=new Map();let posts=[],jobs=[],refreshes=0;
const context={$:id=>fields[id],localStorage:{getItem:k=>storage.get(k)||null,setItem:(k,v)=>storage.set(k,v),removeItem:k=>storage.delete(k)},crypto:{randomUUID:()=> 'request-1'},selectedSymbol:()=> 'EURUSD',settings:()=>({deposit:4321}),genSettings:()=>({long:true}),getJSON:async()=>({jobs}),postJSON:async(path,raw)=>{posts.push(raw);jobs=[{metadata:{request_key:'request-1'},status:{job_id:'saved-id'}}];throw Error('lost response');},refreshEvolutionJobs:async()=>{refreshes++;}};
const ctx=vm.createContext(context);vm.runInContext(source.slice(start,end),ctx);
(async()=>{
 fields['evolution-seed'].value='';await ctx.createSavedEvolution();assert.equal(posts.length,0);assert.equal(storage.size,0);
 fields['evolution-seed'].value='42';await ctx.createSavedEvolution();assert.equal(posts.length,1);assert.equal(storage.size,1);assert.match(fields['evolution-status'].textContent,/may already exist/);
 fields['evolution-seed'].value='99';context.settings=()=>{throw Error('live settings changed');};
 await ctx.createSavedEvolution();assert.equal(posts.length,1);assert.equal(storage.size,0);assert.equal(refreshes,1);assert.equal(JSON.parse(posts[0]).seed,42);assert.equal(JSON.parse(posts[0]).deposit,4321);assert.equal(fields['evolution-create'].disabled,false);
 console.log('Evolution creation passed: invalid input sends nothing; lost response recovers the same saved request despite changed live settings.');
})().catch(e=>{console.error(e);process.exitCode=1;});
