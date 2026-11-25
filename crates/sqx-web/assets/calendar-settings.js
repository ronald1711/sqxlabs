/* Dataset review and explicit calendar editing. No inferred market closures. */
(function(){
'use strict';
const esc=escapeHtml, $=id=>document.getElementById(id);
const utc=t=>new Date(t).toISOString().replace('T',' ').replace('.000Z',' UTC');
function weekendHint(r,g){
 // Descriptive hint only: never changes the recorded gap classification or gate.
 // The UTC interpretation is the dataset's declaration, not independent clock proof.
 const currencies=['EUR','USD','GBP','JPY','CHF','CAD','AUD','NZD'];
 const symbol=r.symbol||'';
 if(symbol.length!==6||!currencies.includes(symbol.slice(0,3))||!currencies.includes(symbol.slice(3)))return false;
 const length=g.end_time-g.start_time;
 if(g.kind!=='unknown'||length<86400000||length>4*86400000)return false;
 for(let t=Math.floor(g.start_time/86400000)*86400000;t<g.end_time;t+=86400000){if(new Date(t).getUTCDay()===6)return true;}
 return false;
}
function summary(r){
 const q=r.data_quality||{}, f=r.calendar?.frozen, source=r.source;
 const status={assumed_market_calendar:'Standaard marktkalender — vastgelegde aanname, niet brongeverifieerd',documented_binding:'Brongebonden kalender met vastgelegde referenties',unconfirmed_binding:'Kalender of bron nog onbevestigd',legacy_declared_utc:'Oude, handmatig opgegeven UTC-kalender',unknown:'Kalender onbekend'}[q.calendar_status]||'Kalender onbekend';
 const weekends=(q.gaps||[]).filter(g=>weekendHint(r,g));
 const kinds={scheduled_closed:'Verwachte marktsluiting',missing_open:'Ontbrekende data tijdens handelsuren',unknown:'Onbekend'};
 return `<p><b>${esc(r.symbol||'')} · ${esc(r.timeframe||'')}</b> — ${esc(status)}</p>
 <p>Oorspronkelijke bron: ${esc(source?.source||'onbekend')}. Bron ${source?.source_confirmed?'bevestigd':'niet bevestigd'}; UTC-tijdstempels ${source?.utc_timestamps_confirmed?'bevestigd':'niet bevestigd'}.</p>
 ${weekends.length?`<p><b>${weekends.length} onderbreking(en) passen bij weekendsluiting</b> (${weekends.reduce((n,g)=>n+g.intervals,0)} candle-intervallen). De exacte grenzen zijn nog niet bevestigd. Deze aanwijzing gebruikt de opgegeven UTC-klok en het weekendpatroon; de formele beoordeling blijft onbekend. Het zijn geen duizenden afzonderlijke storingen.</p>`:''}
 <ul><li>Verwacht gesloten: <b>${q.scheduled_closed_intervals||0}</b> candle-intervallen — blokkeren zoeken niet.</li><li>Ontbrekend tijdens handelsuren: <b>${q.missing_open_intervals||0}</b>.</li><li>Onbekend: <b>${q.unclassified_intervals||0}</b> — eerst bron, kalender of tijdzone controleren.</li><li>Onregelmatige tijdstappen: ${q.irregular_steps||0}; onvolledige candles: ${q.incomplete_bars||0}.</li></ul>
 ${f?`<p>Kalender: ${esc(f.profile.name)} · versie ${esc(f.profile.version)} · ${esc(f.profile.timezone)}. Beoordelingsbereik ${utc(f.coverage_start)} tot ${utc(f.coverage_end)} (einde exclusief). Buiten dit bereik blijft de beoordeling onbekend.</p>`:''}
 ${(q.errors||[]).length?`<p role="alert">Niet klaar voor onderzoek: ${q.errors.map(esc).join('; ')}</p>`:''}
 <details><summary>Details bekijken: tijdstippen en bewijs</summary><p>Alle tijden hieronder zijn UTC. Het einde is exclusief; aantallen zijn ontbrekende candle-intervallen, geen afzonderlijke storingen.</p>
 <div style="overflow:auto;max-height:300px"><table><tr><th>Begin</th><th>Einde</th><th>Aantal</th><th>Beoordeling</th></tr>${(q.gaps||[]).map(g=>`<tr><td>${utc(g.start_time)}</td><td>${utc(g.end_time)}</td><td>${g.intervals}</td><td>${kinds[g.kind]||esc(g.kind)}${weekendHint(r,g)?' — past bij weekendsluiting; exacte grenzen nog niet bevestigd':''}</td></tr>`).join('')}</table></div>
 ${q.gap_details_truncated?'<p>Alleen de eerste 200 groepen getoond; de totaaltellingen zijn volledig binnen de controlelimiet.</p>':''}
 <p>Dataset: ${esc(r.dataset_id||'')}. Controle: ${r.as_of_ms?utc(r.as_of_ms):'onbekend'}.</p>
 ${f?`<p>Referenties (door de gebruiker opgegeven): ${f.profile.references.map(esc).join(' · ')}</p><p>${esc(f.profile.notes)}</p>`:''}</details>
 <p class="muted">Geen candles worden aangevuld. Deze controle bewijst niet dat de data volledig zijn of dat een strategie betrouwbaar is.</p>`;
}
window.sqxDataReviewHtml=summary;
async function inspect(){const d=await postJSON('/api/data/review',JSON.stringify({symbol:selectedSymbol()}));if(d.error)throw Error(d.error);return d;}
window.reviewGenerationData=async function(body){
 const r=await inspect();
 return new Promise(resolve=>{
  const dialog=document.createElement('dialog');dialog.style.cssText='max-width:850px;width:90%;max-height:85vh;overflow:auto;background:var(--panel);color:var(--fg);border:1px solid var(--line);border-radius:12px;padding:24px';
  dialog.innerHTML='<h2>Data controleren vóór het zoeken</h2>'+summary(r)+
   (r.unresolved?'<p><b>Verkennend doorgaan</b> betekent dat resultaten door ontbrekende of onbekende intervallen vertekend kunnen zijn. Deze keuze geldt alleen voor deze dataset en zoekopdracht.</p>':'<p>Er zijn geen onopgeloste gaten gevonden binnen deze beoordeling.</p>')+
   '<div class="row"><button data-action="cancel" class="secondary">Terug</button><button data-action="calendar" class="secondary">Bron en kalender controleren</button>'+(!r.blocked?'<button data-action="continue">'+(r.unresolved?'Verkennend doorgaan':'Zoeken starten')+'</button>':'')+'</div>';
  let finished=false;function finish(value){if(finished)return;finished=true;dialog.close();dialog.remove();resolve(value);}
  dialog.addEventListener('cancel',e=>{e.preventDefault();finish(null);});
  dialog.querySelector('[data-action="cancel"]').onclick=()=>finish(null);
  dialog.querySelector('[data-action="calendar"]').onclick=()=>{finish(null);document.querySelector('[data-tab="data"]').click();$('calendar-editor').scrollIntoView({behavior:'smooth'});load();};
  const go=dialog.querySelector('[data-action="continue"]');if(go)go.onclick=()=>finish({dataset_id:r.dataset_id,as_of_ms:r.as_of_ms,data_quality:r.data_quality,exploratory_consent:r.unresolved});
  document.body.appendChild(dialog);dialog.showModal();
 });
};
const card=document.createElement('div');card.id='calendar-editor';card.className='card';card.style.marginTop='18px';
card.innerHTML=`<h2>Bron en handelskalender</h2><p>Voor de geselecteerde dataset. Kies alleen bevestigde handelsuren met een bronreferentie. Leid sluitingen niet af uit ontbrekende candles. Bij gemengde of onbekende herkomst laat je de bevestigingen uit.</p>
<button id="cal-load" class="secondary">Geselecteerde dataset controleren</button><button id="cal-forex" class="secondary">Standaard forexweek invullen</button><p>Standaard forexweek: zondag 17:00 tot vrijdag 17:00 in New York, met Amerikaanse zomer-/wintertijd. Dit is een expliciete onderzoeksaanname, geen bevestiging van jouw databron. Dagelijkse brokerpauzes en feestdagen zijn hierin niet opgenomen. Bekijk de beoordeling vóór opslaan.</p><button id="cal-refetch" class="secondary" disabled>Zelfde periode opnieuw uit QuestDB ophalen</button><p class="muted">Opnieuw ophalen maakt een voorstel uit de huidige QuestDB-verbinding. Bestaande data blijven geladen totdat je het voorstel accepteert. Dit kan upstream ontbrekende data niet herstellen en bevestigt de oorspronkelijke bron niet.</p><div id="cal-repair"></div><div id="cal-report" role="status"></div>
<label>Oorspronkelijke koersbron <input type="text" id="cal-source" value="unknown" placeholder="unknown, mixed of bronnaam"></label>
<label><input type="checkbox" id="cal-source-confirmed"> De bron van deze koersen is bevestigd</label>
<label><input type="checkbox" id="cal-utc"> Ik verklaar dat de candle-tijdstempels UTC voorstellen (niet onafhankelijk geverifieerd)</label>
<label><input type="checkbox" id="cal-enabled"> Een expliciete handelskalender gebruiken</label>
<fieldset id="cal-fields" hidden disabled><legend>Kalender — lokale openingstijden</legend>
<div class="row"><label>Naam <input type="text" id="cal-name"></label><label>Identificatie <input type="text" id="cal-id" placeholder="mijn-bron-eurusd"></label><label>Versie <input type="text" id="cal-version" value="1"></label></div>
<div class="row"><label>Tijdzone (IANA) <input type="text" id="cal-zone" placeholder="Europe/Amsterdam"></label><label>Extra klokverschuiving in minuten <input type="number" id="cal-shift" value="0" min="-1440" max="1440"></label></div>
<p class="muted">Zomer-/wintertijd volgt de gekozen tijdzone. Laat klokverschuiving op 0, tenzij een brokerspecificatie een afwijking voorschrijft.</p>
<div class="row"><label>Geldig vanaf <input type="date" id="cal-from"></label><label>Geldig tot (exclusief) <input type="date" id="cal-until"></label><label>Bron gecontroleerd op <input type="date" id="cal-reviewed"></label></div>
<p>Openingstijden per dag, bijvoorbeeld 00:00-12:00, 12:05-24:00 voor een dagelijkse pauze. Leeg betekent gesloten. Splits een sessie over middernacht over twee dagen.</p>
<div id="cal-week"></div>
<label>Uitzonderingen: één datum per regel, bijvoorbeeld 2026-12-25=closed of 2026-12-24=00:00-13:00<textarea id="cal-exceptions" rows="3"></textarea></label>
<label>Referenties naar bron- of brokerspecificaties (één per regel)<textarea id="cal-references" rows="2"></textarea></label>
<label>Toelichting en resterende aannames<textarea id="cal-notes" rows="2"></textarea></label>
<label><input type="checkbox" id="cal-market-assumption"> Deze kalender bewust als algemene marktaanname gebruiken, ook bij onbekende bron</label><label><input type="checkbox" id="cal-documented"> Deze regels en uitzonderingen zijn onderbouwd door de opgegeven referenties voor dit geldigheidsbereik</label></fieldset>
<label><input type="checkbox" id="cal-questdb"> Ook gebruiken bij volgende QuestDB-imports van dit instrument, op deze verbinding, voor alle tijdframes</label>
<div class="row"><button id="cal-preview" class="secondary">Beoordeling vooraf bekijken</button><button id="cal-save">Opslaan als nieuwe datasetversie</button></div><p id="cal-status" role="status"></p>`;
$('tab-data').querySelector('.card').insertAdjacentElement('afterend',card);
const weekdays=['Maandag','Dinsdag','Woensdag','Donderdag','Vrijdag','Zaterdag','Zondag'];
$('cal-week').innerHTML=weekdays.map((d,i)=>`<label style="display:block">${d} <input type="text" id="cal-day-${i}" placeholder="gesloten"></label>`).join('');
let current=null;
const clock=m=>String(Math.floor(m/60)).padStart(2,'0')+':'+String(m%60).padStart(2,'0');
const ranges=rs=>rs.map(([a,b])=>clock(a)+'-'+clock(b)).join(', ');
function parseRanges(text){if(!text.trim()||text.trim()==='closed')return [];return text.split(',').map(part=>{const m=part.trim().match(/^(\d{2}):(\d{2})-(\d{2}):(\d{2})$/);if(!m)throw Error('Gebruik tijdvakken als 00:00-12:00, 12:05-24:00.');const [h,a,j,b]=m.slice(1).map(Number);if(h>23||j>24||a>59||b>59||(j===24&&b!==0))throw Error('Ongeldig tijdstip.');const start=h*60+a,end=j*60+b;if(start>=end)throw Error('Het einde moet na het begin liggen. Splits sessies over middernacht.');return [start,end];});}
async function load(){try{
 current=await inspect();$('cal-report').innerHTML=summary(current);
 const source=current.source||{source:'unknown'},p=current.calendar?.frozen?.profile;
 $('cal-refetch').disabled=!current.questdb_available;$('cal-repair').textContent='';
 $('cal-source').value=source.source;$('cal-source-confirmed').checked=!!source.source_confirmed;$('cal-utc').checked=!!source.utc_timestamps_confirmed;
 $('cal-enabled').checked=!!p;$('cal-fields').disabled=!p;$('cal-fields').hidden=!p;$('cal-market-assumption').checked=p?.evidence==='market_assumption';$('cal-questdb').checked=false;$('cal-questdb').disabled=!current.questdb_available;
 for(const [id,key] of [['name','name'],['id','id'],['version','version'],['zone','timezone'],['shift','clock_shift_minutes'],['from','valid_from'],['until','valid_until'],['reviewed','reviewed_on'],['notes','notes']])$('cal-'+id).value=p?.[key]??({version:'1',shift:0}[id]??'');
 $('cal-references').value=(p?.references||[]).join('\n');$('cal-documented').checked=p?.evidence==='documented';
 weekdays.forEach((_,i)=>$('cal-day-'+i).value=ranges((p?.weekly||[]).filter(s=>s.weekday===i).map(s=>[s.start_minute,s.end_minute])));
 $('cal-exceptions').value=Object.entries(p?.date_overrides||{}).map(([day,rs])=>day+'='+(ranges(rs)||'closed')).join('\n');
 $('cal-status').textContent=current.calendar&&!p?'Deze dataset gebruikt een oude UTC-kalender. Alleen opslaan als je deze bewust wilt vervangen.':'';
}catch(e){$('cal-status').textContent=e.message;}}
$('cal-load').onclick=load;$('cal-enabled').onchange=()=>{$('cal-fields').disabled=!$('cal-enabled').checked;$('cal-fields').hidden=!$('cal-enabled').checked;};
function selection(){
 const binding={source:$('cal-source').value.trim(),instrument:current.symbol,source_confirmed:$('cal-source-confirmed').checked,utc_timestamps_confirmed:$('cal-utc').checked};
 let profile=null;if($('cal-enabled').checked){const overrides={};for(const line of $('cal-exceptions').value.split('\n').filter(s=>s.trim())){const pair=line.split('=');if(pair.length!==2||!/^\d{4}-\d{2}-\d{2}$/.test(pair[0].trim()))throw Error('Gebruik voor uitzonderingen YYYY-MM-DD=tijdvakken of YYYY-MM-DD=closed.');if(Object.hasOwn(overrides,pair[0].trim()))throw Error('Dubbele uitzonderingsdatum.');overrides[pair[0].trim()]=parseRanges(pair[1]);}
 profile={schema:1,id:$('cal-id').value.trim(),version:$('cal-version').value.trim(),name:$('cal-name').value.trim(),source:binding.source,instruments:[current.symbol],timezone:$('cal-zone').value.trim(),clock_shift_minutes:Number($('cal-shift').value),weekly:weekdays.flatMap((_,i)=>parseRanges($('cal-day-'+i).value).map(([a,b])=>({weekday:i,start_minute:a,end_minute:b}))),date_overrides:overrides,valid_from:$('cal-from').value,valid_until:$('cal-until').value,reviewed_on:$('cal-reviewed').value,evidence:$('cal-market-assumption').checked?'market_assumption':($('cal-documented').checked?'documented':'provisional'),references:$('cal-references').value.split('\n').map(s=>s.trim()).filter(Boolean),notes:$('cal-notes').value};}
 return {binding,profile};
}
async function apply(action){try{
 if(!current)throw Error('Controleer eerst de geselecteerde dataset.');
 const result=await postJSON('/api/calendar/'+action,JSON.stringify({symbol:selectedSymbol(),dataset_id:current.dataset_id,selection:selection(),use_for_questdb_imports:$('cal-questdb').checked}));if(result.error)throw Error(result.error);
 $('cal-report').innerHTML=summary(result);$('cal-status').textContent=action==='save'?'Opgeslagen. Eerdere datasets en onderzoeksruns blijven ongewijzigd.':'Voorbeeld; nog niet opgeslagen.';
 if(action==='save'){current=result;await refreshDataInfo();}
}catch(e){$('cal-status').textContent=e.message;}}
$('cal-refetch').onclick=async()=>{try{
 if(!current)throw Error('Controleer eerst de geselecteerde dataset.');
 $('cal-repair').textContent='Dezelfde periode wordt opnieuw uit QuestDB gelezen…';
 const result=await postJSON('/api/calendar/refetch',JSON.stringify({symbol:selectedSymbol(),dataset_id:current.dataset_id}));if(result.error)throw Error(result.error);
 const c=result.changes;$('cal-repair').innerHTML=`<h3>Voorstel — nog niet geladen</h3><p>${c.added} candles toegevoegd, ${c.removed} niet meer aanwezig, ${c.revised} gewijzigd. Geen candles zijn gefabriceerd.</p>`+summary(result)+'<button id="cal-repair-accept">Dit voorstel laden</button>';
 $('cal-repair-accept').onclick=async()=>{try{const loaded=await postJSON('/api/data/load',JSON.stringify({id:result.snapshot_id}));if(loaded.error)throw Error(loaded.error);await loadInfo();await load();}catch(e){$('cal-status').textContent=e.message;}};
 }catch(e){$('cal-repair').textContent=e.message;}};
$('cal-forex').onclick=async()=>{
 await load();if(!current)return;
 const currencies=['EUR','USD','GBP','JPY','CHF','CAD','AUD','NZD'];
 if(current.symbol.length!==6||!currencies.includes(current.symbol.slice(0,3))||!currencies.includes(current.symbol.slice(3))){$('cal-status').textContent='Deze standaard is alleen bedoeld voor valutaparen, niet voor crypto of andere markten.';return;}
 $('cal-enabled').checked=true;$('cal-fields').disabled=false;$('cal-fields').hidden=false;
 $('cal-id').value='standard-fx-week';$('cal-name').value='Standaard forexweek (aanname)';$('cal-version').value='1';$('cal-zone').value='America/New_York';$('cal-shift').value='0';
 $('cal-from').value='2026-01-01';$('cal-until').value='2027-01-01';$('cal-reviewed').value='2026-11-25';
 weekdays.forEach((_,i)=>$('cal-day-'+i).value=i<4?'00:00-24:00':i===4?'00:00-17:00':i===6?'17:00-24:00':'');
 $('cal-exceptions').value='';$('cal-market-assumption').checked=true;$('cal-documented').checked=false;
 $('cal-references').value='https://www.oanda.com/us-en/trading/forex/';
 $('cal-notes').value='Algemene forexweek van zondag 17:00 tot vrijdag 17:00 America/New_York. Vastgelegde onderzoeksaanname, niet geverifieerd voor deze databron. Geen dagelijkse pauzes of feestdaguitzonderingen opgenomen; afwijkingen blijven zichtbaar. Geldigheidsbereik 2026 is een expliciete onderzoekskeuze, geen bewijs van historische brokeruren.';
 $('cal-status').textContent='Standaard ingevuld, nog niet opgeslagen. Controleer het geldigheidsbereik en de UTC-klokverklaring; bekijk daarna de beoordeling.';
};
$('cal-preview').onclick=()=>apply('preview');$('cal-save').onclick=()=>apply('save');
})();
