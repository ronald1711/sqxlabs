const fs=require('node:fs'),vm=require('node:vm'),assert=require('node:assert/strict');
const source=fs.readFileSync('crates/sqx-web/assets/workspace.js','utf8'),start=source.indexOf('function signalEvidenceHtml('),end=source.indexOf('function renderTrades(',start);assert(start>=0&&end>start);
const ctx=vm.createContext({escapeHtml:s=>String(s??'').replaceAll('&','&amp;').replaceAll('<','&lt;').replaceAll('>','&gt;')});vm.runInContext(source.slice(start,end),ctx);
const trade={id:0,direction:'Long',open_time:0,close_time:120009,size:0.1,open_price:1,close_price:1.1,pnl:10,commission:0,financing:0,exit_reason:'ExitSignal'};
const execution={account_currency:'USD',scheduled_exit_decisions:[{trade_id:0,decision_bar_index:1,decision_bar_open_time:60000,reasons:['ExitAfterBars','ExitSignal','<unsafe>'],chosen_reason:'ExitSignal',executed_time:120009},{trade_id:9,reasons:['NOT THIS TRADE']}]};
const html=ctx.tradeLedgerHtml([trade],execution);assert.match(html,/1970-01-01T00:00:00.000Z/);assert.match(html,/Executed at 1970-01-01T00:02:00.009Z/);assert.match(html,/ExitAfterBars, ExitSignal, &lt;unsafe&gt;/);assert.doesNotMatch(html,/NOT THIS TRADE|<unsafe>/);assert.match(html,/timestamp is not the decision instant/);assert.match(html,/includes commission and financing/);
execution.scheduled_exit_decisions[0].executed_time=null;trade.exit_reason='EndOfData';const terminal=ctx.tradeLedgerHtml([trade],execution);assert.match(terminal,/Not executed/);assert.match(terminal,/reporting convention/);assert.doesNotMatch(terminal,/Executed at/);
assert.match(ctx.tradeLedgerHtml([trade],{}),/was not recorded/);assert.match(ctx.tradeLedgerHtml([trade],{scheduled_exit_decisions:[]}),/No close-derived/);
console.log('Trade explanations passed: matching IDs, competing triggers, exact ticks, terminal unfilled requests, legacy absence and escaping.');

const settled={cash_settlements:[{trade_id:0,account_currency:'USD',quote_currency:'JPY',balance_before:10000,gross_profit_quote:100000,quote_to_account_rate:1/151,gross_profit_account:100000/151,commission:0,financing:-7,net_change:100000/151+7,balance_after:10000+100000/151+7},{trade_id:9,account_currency:'NOT THIS TRADE'}]};
const cash=ctx.cashSettlementHtml(0,settled);assert.match(cash,/How did this trade change the balance/);assert.match(cash,/Gross profit \(JPY\)/);assert(cash.includes(String(1/151)));assert.match(cash,/<dd>0<\/dd>/);assert.match(cash,/<dd>-7<\/dd>/);assert.match(cash,/Negative financing is a credit/);assert.match(cash,/do not subtract them again/);assert.doesNotMatch(cash,/NOT THIS TRADE/);
assert.match(ctx.cashSettlementHtml(0,{}),/was not recorded/);assert.match(ctx.cashSettlementHtml(0,{cash_settlements:[]}),/found 0/);
assert.match(ctx.cashSettlementHtml(0,{cash_settlements:[settled.cash_settlements[0],settled.cash_settlements[0]]}),/found 2/);
settled.cash_settlements[0].account_currency='<unsafe>';settled.cash_settlements[0].commission=NaN;
const escaped=ctx.cashSettlementHtml(0,settled);assert.match(escaped,/&lt;unsafe&gt;/);assert.doesNotMatch(escaped,/<unsafe>|NaN/);assert.match(escaped,/Not recorded/);
assert.match(ctx.tradeLedgerHtml([trade],settled),/How did this trade change the balance/);
console.log('Cash explanations passed: trade identity, conversion precision, zero costs, financing credits, legacy/missing/duplicate evidence and escaping.');

const accrued={account_currency:'<USD>',financing_accruals:[{trade_id:0,rollover_utc_day:0,applied_at:86400123,lots:0.1,daily_charge_per_lot:-7,multiplier:3,charge:-2.1,accrued_before:0,accrued_after:-2.1},{trade_id:8,rollover_utc_day:900}]};
const finance=ctx.financingEvidenceHtml(0,accrued);assert.match(finance,/1970-01-01T00:00:00.000Z/);assert.match(finance,/1970-01-02T00:00:00.123Z/);assert.match(finance,/&lt;USD&gt;/);assert.match(finance,/<td>3<\/td>/);assert.match(finance,/<td>-2.1<\/td>/);assert.match(finance,/<td>0<\/td>/);assert.match(finance,/1 accruals/);assert.match(finance,/may differ after a data gap/);assert.match(finance,/settled at trade close/);
assert.match(ctx.financingEvidenceHtml(0,{}),/was not recorded/);assert.match(ctx.financingEvidenceHtml(0,{financing_accruals:[]}),/No nonzero financing/);
assert.match(ctx.tradeLedgerHtml([trade],accrued),/Financing history/);
console.log('Financing explanations passed: trade filtering, epoch day zero, delayed recognition, triple multiplier, credits and missing evidence.');

const stopped={account_currency:'USD',stop_out_decisions:[{trade_id:0,time:0,balance:10000,unrealized_profit_account:-9000,commission_reserve:5,financing_accrued:0,equity_before_slippage:995,required_margin:2000,stop_out_ratio:0.5,minimum_equity:1000,threshold_price:0.91,liquidation_price_before_slippage:0.90,fill_price:0.8999,quote_to_account_rate:1},{trade_id:8,balance:123456789}]};
const stopTrade={...trade,exit_reason:'StopOut'};
const stopHtml=ctx.stopOutEvidenceHtml(stopTrade,stopped);assert.match(stopHtml,/Why was margin insufficient/);assert.match(stopHtml,/1970-01-01T00:00:00.000Z/);assert.match(stopHtml,/<dd>0.8999<\/dd>/);assert.match(stopHtml,/<dd>0.9<\/dd>/);assert.match(stopHtml,/<dd>0.91<\/dd>/);assert.match(stopHtml,/gap or observed tick/);assert.doesNotMatch(stopHtml,/123456789/);
assert.match(ctx.stopOutEvidenceHtml(stopTrade,{}),/was not recorded/);assert.match(ctx.stopOutEvidenceHtml(stopTrade,{stop_out_decisions:[]}),/does not match/);assert.equal(ctx.stopOutEvidenceHtml(trade,{stop_out_decisions:[]}), '');assert.match(ctx.stopOutEvidenceHtml(trade,stopped),/does not match/);
assert.match(ctx.tradeLedgerHtml([stopTrade],stopped),/Why was margin insufficient/);
console.log('Stop-out explanations passed: matching exit/trade, distinct threshold/quote/fill, epoch zero and missing or contradictory evidence.');

const stopChanges={stop_changes:[{trade_id:0,time:0,observed_price:1.02,source:'ohlc_high_first',path_step:1,previous_stop:0.99,new_stop:1.01,previous_reason:'StopLoss',new_reason:'TrailingStop'},{trade_id:0,time:123,observed_price:1.03,source:'observed_tick',path_step:null,previous_stop:1.01,new_stop:1.02,previous_reason:'TrailingStop',new_reason:'TrailingStop'},{trade_id:9,new_reason:'UNRELATED'}]};
const history=ctx.stopHistoryHtml(0,stopChanges);assert.match(history,/2 changes/);assert.match(history,/Modelled OHLC · high first · step 1/);assert.match(history,/Observed tick/);assert.match(history,/1970-01-01T00:00:00.123Z/);assert.match(history,/<td>0.99<\/td>/);assert.match(history,/<td>1.02<\/td>/);assert.doesNotMatch(history,/UNRELATED/);assert.match(history,/not exact intrabar times/);
assert.match(ctx.stopHistoryHtml(0,{}),/was not recorded/);assert.match(ctx.stopHistoryHtml(0,{stop_changes:[]}),/No effective stop changes/);
stopChanges.stop_changes[0].previous_stop=null;stopChanges.stop_changes[0].new_reason='<unsafe>';assert.match(ctx.stopHistoryHtml(0,stopChanges),/No finite stop/);assert.match(ctx.stopHistoryHtml(0,stopChanges),/&lt;unsafe&gt;/);
assert.match(ctx.tradeLedgerHtml([trade],stopChanges),/Stop history/);
console.log('Stop history passed: selected trade, model/tick distinction, exact milliseconds, missing/empty evidence, finite-stop absence and escaping.');

const snapshots={account_currency:'USD',account_snapshots:Array.from({length:26},(_,i)=>({phase:i===25?'terminal_liquidation':'bar_close',bar_open_time:i*60000,quote_time:i===0?123:null,trade_id:i===25?null:0,liquidation_price:i===25?null:1,quote_to_account_rate:i===25?null:1,balance:10000,gross_unrealized:0,commission_reserve:0,financing_accrued:0,equity:10000,required_margin:0,free_margin:10000}))};
const firstPage=ctx.accountSnapshotsHtml(snapshots);assert.match(firstPage,/Observations 1–25 of 26/);assert.match(firstPage,/Trade 0/);assert.match(firstPage,/1970-01-01T00:00:00.123Z/);assert.doesNotMatch(firstPage,/After final liquidation/);assert.equal((firstPage.match(/Account calculation · observation/g)||[]).length,25);
const lastPage=ctx.accountSnapshotsHtml(snapshots,1);assert.match(lastPage,/Observations 26–26 of 26/);assert.match(lastPage,/After final liquidation · Flat/);assert.match(lastPage,/Not applicable/);assert.match(lastPage,/not at every intrabar price/);assert.match(lastPage,/data-account-page="2" disabled/);
assert.equal(ctx.accountSnapshotsHtml(snapshots,999),lastPage);assert.equal(ctx.accountSnapshotsHtml(snapshots,-3),firstPage);assert.match(ctx.accountSnapshotsHtml({}),/were not recorded/);assert.match(ctx.accountSnapshotsHtml({account_snapshots:[]}),/No account observations/);
console.log('Account observations passed: bounded pages, clamped indices, trade zero, flat liquidation, exact quote time and legacy absence.');
