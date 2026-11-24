import importlib.util
from pathlib import Path
import sys
import tempfile
import types
import unittest
from unittest.mock import patch


class Column(list):
    def __lt__(self, other): return Column(x < other for x in self)
    def __ge__(self, other): return Column(x >= other for x in self)
    def __and__(self, other): return Column(a and b for a,b in zip(self,other))


class Scalar:
    def __init__(self, value): self.value=value
    def item(self): return self.value


class Row:
    def __init__(self,names,values): self.names=names;self.values=values
    def __getitem__(self,key):
        if isinstance(key,str): return Scalar(self.values[self.names.index(key)])
        if isinstance(key,slice): raise IndexError('structured row')
        return self.values[key]


class Rows(list):
    def __init__(self,names,values):
        self.dtype=types.SimpleNamespace(names=names)
        super().__init__(Row(names,v) for v in values)
    def __getitem__(self,key):
        if isinstance(key,str): return Column(row[key].item() for row in self)
        if isinstance(key,Column): return Rows(self.dtype.names,[row.values for row,keep in zip(self,key) if keep])
        return super().__getitem__(key)


class CaptureTests(unittest.TestCase):
    def module(self):
        root=Path(__file__).resolve().parents[1]
        stub=types.ModuleType('siliconmetatrader5');stub.MetaTrader5=object
        with patch.dict(sys.modules,{'siliconmetatrader5':stub}),patch.object(sys,'path',[str(root),*sys.path]):
            spec=importlib.util.spec_from_file_location('capture_under_test',root/'mt5_capture_window.py')
            module=importlib.util.module_from_spec(spec);spec.loader.exec_module(module)
        return module

    def client(self):
        class Client:
            TIMEFRAME_M1=1;COPY_TICKS_ALL=0
            def account_info(self): return types.SimpleNamespace(server='fixture',currency='EUR',leverage=30,trade_mode=0)
            def symbol_info(self,s):
                keys=('name','digits','point','trade_contract_size','trade_tick_size','volume_min','volume_max','volume_step','currency_base','currency_profit','currency_margin','trade_calc_mode')
                return types.SimpleNamespace(**dict.fromkeys(keys,1))
            def symbol_info_tick(self,s):return types.SimpleNamespace(time=240)
            def copy_rates_range(self,*args):return Rows(('time','open','high','low','close','tick_volume'),[(120,1,1,1,1,2)])
            def copy_ticks_range(self,s,start,end,flags):
                self.request=(start,end)
                return Rows(('time','bid','ask','last','volume','time_msc'),[(119,1,1,0,0,119999),(120,1,1,0,0,120000),(120,1,1.1,0,0,120000),(179,1,1,0,0,179999),(180,1,1,0,0,180000)])
        return Client()

    def test_half_open_window_retains_warmup_endpoint_and_equal_time_order(self):
        module=self.module();client=self.client();result=module.capture(client,['EURUSD'],120,180)
        self.assertEqual(client.request,(60,180))
        market=result['markets'][0]
        self.assertEqual(len(market['raw_tick_response']),5)
        self.assertEqual(len(market['raw_warmup_ticks']),1)
        self.assertEqual([r['time_msc'] for r in market['raw_ticks']],[120000,120000,179999])
        self.assertEqual([r['ask'] for r in market['raw_ticks']],[1,1.1,1])
        with tempfile.TemporaryDirectory() as root:
            path=module.publish(result,root)
            self.assertEqual(module.publish(result,root),path)
            (path/'capture.json').write_text('corrupted')
            with self.assertRaisesRegex(ValueError,'integrity'):module.publish(result,root)

    def test_missing_candle_and_changed_account_reject_whole_capture(self):
        module=self.module();client=self.client()
        with self.assertRaisesRegex(ValueError,'candles'):module.capture(client,['EURUSD'],60,180)
        original=client.account_info;calls=[]
        def account():
            value=original();calls.append(1)
            if len(calls)>1:value.currency='USD'
            return value
        client.account_info=account
        with self.assertRaisesRegex(ValueError,'account context changed'):module.capture(client,['EURUSD'],120,180)
