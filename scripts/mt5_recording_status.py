#!/usr/bin/env python3
"""Verify and report a committed recording snapshot without writes or MT5 access."""
import argparse
import json
from pathlib import Path
import sys
from mt5_capture_journal import Journal


def main():
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--journal',type=Path,required=True)
    args=parser.parse_args()
    config=json.loads((args.journal/'config.json').read_bytes())
    print(json.dumps(Journal(args.journal,config).status(),indent=2,allow_nan=False))


if __name__=='__main__':
    try:main()
    except (ValueError,OSError,KeyError,TypeError) as error:
        print(json.dumps({'integrity':'not_verified','error_type':type(error).__name__,'message':str(error)}),file=sys.stderr)
        raise SystemExit(1)
