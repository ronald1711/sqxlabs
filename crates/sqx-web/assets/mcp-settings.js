(function(){
'use strict';
const el=id=>document.getElementById(id);
let savedId=null,clientConfig=null,busy=false;
const fields={executable:'mcp-executable',working_directory:'mcp-working',research_directory:'mcp-research',log_directory:'mcp-logs',log_max_bytes:'mcp-log-bytes',log_max_files:'mcp-log-files',log_retention_days:'mcp-log-days'};
function buttons(){Object.values(fields).concat(['mcp-source','mcp-csv','mcp-symbol','mcp-timeframe']).forEach(id=>el(id).disabled=busy);el('mcp-save').disabled=busy;el('mcp-reload').disabled=busy;el('mcp-probe').disabled=busy||!savedId;el('mcp-copy').disabled=busy||!clientConfig;}
function dirty(){savedId=null;clientConfig=null;el('mcp-client-config').textContent='';el('mcp-status').textContent='Unsaved changes. Save before testing or copying. Existing MCP processes are unchanged.';buttons();}
Object.values(fields).concat(['mcp-source','mcp-csv','mcp-symbol','mcp-timeframe']).forEach(id=>el(id).addEventListener('input',dirty));
function fill(response){
 if(response.error)throw Error(response.error);
 const config=response.config;Object.entries(fields).forEach(([key,id])=>el(id).value=config[key]);
 el('mcp-source').value=config.source.kind;el('mcp-csv').value=config.source.path||'';el('mcp-symbol').value=config.source.symbol||'';el('mcp-timeframe').value=config.source.timeframe||'';
 savedId=response.configuration_id||null;clientConfig=response.client_config||null;
 el('mcp-client-config').textContent=clientConfig?JSON.stringify(clientConfig,null,2):'';
 el('mcp-status').textContent=(response.saved?'Saved profile loaded. ':'Suggested paths loaded; validate and save first. ')+(response.probe?'Last test: '+response.probe.handshake+'. Retest after changing the executable. ':'Configuration not tested. ')+'Running process: unknown. External client connection: unknown.';
}
function read(){
 const config={schema:1};Object.entries(fields).forEach(([key,id])=>config[key]=key.startsWith('log_')?Number(el(id).value):el(id).value.trim());
 const kind=el('mcp-source').value;config.source={kind};if(kind!=='request'){config.source.symbol=el('mcp-symbol').value.trim();config.source.timeframe=el('mcp-timeframe').value.trim();}if(kind==='csv')config.source.path=el('mcp-csv').value.trim();return config;
}
async function perform(action){if(busy)return;busy=true;buttons();try{await action();}catch(e){el('mcp-status').textContent='MCP operation failed: '+e.message;}finally{busy=false;buttons();}}
el('mcp-reload').onclick=()=>perform(async()=>fill(await getJSON('/api/mcp/config')));
el('mcp-save').onclick=()=>perform(async()=>fill(await postJSON('/api/mcp/config',JSON.stringify(read()))));
el('mcp-probe').onclick=()=>perform(async()=>{
 el('mcp-status').textContent='Testing saved executable: start → handshake → tools list. No research tools are called.';
 const result=await postJSON('/api/mcp/probe',JSON.stringify({configuration_id:savedId}));if(result.error)throw Error(result.error);
 el('mcp-status').textContent='Handshake passed · '+result.tool_count+' tools · test process exited. '+(result.logging_warning?'Warning: MCP logging unavailable. ':'')+(result.log_history_repaired?'Warning: log history was repaired and may be incomplete. ':'')+'External Claude/ChatGPT connection: unknown. No research was executed.';
});
el('mcp-copy').onclick=()=>perform(async()=>{await navigator.clipboard.writeText(JSON.stringify(clientConfig,null,2));el('mcp-status').textContent='Client configuration copied. Add it to your client MCP settings; this does not establish a connection.';});
})();
