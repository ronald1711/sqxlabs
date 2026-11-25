const fs=require('fs'),vm=require('vm'),assert=require('assert');
const html=fs.readFileSync(require('path').join(__dirname,'../assets/index.html'),'utf8');
const code=html.slice(html.indexOf('let eurProfileApplied='),html.indexOf('function robustnessArchiveNotice'));
const nodes={};for(const id of [...html.matchAll(/id="(set-[^"]+|eur-profile-[^"]+)"/g)].map(m=>m[1]))nodes[id]={value:'1',checked:false,textContent:'',dispatchEvent(){}};
nodes['set-rates'].value='[]';nodes['set-intrabar'].value='adverse_first';nodes['set-currency'].value='USD';nodes['set-stopout'].value='0.4';nodes['set-max-lots'].value='7';
const context={document:{getElementById:id=>nodes[id]},Event:class{}};vm.createContext(context);vm.runInContext(code,context);
for(const [scenario,spread,slippage] of [['baseline',2,0],['spread',3,0],['stress',4,1]]){
 nodes['eur-profile-scenario'].value=scenario;context.applyEurResearchProfile();
 const settings=context.settings();assert.equal(settings.account.currency,'EUR');assert.equal(settings.account.max_leverage,30);assert.equal(settings.deposit,10000);assert.equal(settings.spread,spread);assert.equal(settings.slippage,slippage);assert.equal(settings.commission,0);assert.equal(settings.account.long_financing_per_lot,0);assert.equal(settings.account.stop_out_ratio,0.4);assert.equal(settings.account.max_lots,7);assert.equal(settings.research_profile.modified,false);
 nodes['set-commission'].value='6';assert.equal(context.settings().commission,6);assert.equal(context.settings().research_profile.modified,true);assert.equal(settings.commission,0);assert.equal(settings.research_profile.modified,false);
 assert.match(nodes['eur-profile-status'].textContent,/currently excluded/);
}
console.log('EUR research profile assumptions, editable costs and frozen settings passed');
