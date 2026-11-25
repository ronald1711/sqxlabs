(function(root){
'use strict';
const key='sqxlabs.diagnostics.v1';
const kinds=new Set(['application-error','transport-error','browser-error']);
const areas=new Set(['search','backtest','data','assistant','optimization','portfolio','pipeline','research','other']);
function clean(value){
 if(!value||!kinds.has(value.kind)||!areas.has(value.area)||typeof value.at!=='string'||!/^\d{4}-\d\d-\d\dT[\d:.]+Z$/.test(value.at))return null;
 return {at:value.at,kind:value.kind,area:value.area};
}
function create(storage){
 let rows=[],persistent=true;
 try{const saved=JSON.parse(storage.getItem(key)||'[]');if(Array.isArray(saved))rows=saved.map(clean).filter(Boolean).slice(-100);}catch(_){persistent=false;}
 const save=()=>{try{storage.setItem(key,JSON.stringify(rows));}catch(_){persistent=false;}};
 return {
  add(kind,area){const row=clean({at:new Date().toISOString(),kind,area});if(row){rows.push(row);rows=rows.slice(-100);save();}},
  report(){return {schema:1,scope:'Browser error categories only; no server log, credentials, prompts, strategy or account values. Not a validation certificate.',persistent,events:rows.map(x=>({...x}))};},
  clear(){rows=[];save();}
 };
}
function area(url){const path=String(url).split('?')[0];if(path==='/api/generate')return 'search';if(path.includes('/backtest'))return 'backtest';if(/\/data|\/questdb/.test(path))return 'data';if(path.includes('/assistant'))return 'assistant';if(path.includes('/optimize'))return 'optimization';if(path.includes('/portfolio'))return 'portfolio';if(path.includes('/pipeline'))return 'pipeline';if(/\/research|\/experiments|\/evolution/.test(path))return 'research';return 'other';}
const api={create,area};if(typeof module!=='undefined'&&module.exports)module.exports=api;else root.SqxDiagnostics=api;
})(typeof globalThis!=='undefined'?globalThis:this);
