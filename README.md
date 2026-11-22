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
| Data model | sqx-core | Bars, ticks, timeframes, instruments, directions |
| Data Manager | sqx-data | Multi-symbol bar store, CSV import, tick CSV, synthetic demo bars |
| Indicators | sqx-indicators | 180 dependency-free indicators (540 building blocks incl. variants) |
| Strategy model | sqx-strategy | Blocks, conditions, signals, money management, pseudo-code, catalog |
| Backtester | sqx-backtest | Event-driven, no look-ahead, SL/PT/trailing/BE, costs, tick-precision |
| Statistics | sqx-metrics | ~30 metrics mirroring the databank column catalog |
| Discovery | sqx-optimizer | Random generation + genetic evolution + walk-forward + ranking |
| Robustness | sqx-robustness | What-if, Monte Carlo trade manipulation, MC retest |
| Portfolio | sqx-portfolio | Combine strategies, weights, correlation, best subset |
| Custom Projects | sqx-pipeline | Build -> Retest -> Optimize pipeline |
| Persistence | sqx-project | Project -> databanks -> ranked strategies (JSON) |
| Export | sqx-export | MQL4 / MQL5 / PineScript / NinjaScript / EasyLanguage / PseudoCode |
| Analysis | sqx-analyzer | QuantAnalyzer-style report: monthly returns, drawdowns, trade stats |
| Cloud signals | sqx-cloud | AlgoCloud-style daily LONG/SHORT/FLAT signals |
| Assistant | sqx-assistant | LLM client (OpenAI-compatible) that drives the engine via tools |
| MCP server | sqx-mcp | stdio JSON-RPC server exposing the engine as MCP tools |
| Web | sqx-web | Embedded HTTP server + 9-tab dashboard (no build step) |
| CLI | sqx-cli | backtest / generate / evolve / optimize / export / robustness / portfolio / montecarlo / crosscheck / pipeline / analyze / cloud / assistant / serve / doctor / indicators |

## Quick start

```bash
cargo build --release
cargo test

# environment check (toolchain, data files, port, MT5 runtime)
cargo run -q -p sqx-cli -- doctor

# list the indicator catalog
cargo run -q -p sqx-cli -- indicators

# start the dashboard and open it in the browser
cargo run -q -p sqx-cli -- serve --open
```

`serve` with no `--data`/`--data-dir` starts on synthetic demo data (EURUSD and
GBPUSD, 2000 M1 bars), so the app can be opened without preparing a data source.

### Full tour (one script)

`examples/run_everything.sh` runs the whole flow end to end — indicator count,
backtest, generate + rank, robustness (what-if + Monte Carlo), the build →
retest → optimize pipeline, the analysis report, a portfolio and an MQL5 export:

```bash
./examples/run_everything.sh data/mt5/EURUSD_M5.csv EURUSD
```

### AI assistant

Any OpenAI-compatible endpoint works (for example a free provider from
https://github.com/open-free-llm-api/awesome-freellm-apis). If Ollama runs on
127.0.0.1:11434 it is used automatically (model llama3.1, no key needed).

```bash
export SQX_LLM_API_KEY=...                       # e.g. from console.groq.com/keys
export SQX_LLM_BASE_URL=https://api.groq.com/openai/v1
export SQX_LLM_MODEL=llama-3.3-70b-versatile
cargo run -q -p sqx-cli -- assistant --data data/mt5/EURUSD_M5.csv --ask "generate 50 strategies and analyse the best"
```

### CLI examples

```bash
# generate 300 random strategies, backtest, rank, print top 10
cargo run -q -p sqx-cli -- generate --data data/eurusd_sample.csv --count 300 --top 10

# genetic evolution
cargo run -q -p sqx-cli -- evolve --data data/eurusd_sample.csv --generations 20 --population 50

# backtest a strategy defined as JSON
cargo run -q -p sqx-cli -- backtest --strategy examples/rsi-mean-reversion.json --data data/eurusd_sample.csv

# export a strategy to platform source code (mql4 | mql5 | pine | ninjascript | easylanguage | pseudocode)
cargo run -q -p sqx-cli -- export --strategy examples/rsi-mean-reversion.json --engine mql5
cargo run -q -p sqx-cli -- export --strategy examples/rsi-mean-reversion.json --engine ninjascript
cargo run -q -p sqx-cli -- export --strategy examples/rsi-mean-reversion.json --engine easylanguage

# full robustness cross-check set (what-if, MC trade, MC retest, walk-forward)
cargo run -q -p sqx-cli -- robustness --strategy examples/rsi-mean-reversion.json --data data/eurusd_sample.csv

# walk-forward optimization
cargo run -q -p sqx-cli -- optimize --data data/eurusd_sample.csv --windows 6 --oos 0.25

# portfolio: best subset of generated strategies (equal weights + correlation)
cargo run -q -p sqx-cli -- portfolio --data data/eurusd_sample.csv --count 300 --top 12

# multi-market cross-check (one strategy on every symbol in a folder)
cargo run -q -p sqx-cli -- crosscheck --strategy examples/rsi-mean-reversion.json --data-dir data

# analysis report (monthly returns, drawdown periods, trade stats, rolling Sharpe)
cargo run -q -p sqx-cli -- analyze --strategy examples/rsi-mean-reversion.json --data data/eurusd_sample.csv

# AlgoCloud-style daily signals for a folder of strategies
cargo run -q -p sqx-cli -- cloud --strategies /tmp/sqx_strats --data data/eurusd_sample.csv

# build -> retest -> optimize pipeline
cargo run -q -p sqx-cli -- pipeline --data data/eurusd_sample.csv --count 200 --keep 20

# MCP server over stdio (JSON-RPC 2.0) for LLM clients
cargo run -q -p sqx-mcp

# dashboard on http://127.0.0.1:8080
cargo run -q -p sqx-cli -- serve --data data/eurusd_sample.csv
```

### Strategy JSON format

A strategy is a transparent rule tree (serde-serialized). See
`examples/rsi-mean-reversion.json`. Pseudo-code is rendered by
`sqx-cli pseudocode` and by the backtest output.

### Data format

CSV with header `timestamp,open,high,low,close,volume`, timestamp in unix
seconds. `data/eurusd_sample.csv` is a 2000-bar synthetic EURUSD sample.

### Real MT5 data (SiliconMetaTrader5)

If [SiliconMetaTrader5](https://github.com/bahadirumutiscimen/silicon-metatrader5)
is running on this Mac (native mode, Wine 11 + MT5), you can pull real history:

```bash
bash "$HOME/Library/Application Support/SiliconMT5/runtime/macos/native.sh" start

# export bars (client venv has siliconmetatrader5 + numpy)
"$HOME/Library/Application Support/SiliconMT5/venv/bin/python" scripts/mt5_export.py --symbols EURUSD,GBPUSD,USDJPY --timeframe M5 --count 5000 --out data/mt5

# use the real data
cargo run -q -p sqx-cli -- generate --data data/mt5/EURUSD_M5.csv --count 300
cargo run -q -p sqx-cli -- crosscheck --strategy examples/rsi-mean-reversion.json --data-dir data/mt5
cargo run -q -p sqx-cli -- serve --data-dir data/mt5
```

Export ticks with `--ticks` (writes `<symbol>_ticks.csv`). MT5 bar times are
broker server time; the engine uses relative bar intervals, so backtests are
unaffected. `data/mt5/` is gitignored (your broker data stays local).

## Web dashboard

A fully self-contained web app (no frontend build step, no external runtime):

```bash
cargo run -q -p sqx-cli -- serve --open
# open http://127.0.0.1:8080
```

Nine tabs:

| Tab | What it does |
|---|---|
| Home | Live overview (symbols, databank, profiles, AI model) + one-click pipeline run |
| Builder | AlgoWizard-style strategy editor, backtest, export (MQL5 / NinjaScript / EasyLanguage / Pine) |
| Optimizer | Generator settings (conditions, SL/PT ranges, per-indicator parameter ranges), simple / walk-forward / walk-forward matrix, portfolio composer |
| Retester | What-if cost scenarios and Monte Carlo trade manipulation |
| Results | Persistent databank: filter, sort, portfolio, equity chart, CSV download, per-row Use / MQL5 / Robust / Analyze |
| Data Manager | CSV import, broker profiles, sessions |
| Projects | Build -> retest -> optimize pipeline with a log |
| Settings | Deposit, spread, slippage, commission; active AI model |
| AI assistant | Chat that drives the engine through tools |

Dark and light themes, responsive layout, and a busy indicator for long
operations. All endpoints are plain JSON:

`/api/data`, `/api/datasources`, `/api/indicators`, `/api/catalog`,
`/api/backtest`, `/api/generate`, `/api/optimize`, `/api/portfolio`,
`/api/portfolio/databank`, `/api/robustness`, `/api/analyze`, `/api/export`,
`/api/pipeline`, `/api/databank`, `/api/databank.csv`, `/api/databank/clear`,
`/api/profiles`, `/api/profiles/save`, `/api/profiles/delete`,
`/api/project/list`, `/api/project/load`, `/api/project/save`,
`/api/data/import`, `/api/data/delete`, `/api/assistant`, `/api/assistant/config`.

## Design notes

- **No look-ahead**: signals are evaluated at bar close and filled at the next
  bar open; stop/profit/trailing levels are checked intra-bar against high/low.
- **Determinism**: the backtester and generators are seedable and pure.
- **Money management**: FixedSize, FixedAmount, RiskFixedBalancePct,
  RiskFixedPctOfAccount, ATRRiskBased, Crypto/Stocks variants, SimpleMartingale.
- **Exit blocks**: StopLoss (points/ATR), ProfitTarget (points/ATR),
  TrailingStop, MoveToBreakEven, ExitAfterBars, opposite-signal exit.
- **Multi-timeframe**: a strategy can carry a `confirmation` signal evaluated on a
  higher timeframe; entry requires both the base-TF signal and the confirmation
  (the last completed higher-TF bar is used, so there is no look-ahead).
- **Tick precision**: `backtest_ticks` resolves SL/PT against the ticks inside each
  bar (long exits on bid, short exits on ask); `--ticks` loads a tick CSV.
- **Parallel backtests**: `backtest_batch` spreads strategies across cores
  (deterministic, order-preserving).

## Tests

32 tests across the workspace: indicator references, engine behaviour,
exporters, metrics edge cases, strategy JSON round-trips, walk-forward
determinism, generator parameter ranges, and an in-process HTTP test of the
web endpoints. Run with `cargo test`.

## License

MIT. See Cargo.toml.
