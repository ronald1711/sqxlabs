import sys
import unittest
from decimal import Decimal as D
from pathlib import Path
sys.path.insert(0,str(Path(__file__).resolve().parents[1]))
from mt5_reference_check import sma2_signal_index,coverage_status

class ReferenceCoverage(unittest.TestCase):
    def test_signal_requires_warmup_and_a_following_fill_bar(self):
        self.assertEqual(sma2_signal_index(list(map(D,['1','2','3'])),True),1)
        self.assertIsNone(sma2_signal_index(list(map(D,['1','2','3'])),False))
        self.assertEqual(sma2_signal_index(list(map(D,['3','1','2','4'])),False),1)
        self.assertEqual(sma2_signal_index(list(map(D,['3','1','2','4'])),True),2)
        self.assertIsNone(sma2_signal_index(list(map(D,['2','2','1'])),False), 'last-bar signal has no next-open fill in this capture')
        self.assertIsNone(sma2_signal_index(list(map(D,['2','1'])),False))
        self.assertIsNone(sma2_signal_index(list(map(D,['2','2','2'])),True))

    def test_no_trade_verification_cannot_become_full_execution_coverage(self):
        reports=[{'rule':mode,'side':side,'status':'passed','matches':True} for mode in ('constant','sma2','stop','signal_exit') for side in ('long','short')]
        self.assertEqual(coverage_status(reports),'passed')
        self.assertEqual(coverage_status(reports[:-1]),'coverage_incomplete')
        self.assertEqual(coverage_status(reports[:-1]+[reports[0]]),'coverage_incomplete')
        reports[3]={'rule':'sma2','side':'short','status':'not_exercised','no_trade_matches':True,'matches':None}
        self.assertEqual(coverage_status(reports),'coverage_incomplete')

if __name__=='__main__':unittest.main()
