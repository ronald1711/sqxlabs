#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."
DATA="${1:-data/eurusd_sample.csv}"
SYM="${2:-EURUSD}"
STRAT="examples/rsi-mean-reversion.json"
SQX="cargo run -q -p sqx-cli --"

echo "==============================================="
echo " SqxLabs — full tour ($DATA, $SYM)"
echo "==============================================="

echo; echo "== 1/8  indicator library =="
$SQX indicators | wc -l

echo; echo "== 2/8  backtest the example strategy =="
$SQX backtest --strategy "$STRAT" --data "$DATA"

echo; echo "== 3/8  generate + rank =="
$SQX generate --data "$DATA" --count 200 --top 5

echo; echo "== 4/8  robustness (what-if + Monte Carlo) =="
$SQX robustness --strategy "$STRAT" --data "$DATA" --mc-runs 500 --retest-runs 100

echo; echo "== 5/8  build -> retest -> optimize pipeline =="
$SQX pipeline --data "$DATA" --count 200 --keep 10 | tail -8

echo; echo "== 6/8  analysis report =="
$SQX analyze --strategy "$STRAT" --data "$DATA" | head -30

echo; echo "== 7/8  portfolio from generated strategies =="
$SQX portfolio --data "$DATA" --count 200 --top 10 | head -20

echo; echo "== 8/8  export to MQL5 =="
$SQX export --strategy "$STRAT" --engine mql5 | head -20

echo; echo "Done. Start the web app with:"
echo "  $SQX serve --data $DATA --symbol $SYM"
