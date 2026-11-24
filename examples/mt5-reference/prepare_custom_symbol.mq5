// User-started synthetic-data setup. No order, account or broker-setting calls.
// Includes 120 warmup M1 bars before the 120-bar baseline on January 5.
// Existing symbols are never overwritten. A partial failed setup remains visible.
#property strict
#property script_show_inputs
input bool CreateSyntheticTestMarket=false;
void OnStart() {
 if(!CreateSyntheticTestMarket || MQLInfoInteger(MQL_TESTER)) {
  Print("SQX_FIXTURE_REFUSED: explicitly enable CreateSyntheticTestMarket in the terminal script inputs");return;
 }
 string s="SQX_EURUSD";
 if(!CustomSymbolCreate(s,"SqxReference","")) {Print("SQX_REF_CREATE_ERROR ",GetLastError());return;}
 bool ok=true;
 ok=CustomSymbolSetInteger(s,SYMBOL_DIGITS,5)&&ok;
 ok=CustomSymbolSetInteger(s,SYMBOL_SWAP_MODE,SYMBOL_SWAP_MODE_DISABLED)&&ok;
 ok=CustomSymbolSetInteger(s,SYMBOL_TRADE_STOPS_LEVEL,0)&&ok;
 ok=CustomSymbolSetInteger(s,SYMBOL_TRADE_FREEZE_LEVEL,0)&&ok;
 ok=CustomSymbolSetDouble(s,SYMBOL_POINT,0.00001)&&ok;
 ok=CustomSymbolSetDouble(s,SYMBOL_TRADE_TICK_SIZE,0.00001)&&ok;
 ok=CustomSymbolSetDouble(s,SYMBOL_TRADE_CONTRACT_SIZE,100000)&&ok;
 ok=CustomSymbolSetDouble(s,SYMBOL_VOLUME_MIN,0.01)&&ok;
 ok=CustomSymbolSetDouble(s,SYMBOL_VOLUME_MAX,100)&&ok;
 ok=CustomSymbolSetDouble(s,SYMBOL_VOLUME_STEP,0.01)&&ok;
 ok=CustomSymbolSetString(s,SYMBOL_CURRENCY_BASE,"EUR")&&ok;
 ok=CustomSymbolSetString(s,SYMBOL_CURRENCY_PROFIT,"USD")&&ok;
 ok=CustomSymbolSetString(s,SYMBOL_CURRENCY_MARGIN,"EUR")&&ok;
 ok=CustomSymbolSetInteger(s,SYMBOL_TRADE_CALC_MODE,SYMBOL_CALC_MODE_FOREX)&&ok;
 ok=CustomSymbolSetInteger(s,SYMBOL_TRADE_MODE,SYMBOL_TRADE_MODE_FULL)&&ok;
 ok=CustomSymbolSetInteger(s,SYMBOL_TRADE_EXEMODE,SYMBOL_TRADE_EXECUTION_MARKET)&&ok;
 ok=CustomSymbolSetInteger(s,SYMBOL_FILLING_MODE,SYMBOL_FILLING_FOK)&&ok;
 for(int day=0;day<7;day++) {
  ok=CustomSymbolSetSessionQuote(s,(ENUM_DAY_OF_WEEK)day,0,0,86400)&&ok;
  ok=CustomSymbolSetSessionTrade(s,(ENUM_DAY_OF_WEEK)day,0,0,86400)&&ok;
 }
 if(!ok){Print("SQX_REF_PROPERTIES_ERROR ",GetLastError());return;}
 if(!SymbolSelect(s,true)){Print("SQX_FIXTURE_SELECT_ERROR ",GetLastError());return;}
 MqlRates rates[];MqlTick ticks[];ArrayResize(rates,240);ArrayResize(ticks,960);
 datetime start=D'2026.01.04 22:00';
 for(int i=0;i<240;i++) {
  double p=(110000+(i%20)*10)/100000.0;
  rates[i].time=start+i*60;rates[i].open=p;rates[i].high=p+0.00004;rates[i].low=p-0.00003;rates[i].close=p+0.00001;rates[i].tick_volume=4;rates[i].spread=20;rates[i].real_volume=0;
  double prices[4];prices[0]=rates[i].open;prices[1]=rates[i].low;prices[2]=rates[i].high;prices[3]=rates[i].close;
  for(int j=0;j<4;j++) {
   int k=i*4+j;ZeroMemory(ticks[k]);ticks[k].time=start+i*60+j*15;ticks[k].time_msc=(long)ticks[k].time*1000;ticks[k].bid=prices[j];ticks[k].ask=prices[j]+0.00020;ticks[k].flags=TICK_FLAG_BID|TICK_FLAG_ASK;
  }
 }
 int bars=CustomRatesReplace(s,start,start+239*60,rates);
 int count=CustomTicksReplace(s,ticks[0].time_msc,ticks[959].time_msc,ticks);
 PrintFormat("SQX_REF_IMPORT bars=%d ticks=%d error=%d",bars,count,GetLastError());
 if(bars!=240||count!=960)return;
 MqlTick readback[];int n=CopyTicksRange(s,readback,COPY_TICKS_ALL,ticks[0].time_msc,ticks[959].time_msc);
 if(n!=960){Print("SQX_REF_READBACK_ERROR ",n);return;}
 for(int k=0;k<n;k++) if(readback[k].time_msc!=ticks[k].time_msc||MathAbs(readback[k].bid-ticks[k].bid)>1e-12||MathAbs(readback[k].ask-ticks[k].ask)>1e-12){Print("SQX_REF_MISMATCH ",k);return;}
 MqlRates check[];int nr=CopyRates(s,PERIOD_M1,start,start+239*60,check);
 if(nr!=240){Print("SQX_FIXTURE_BAR_READBACK_ERROR ",nr);return;}
 for(int k=0;k<nr;k++) {
  if(check[k].time!=rates[k].time || MathAbs(check[k].open-rates[k].open)>1e-12
    || MathAbs(check[k].high-rates[k].high)>1e-12 || MathAbs(check[k].low-rates[k].low)>1e-12
    || MathAbs(check[k].close-rates[k].close)>1e-12 || check[k].tick_volume!=4) {
   Print("SQX_FIXTURE_BAR_MISMATCH ",k);return;
  }
 }
 Print("SQX_FIXTURE_READY symbol=SQX_EURUSD bars=240 ticks=960 tick_and_bar_readback=matched");
}
