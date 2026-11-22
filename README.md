# SqxLabs — StrategyQuant open-source port (Rust)

A clean-room, open-source re-implementation of the core of
**StrategyQuant X** (a desktop platform for building, backtesting, optimizing and
stress-testing algorithmic trading strategies) in **Rust**.

The original application was analyzed at the shipped-artifact level only: its
bundled documentation, its unencrypted building-block catalog
(Snippets.jar class inventory), and its packaging (native launchers, JVM, jars).
No proprietary bytecode was decompiled; the Rust code here is written
independently from the documented behaviour and public domain knowledge.

## What is implemented

| Area | Crate | Notes |
|---|---|---|
| Data model | sqx-core | Bars, timeframes, instruments, directions |
| Data Manager | sqx-data | Multi-symbol bar store, CSV import, listing |
| Indicators | sqx-indicators | 76 dependency-free indicators (228 building blocks incl. variants) |
| Strategy model | sqx-strategy | Building blocks, conditions, signals, rules, money management, pseudo-code, block catalog |
| Backtester | sqx-backtest | Event-driven, no look-ahead, SL/PT/trailing/BE, costs, equity |
| Statistics | sqx-metrics | ~30 metrics mirroring the databank column catalog |
| Discovery | sqx-optimizer | Random generation + genetic evolution + ranking |
| Persistence | sqx-project | Project -> databanks -> ranked strategies (JSON) |
| Export | sqx-export | MQL4 / MQL5 / PineScript / NinjaScript / EasyLanguage / PseudoCode |
| Assistant | sqx-assistant | LLM client (OpenAI-compatible) that drives the engine via tools |
| Robustness | sqx-robustness | Monte Carlo trade-manipulation |
| Portfolio | sqx-portfolio | Combine strategies, weights, correlation, best subset |
| Custom Projects | sqx-pipeline | Build -> Retest -> Optimize pipeline with GoToTask |
| MCP server | sqx-mcp | stdio JSON-RPC server exposing the engine as MCP tools |
| Web | sqx-web | Embedded HTTP server + single-page dashboard |
| CLI | sqx-cli | backtest / generate / evolve / optimize / export / montecarlo / portfolio / serve / indicators / pseudocode |

## Quick start

```bash
cargo build --release
cargo test

# List the indicator catalog
cargo run -q -p sqx-cli -- indicators

# QuantAnalyzer-style analysis report (monthly returns, drawdown periods,
# trade statistics, rolling Sharpe)
cargo run -q -p sqx-cli -- analyze --strategy examples/rsi-mean-reversion.json --data data/mt5/EURUSD_M5.csv

# AlgoCloud-style daily signals for a folder of strategies
cargo run -q -p sqx-cli -- cloud --strategies /tmp/sqx_strats --data data/mt5/EURUSD_M5.csv

# LLM assistant: any OpenAI-compatible endpoint (e.g. a free provider from
# https://github.com/open-free-llm-api/awesome-freellm-apis)
export SQX_LLM_API_KEY=...                       # e.g. from console.groq.com/keys
export SQX_LLM_BASE_URL=https://api.groq.com/openai/v1
export SQX_LLM_MODEL=llama-3.3-70b-versatile
cargo run -q -p sqx-cli -- assistant --data data/mt5/EURUSD_M5.csv \
  --ask "genereer 50 strategieen en analyseer de beste"

# zero-key local option: if Ollama runs on 127.0.0.1:11434, the assistant uses it
# automatically (model llama3.1); otherwise it falls back to Groq. The same
# applies to the web dashboard's AI assistant panel.

# Generate 300 random strategies, backtest, rank, print top 10
cargo run -q -p sqx-cli -- generate --data data/eurusd_sample.csv --count 300 --top 10

# Genetic evolution
cargo run -q -p sqx-cli -- evolve --data data/eurusd_sample.csv --generations 20 --population 50

# Backtest a strategy defined as JSON
cargo run -q -p sqx-cli -- backtest --strategy examples/rsi-mean-reversion.json --data data/eurusd_sample.csv

# Export a strategy to platform source code
# (mql4 | mql5 | pine | ninjascript | easylanguage | pseudocode)
cargo run -q -p sqx-cli -- export --strategy examples/rsi-mean-reversion.json --engine mql5
cargo run -q -p sqx-cli -- export --strategy examples/rsi-mean-reversion.json --engine ninjascript
cargo run -q -p sqx-cli -- export --strategy examples/rsi-mean-reversion.json --engine easylanguage

# Tick-precision backtest: SL/PT are resolved against the ticks inside each bar
cargo run -q -p sqx-cli -- backtest --strategy examples/rsi-mean-reversion.json \
  --data data/mt5/EURUSD_M5.csv --ticks data/mt5/EURUSD_ticks.csv

# Monte Carlo robustness test
cargo run -q -p sqx-cli -- montecarlo --strategy examples/rsi-mean-reversion.json --data data/eurusd_sample.csv --runs 2000

# Walk-forward optimization
cargo run -q -p sqx-cli -- optimize --data data/eurusd_sample.csv --windows 6 --oos 0.25

# Build a portfolio: best subset of generated strategies (equal weights + correlation)
cargo run -q -p sqx-cli -- portfolio --data data/eurusd_sample.csv --count 300 --top 12

# Multi-market cross-check (one strategy on every symbol in a folder)
cargo run -q -p sqx-cli -- crosscheck --strategy examples/rsi-mean-reversion.json --data-dir data

# Serve the dashboard with multiple symbols (symbol = file stem)
cargo run -q -p sqx-cli -- serve --data-dir data

# Run a custom-project pipeline (Build -> Retest -> Optimize)
cargo run -q -p sqx-cli -- pipeline --data data/eurusd_sample.csv --count 200 --keep 20

# Full robustness cross-check set (what-if, MC trade, MC retest, walk-forward)
cargo run -q -p sqx-cli -- robustness --strategy examples/rsi-mean-reversion.json --data data/eurusd_sample.csv

# MCP server over stdio (JSON-RPC 2.0) for LLM clients
cargo run -q -p sqx-mcp

# Start the web dashboard, then open http://127.0.0.1:8080
cargo run -q -p sqx-cli -- serve --data data/eurusd_sample.csv
```

### Strategy JSON format

A strategy is a transparent rule tree (serde-serialized). See
examples/rsi-mean-reversion.json. Pseudo-code is rendered by
\`sqx-cli pseudocode\` and by the backtest output.

### Data format

CSV with header `timestamp,open,high,low,close,volume`, timestamp in unix
seconds. data/eurusd_sample.csv is a 2000-bar synthetic EURUSD sample.

### Real MT5 data (SiliconMetaTrader5)

If [SiliconMetaTrader5](https://github.com/bahadirumutiscimen/silicon-metatrader5)
is running on this Mac (native mode, Wine 11 + MT5), you can pull real history:

```bash
# start the runtime (native mode) if it is stopped
bash "$HOME/Library/Application Support/SiliconMT5/runtime/macos/native.sh" start

# export bars (client venv has siliconmetatrader5 + numpy)
"$HOME/Library/Application Support/SiliconMT5/venv/bin/python" scripts/mt5_export.py \
  --symbols EURUSD,GBPUSD,USDJPY --timeframe M5 --count 5000 --out data/mt5

# use the real data
cargo run -q -p sqx-cli -- generate --data data/mt5/EURUSD_M5.csv --count 300
cargo run -q -p sqx-cli -- crosscheck --strategy examples/rsi-mean-reversion.json --data-dir data/mt5
cargo run -q -p sqx-cli -- serve --data-dir data/mt5
```

Export ticks with `--ticks` (writes `<symbol>_ticks.csv`). MT5 bar times are broker server time; the engine uses relative bar intervals,
so backtests are unaffected. `data/mt5/` is gitignored (your broker data stays
local).

## Web dashboard

A fully self-contained web app (no frontend build step, no external runtime):

```bash
cargo run -q -p sqx-cli -- serve --data data/eurusd_sample.csv
# open http://127.0.0.1:8080
```

The dashboard shows the data summary, the indicator catalog, a generate-and-rank
panel, and a backtest panel with a metrics table, an SVG equity curve and the
pseudo-code of the tested strategy. All endpoints are plain JSON
(/api/data, /api/indicators, /api/backtest, /api/generate, /api/assistant, /api/assistant/config).
The dashboard also has an **AI assistant** panel that posts to `/api/assistant`.
`serve` with no `--data`/`--data-dir` starts on synthetic demo data (EURUSD and
GBPUSD, 2000 M1 bars) so the app can be opened without preparing a data source.

## Design notes

- **No look-ahead**: signals are evaluated at bar close and filled at the next
  bar's open; stop/profit/trailing levels are checked intra-bar against high/low.
- **Determinism**: the backtester and generators are seedable and pure.
- **Money management**: FixedSize, FixedAmount, RiskFixedBalancePct,
  RiskFixedPctOfAccount, ATRRiskBased.
- **Exit blocks**: StopLoss (points/ATR), ProfitTarget (points/ATR),
  TrailingStop, MoveToBreakEven, ExitAfterBars, opposite-signal exit.
- **Multi-timeframe**: a strategy can carry a `confirmation` signal evaluated on a
  higher timeframe; entry requires both the base-TF signal and the confirmation
  (the last completed higher-TF bar is used, so there is no look-ahead).
- **Intrabar path**: `BacktestConfig.intrabar_up_first` selects which extreme is
  assumed hit first when both SL and TP fall inside one bar (higher-precision
  modelling; true tick replay needs tick data).

## License

MIT OR Apache-2.0 (dual). See Cargo.toml.
