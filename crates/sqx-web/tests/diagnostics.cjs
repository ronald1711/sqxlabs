const assert=require('node:assert/strict');const {create,area}=require('../assets/diagnostics.js');
let raw=null;const store={getItem:()=>raw,setItem:(_,v)=>raw=v};let d=create(store);
d.add('application-error',area('/api/assistant?key=SECRET'));assert.equal(d.report().events[0].area,'assistant');assert(!JSON.stringify(d.report()).includes('SECRET'));
for(let i=0;i<120;i++)d.add('transport-error','data');assert.equal(d.report().events.length,100);
d=create(store);assert.equal(d.report().events.length,100);d.clear();assert.equal(create(store).report().events.length,0);
raw=JSON.stringify([{at:new Date().toISOString(),kind:'application-error',area:'search',key:'SECRET',message:'SECRET'}]);d=create(store);assert(!JSON.stringify(d.report()).includes('SECRET'));
raw='broken';assert.equal(create(store).report().persistent,false);
const denied=create({getItem(){throw Error('denied')},setItem(){throw Error('denied')}});denied.add('browser-error','other');assert.equal(denied.report().events.length,1);assert.equal(denied.report().persistent,false);
console.log('Diagnostics: bounded persistence, secret exclusion, untrusted stored values, malformed and unavailable storage passed');
