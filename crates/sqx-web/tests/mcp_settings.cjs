const assert=require('node:assert/strict'),fs=require('node:fs'),vm=require('node:vm');
const elements=new Map();function el(id){if(!elements.has(id))elements.set(id,{value:'',disabled:false,textContent:'',addEventListener(){}});return elements.get(id);}
const profile={schema:1,executable:'/app/sqx-mcp',working_directory:'/work',research_directory:'/work/archive',log_directory:'/work/logs',log_max_bytes:4096,log_max_files:3,log_retention_days:7,source:{kind:'request'}};
let saved;
const context={document:{getElementById:el},getJSON:async()=>({saved:false,config:profile}),postJSON:async(path,body)=>{saved=JSON.parse(body);return {saved:true,configuration_id:'a'.repeat(64),config:saved,client_config:{mcpServers:{}}};},navigator:{clipboard:{writeText:async()=>{}}},downloadJson(){}};
vm.runInNewContext(fs.readFileSync('crates/sqx-web/assets/mcp-settings.js','utf8'),context);
(async()=>{await el('mcp-reload').onclick();await el('mcp-save').onclick();assert.deepEqual(saved,profile);assert.equal(typeof saved.log_directory,'string');assert.equal(el('mcp-probe').disabled,false);assert.match(el('mcp-status').textContent,/Running process: unknown/);console.log('MCP profile form preserves paths and numeric limits.');})().catch(e=>{console.error(e);process.exitCode=1;});
