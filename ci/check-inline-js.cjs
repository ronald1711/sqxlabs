// Compile executable inline scripts without running application code.
const fs=require('node:fs');
const vm=require('node:vm');
const file='crates/sqx-web/assets/index.html';
const html=fs.readFileSync(file,'utf8');
let checked=0;
for(const match of html.matchAll(/<script\b([^>]*)>([\s\S]*?)<\/script\s*>/gi)){
  const attributes=match[1];
  if(/\bsrc\s*=/i.test(attributes))continue;
  const type=attributes.match(/\btype\s*=\s*["']([^"']+)["']/i)?.[1];
  if(type==='application/json'){JSON.parse(match[2]);continue;}
  if(type && !['text/javascript','application/javascript'].includes(type))throw new Error('Unhandled script type: '+type);
  new vm.Script(match[2],{filename:file+':inline-'+checked});checked++;
}
if(!checked)throw new Error('No inline scripts were checked');
console.log('Inline JavaScript syntax passed ('+checked+' script blocks).');
