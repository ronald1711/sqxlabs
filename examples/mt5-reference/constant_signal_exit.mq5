// Independent reference harness. Compiling is not execution/parity evidence.
// Refuses every environment except the Strategy Tester on SQX custom symbols.
#property strict
input bool ShortSide=false;
input datetime SignalBar=D'2026.01.05 00:00';
input double Lots=0.1;
bool entered=false, closed=false, failed=false;

bool Allowed() {
   return MQLInfoInteger(MQL_TESTER) && SymbolInfoInteger(_Symbol,SYMBOL_CUSTOM)
      && StringFind(_Symbol,"SQX_")==0 && _Period==PERIOD_M1;
}
int OnInit() {
   if(!Allowed() || Lots!=0.1) return INIT_PARAMETERS_INCORRECT;
   PrintFormat("SQX_REFERENCE_INIT symbol=%s short=%d signal_bar=%I64d lots=%.2f",
      _Symbol,ShortSide,(long)SignalBar,Lots);
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
   PrintFormat("SQX_REFERENCE_COMPLETE ok=%d balance=%.8f equity=%.8f",
      complete,AccountInfoDouble(ACCOUNT_BALANCE),AccountInfoDouble(ACCOUNT_EQUITY));
   return complete?1.0:-1.0;
}
