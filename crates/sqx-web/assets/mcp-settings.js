(function(){
'use strict';
const el=id=>document.getElementById(id);
let savedId=null,clientConfig=null,busy=false;
const fields={executable:'mcp-executable',working_directory:'mcp-working',research_directory:'mcp-research',log_directory:'mcp-logs',log_max_bytes:'mcp-log-bytes',log_max_files:'mcp-log-files',log_retention_days:'mcp-log-days'};
function buttons(){Object.values(fields).concat(['mcp-source','mcp-csv','mcp-symbol','mcp-timeframe','mcp-questdb-host','mcp-questdb-port']).forEach(id=>el(id).disabled=busy);el('mcp-save').disabled=busy;el('mcp-reload').disabled=busy;el('mcp-probe').disabled=busy||!savedId;el('mcp-copy').disabled=busy||!clientConfig;}
function dirty(){savedId=null;clientConfig=null;el('mcp-client-config').textContent='';el('mcp-status').textContent='Unsaved changes. Save before testing or copying. Existing MCP processes are unchanged.';buttons();}
Object.values(fields).concat(['mcp-source','mcp-csv','mcp-symbol','mcp-timeframe','mcp-questdb-host','mcp-questdb-port']).forEach(id=>el(id).addEventListener('input',dirty));
function fill(response){
 if(response.error)throw Error(response.error);
 const config=response.config;Object.entries(fields).forEach(([key,id])=>el(id).value=config[key]);
 el('mcp-source').value=config.source.kind;el('mcp-csv').value=config.source.path||'';el('mcp-symbol').value=config.source.symbol||'';el('mcp-timeframe').value=config.source.timeframe||'';el('mcp-questdb-host').value=config.source.host||'';el('mcp-questdb-port').value=config.source.port||9000;
 savedId=response.configuration_id||null;clientConfig=response.client_config||null;
 el('mcp-client-config').textContent=clientConfig?JSON.stringify(clientConfig,null,2):'';
 const r=response.runtime||{process_status:'unknown',active_processes:[]};
 const active=r.active_processes||[];const external=active.filter(p=>p.initialized&&p.client_kind!=='probe');
 el('mcp-status').textContent=(response.saved?'Saved profile loaded. ':'Suggested paths loaded; validate and save first. ')+(response.probe?'Last configuration test: '+response.probe.handshake+'. Retest after changing the executable. ':'Configuration not tested. ')+
 '\nRunning process: '+r.process_status+'. '+active.map(p=>'PID '+p.pid+' · '+p.client_kind+' · '+(p.initialized?'handshake observed':'handshake not observed')+(p.logging_degraded?' · LOGGING DEGRADED':'')).join('; ')+
 '\nExternal client handshake: '+(external.length?'observed (self-declared client; not authenticated).':'unknown.')+' '+(r.error||r.scope||'');
}
function read(){
 const config={schema:1};Object.entries(fields).forEach(([key,id])=>config[key]=['log_max_bytes','log_max_files','log_retention_days'].includes(key)?Number(el(id).value):el(id).value.trim());
 const kind=el('mcp-source').value;config.source={kind};if(kind!=='request'){config.source.symbol=el('mcp-symbol').value.trim();config.source.timeframe=el('mcp-timeframe').value.trim();}if(kind==='csv')config.source.path=el('mcp-csv').value.trim();if(kind==='questdb'){config.source.host=el('mcp-questdb-host').value.trim();config.source.port=Number(el('mcp-questdb-port').value);}return config;
}
async function perform(action){if(busy)return;busy=true;buttons();try{await action();}catch(e){el('mcp-status').textContent='MCP operation failed: '+e.message;}finally{busy=false;buttons();}}
el('mcp-reload').onclick=()=>perform(async()=>fill(await getJSON('/api/mcp/config')));
el('mcp-save').onclick=()=>perform(async()=>fill(await postJSON('/api/mcp/config',JSON.stringify(read()))));
el('mcp-probe').onclick=()=>perform(async()=>{
 el('mcp-status').textContent='Testing saved executable: start → handshake → tools list. No research tools are called.';
 const result=await postJSON('/api/mcp/probe',JSON.stringify({configuration_id:savedId}));if(result.error)throw Error(result.error);
 el('mcp-status').textContent='Handshake passed · '+result.tool_count+' tools · test process exited. '+(result.logging_warning?'Warning: MCP logging unavailable. ':'')+(result.process_status_warning?'Warning: live process status unavailable. ':'')+(result.log_history_repaired?'Warning: log history was repaired and may be incomplete. ':'')+'External Claude/ChatGPT connection: unknown. No research was executed.';
});
el('mcp-copy').onclick=()=>perform(async()=>{await navigator.clipboard.writeText(JSON.stringify(clientConfig,null,2));el('mcp-status').textContent='Client configuration copied. Add it to your client MCP settings; this does not establish a connection.';});

let logReport=null;
['mcp-log-since','mcp-log-tool','mcp-log-session','mcp-log-status'].forEach(id=>el(id).addEventListener('input',()=>{logReport=null;el('mcp-log-download').disabled=true;el('mcp-log-notice').textContent='Filters changed. Read logs again to update the report.';}));
el('mcp-log-refresh').onclick=async function(){
 this.disabled=true;['mcp-log-since','mcp-log-tool','mcp-log-session','mcp-log-status'].forEach(id=>el(id).disabled=true);logReport=null;el('mcp-log-download').disabled=true;el('mcp-log-notice').textContent='Reading bounded MCP log snapshot…';
 try{const filters={};const since=el('mcp-log-since').value;if(since){const time=Date.parse(since+'Z');if(!Number.isFinite(time))throw Error('Invalid UTC time');filters.since_utc_ms=time;}
 for(const [key,id] of [['tool','mcp-log-tool'],['session','mcp-log-session'],['status','mcp-log-status']]){if(el(id).value.trim())filters[key]=el(id).value.trim();}
 const report=await postJSON('/api/mcp/logs',JSON.stringify(filters));if(report.error)throw Error(report.error);
 logReport=report;el('mcp-log-download').disabled=false;el('mcp-log-notice').textContent=report.events.length+' events · invalid records: '+report.invalid_records+(report.truncated?' · LIMITED SNAPSHOT':'')+'. '+report.scope+' '+report.event_scope+' '+report.connection_scope+' Observed handshakes: '+(report.observed_handshakes||[]).map(h=>h.client_kind+' at '+new Date(h.observed_at_utc_ms).toISOString()).join('; ');
 el('mcp-log-events').textContent=report.events.length?report.events.map(e=>new Date(e.utc_unix_ms).toISOString()+' · '+e.session+' / '+e.request_sequence+' · '+(e.tool||e.method)+' · '+e.status+' · '+e.duration_ms+'ms').join('\n'):'No matching events. This does not prove that no MCP activity occurred.';
 }catch(e){el('mcp-log-notice').textContent='Could not read MCP logs: '+e.message;el('mcp-log-events').textContent='';}finally{this.disabled=false;['mcp-log-since','mcp-log-tool','mcp-log-session','mcp-log-status'].forEach(id=>el(id).disabled=false);}
};
el('mcp-log-download').onclick=()=>{if(logReport)downloadJson('sqxlabs-mcp-diagnostics.json',logReport);};
})();
