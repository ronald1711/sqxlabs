const assert=require('node:assert/strict');
const {create}=require('../assets/operation-status.js');
let now=0,view=[],tick,cleared=0;
const status=create(x=>view=x,{now:()=>now,every:f=>(tick=f,1),clear:()=>cleared++});
const first=status.start('Search'),second=status.start('Backtest');
now=2500;tick();assert.equal(view.length,2);assert.match(view[0].message,/2s elapsed/);
status.finish(second,{results:[]});assert.equal(view[0].state,'running');assert.equal(view[1].state,'complete');assert.equal(cleared,0);
status.fail(first,'Network lost');assert.equal(view.find(x=>x.id===first).state,'error');assert.match(view.find(x=>x.id===first).message,/may still be working/);assert.equal(cleared,1);
const third=status.start('Recover');status.finish(third,{recovery_required:true});assert.equal(view.find(x=>x.id===third).state,'attention');
const fourth=status.start('Step');status.finish(fourth,{completed:3,total:20});assert.match(view.find(x=>x.id===fourth).message,/3 \/ 20.*continues/);
const fifth=status.start('Invalid');status.finish(fifth,{error:'No data'});assert.equal(view.find(x=>x.id===fifth).state,'error');
for(let i=0;i<10;i++){const id=status.start('Done');status.finish(id);}
assert.equal(view.length,3,'bounded completed history');
console.log('Operation status: concurrent requests, elapsed time, errors, uncertain replies, partial completion, bounded history passed');
const running=status.start('Still working');status.dismissCompleted();assert.equal(view.length,1);assert.equal(view[0].id,running);assert.equal(view[0].state,'running');
status.finish(running,{completed:10,total:10});assert.equal(view[0].completed,10);assert.equal(view[0].total,10);
status.dismissCompleted();assert.equal(view.length,0);
