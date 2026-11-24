// Independent reference harness. Compiling is not execution/parity evidence.
// Refuses every environment except the Strategy Tester on SQX custom symbols.
#property strict
input bool ShortSide=false;
input datetime SignalBar=D'2026.01.05 00:00';
input double Lots=0.1;
bool entered=false, closed=false, failed=false;
double initial_balance=0.0;

bool Allowed() {
   return MQLInfoInteger(MQL_TESTER) && SymbolInfoInteger(_Symbol,SYMBOL_CUSTOM)
      && StringFind(_Symbol,"SQX_")==0 && _Period==PERIOD_M1;
}
int OnInit() {
   if(!Allowed() || Lots!=0.1) return INIT_PARAMETERS_INCORRECT;
   initial_balance=AccountInfoDouble(ACCOUNT_BALANCE);
   PrintFormat("SQX_REFERENCE_INIT symbol=%s short=%d signal_bar=%I64d lots=%.2f",
      _Symbol,ShortSide,(long)SignalBar,Lots);
   PrintFormat("SQX_REFERENCE_ACCOUNT currency=%s balance=%.8f leverage=%I64d margin_mode=%I64d",
      AccountInfoString(ACCOUNT_CURRENCY),initial_balance,AccountInfoInteger(ACCOUNT_LEVERAGE),AccountInfoInteger(ACCOUNT_MARGIN_MODE));
   PrintFormat("SQX_REFERENCE_SYMBOL base=%s profit=%s margin=%s contract=%.8f point=%.8f tick_size=%.8f calculation=%I64d",
      SymbolInfoString(_Symbol,SYMBOL_CURRENCY_BASE),SymbolInfoString(_Symbol,SYMBOL_CURRENCY_PROFIT),
      SymbolInfoString(_Symbol,SYMBOL_CURRENCY_MARGIN),SymbolInfoDouble(_Symbol,SYMBOL_TRADE_CONTRACT_SIZE),
      SymbolInfoDouble(_Symbol,SYMBOL_POINT),SymbolInfoDouble(_Symbol,SYMBOL_TRADE_TICK_SIZE),SymbolInfoInteger(_Symbol,SYMBOL_TRADE_CALC_MODE));
   return INIT_SUCCEEDED;
}
bool Execute(bool closing) {
   if(!Allowed())return false;
   MqlTick tick; if(!SymbolInfoTick(_Symbol,tick))return false;
   MqlTradeRequest request={}; MqlTradeResult result={};
   request.action=TRADE_ACTION_DEAL;request.symbol=_Symbol;
   request.volume=Lots;request.magic=260511;
   bool sell=closing?!ShortSide:ShortSide;
   request.type=sell?ORDER_TYPE_SELL:ORDER_TYPE_BUY;
   request.price=sell?tick.bid:tick.ask;
   request.type_filling=ORDER_FILLING_FOK;
   request.comment="Sqx tester reference";
   if(closing) {
      if(!PositionSelect(_Symbol))return false;
      request.position=(ulong)PositionGetInteger(POSITION_TICKET);
   }
   bool sent=OrderSend(request,result);
   PrintFormat("SQX_REFERENCE_ORDER closing=%d time_msc=%I64d bid=%.5f ask=%.5f retcode=%u deal=%I64u price=%.5f volume=%.2f",
      closing,tick.time_msc,tick.bid,tick.ask,result.retcode,result.deal,result.price,result.volume);
   return sent && result.retcode==TRADE_RETCODE_DONE && result.volume==Lots;
}
void OnTick() {
   if(!Allowed() || failed || closed)return;
   MqlTick tick;if(!SymbolInfoTick(_Symbol,tick)){failed=true;return;}
   // Constant entry signal at SignalBar close, constant exit at the next close.
   // Fail on missing entry interval instead of silently inventing a later trade.
   if(!entered && tick.time>=SignalBar+120){failed=true;return;}
   if(!entered && tick.time>=SignalBar+60) {
      entered=Execute(false);failed=!entered;return;
   }
   if(entered && tick.time>=SignalBar+120) {
      closed=Execute(true);failed=!closed;
   }
}
double OnTester() {
   bool complete=Allowed() && entered && closed && !failed && PositionsTotal()==0;
   // Read the final history, independent of asynchronous notification ordering.
   int market_deals=0;double net=0.0;long position_id=0;
   if(!HistorySelect(0,TimeCurrent()))complete=false;
   else for(int i=0;i<HistoryDealsTotal();i++) {
      ulong ticket=HistoryDealGetTicket(i);
      if(ticket==0){complete=false;continue;}
      long type=HistoryDealGetInteger(ticket,DEAL_TYPE);
      double profit=HistoryDealGetDouble(ticket,DEAL_PROFIT);
      double commission=HistoryDealGetDouble(ticket,DEAL_COMMISSION);
      double swap=HistoryDealGetDouble(ticket,DEAL_SWAP);
      double fee=HistoryDealGetDouble(ticket,DEAL_FEE);
      long entry=HistoryDealGetInteger(ticket,DEAL_ENTRY);
      long position=HistoryDealGetInteger(ticket,DEAL_POSITION_ID);
      PrintFormat("SQX_REFERENCE_DEAL ticket=%I64u time_msc=%I64d type=%I64d entry=%I64d position=%I64d price=%.8f volume=%.8f profit=%.8f commission=%.8f swap=%.8f fee=%.8f",
         ticket,HistoryDealGetInteger(ticket,DEAL_TIME_MSC),type,entry,position,
         HistoryDealGetDouble(ticket,DEAL_PRICE),HistoryDealGetDouble(ticket,DEAL_VOLUME),profit,commission,swap,fee);
      if(type!=DEAL_TYPE_BUY && type!=DEAL_TYPE_SELL) {
         // Only the tester's initial deposit is permitted in this narrow case.
         if(i!=0 || type!=DEAL_TYPE_BALANCE || profit!=initial_balance
            || commission!=0.0 || swap!=0.0 || fee!=0.0)complete=false;
         continue;
      }
      if(HistoryDealGetString(ticket,DEAL_SYMBOL)!=_Symbol || HistoryDealGetInteger(ticket,DEAL_MAGIC)!=260511
         || HistoryDealGetDouble(ticket,DEAL_VOLUME)!=Lots)complete=false;
      if(market_deals==0) {
         position_id=position;
         if(entry!=DEAL_ENTRY_IN || type!=(ShortSide?DEAL_TYPE_SELL:DEAL_TYPE_BUY))complete=false;
      } else if(market_deals==1) {
         if(entry!=DEAL_ENTRY_OUT || position!=position_id || type!=(ShortSide?DEAL_TYPE_BUY:DEAL_TYPE_SELL))complete=false;
      }
      market_deals++;net+=profit+commission+swap+fee;
   }
   double residual=AccountInfoDouble(ACCOUNT_BALANCE)-initial_balance-net;
   if(market_deals!=2 || MathAbs(residual)>0.000001)complete=false;
   PrintFormat("SQX_REFERENCE_RECONCILIATION market_deals=%d net=%.8f residual=%.8f",market_deals,net,residual);
   PrintFormat("SQX_REFERENCE_COMPLETE ok=%d balance=%.8f equity=%.8f",
      complete,AccountInfoDouble(ACCOUNT_BALANCE),AccountInfoDouble(ACCOUNT_EQUITY));
   return complete?1.0:-1.0;
}
