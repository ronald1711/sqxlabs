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
cargo run -q -p sqx-cli -- assistant --symbol EURUSD --timeframe M5 --data data/mt5/EURUSD_M5.csv --ask "generate 50 strategies and analyse the best"
```

Assistant candidate indices remain stable within one assistant session, including
when the model calls `generate` more than once. Later searches append candidates;
`details` can still inspect earlier indices. Each generation call returns its own
top candidates. The session-wide 200-backtest budget still applies, including
`details` calls. Candidate indices are session-local; a new session does not preserve them. The CLI emits an
`Assistant candidate evidence` JSON line; the web API returns
`candidate_evaluations` on both completed and failed assistant responses. These
contain every generation outcome before top-five filtering, with the strategy,
seed and batch reference. `detail_evaluations` records each executed details rerun,
including failed evaluations, its tool-call reference and exact strategy. These
summaries are kept separate from full engine results. Dashboard sessions with candidate outcomes save both
summaries and `trial_executions` (including trades, equity and execution evidence)
with the frozen dataset, settings and engine identity in Saved research,
including outcomes from sessions that later fail. Existing backup/restore applies;
summary replay is explicitly unavailable. Prompts, answers and provider credentials
are excluded from this archive. These saved sessions count as known data exposure
for holdout checks. CLI sessions with candidate outcomes also archive automatically in `data/research`
relative to the working directory, printing an `Assistant archive` receipt. Provider
failure after generation preserves completed candidate summaries; archive failures
produce a nonzero exit without claiming that failed storage succeeded. Both entry
points now save an `assistant_trial_started` checkpoint before each backtest and an
`assistant_trial` result immediately after it. The records bind strategy, data,
configuration, engine identity, tool call and a unique session identifier. A
storage error stops further work in that session. Completed trial records survive
process interruption and appear individually in Saved research; pending checkpoints
also disclose potential holdout exposure. These record counts are not independent
trial counts. Provider-conversation resume remains unfinished; no provider credentials or
conversation text are checkpointed. In Saved research, **Verify replay** compares
a trial's complete result and **Restart frozen trial** reruns one checkpoint from
the beginning and saves its result. Both require the matching executable; neither
contacts the provider. A matching replay of a failed trial remains invalid.

CLI equivalents (the archive defaults to `data/research`):

```bash
sqx-cli list-assistant-trials
sqx-cli replay-assistant-trial --run-id RESULT_ID
sqx-cli replay-assistant-trial --run-id CHECKPOINT_ID --resume
```

A pending checkpoint does not establish whether another process is still running.
Repeated restarts deduplicate identical results; a contradictory existing result
causes an error rather than silently publishing a replacement. Portable standalone
result backups can be replayed without the original checkpoint or source CSV.

### CLI examples

```bash
# generate 300 random strategies, backtest, rank, print top 10
# All candidate outcomes are archived, including those excluded from ranking.
# Failed evaluations stop publication of a ranked databank and return a nonzero exit.
cargo run -q -p sqx-cli -- generate --symbol EURUSD --timeframe M1 --data data/eurusd_sample.csv --count 300 --top 10

# genetic evolution
cargo run -q -p sqx-cli -- evolve --symbol EURUSD --timeframe M1 --data data/eurusd_sample.csv --generations 20 --population 50

# backtest a strategy defined as JSON
cargo run -q -p sqx-cli -- backtest --strategy examples/rsi-mean-reversion.json --data data/eurusd_sample.csv --timeframe M1

# export a strategy to platform source code (mql4 | mql5 | pine | ninjascript | easylanguage | pseudocode)
cargo run -q -p sqx-cli -- export --strategy examples/rsi-mean-reversion.json --engine mql5
cargo run -q -p sqx-cli -- export --strategy examples/rsi-mean-reversion.json --engine ninjascript
cargo run -q -p sqx-cli -- export --strategy examples/rsi-mean-reversion.json --engine easylanguage

# full robustness cross-check set (what-if, MC trade, MC retest, walk-forward)
cargo run -q -p sqx-cli -- robustness --symbol EURUSD --timeframe M1 --strategy examples/rsi-mean-reversion.json --data data/eurusd_sample.csv

# walk-forward optimization
# Nonzero exit on invalid computation; exit 0 alone does not certify sufficient trade evidence.
cargo run -q -p sqx-cli -- optimize --symbol EURUSD --timeframe M1 --data data/eurusd_sample.csv --windows 6 --oos 0.25

# generate, optimize and robustness archive inputs and executions in data/research.
# Use the printed checkpoint run_id with the shared recovery commands:
cargo run -q -p sqx-cli -- pipeline-history --run-id <checkpoint-id>
cargo run -q -p sqx-cli -- replay-pipeline --run-id <checkpoint-id> --resume
# Recovery requires the exact original executable; rebuilding can change its identity.
# export-pipeline / import-pipeline also support these standalone study archives.

# portfolio: best subset of generated strategies (equal weights + correlation)
cargo run -q -p sqx-cli -- portfolio --symbol EURUSD --timeframe M1 --data data/eurusd_sample.csv --count 300 --top 12

# multi-market cross-check (explicit symbols and timeframes in a manifest)
cargo run -q -p sqx-cli -- crosscheck --strategy examples/rsi-mean-reversion.json --manifest examples/crosscheck-markets.json

# analysis report (monthly returns, drawdown periods, trade stats, rolling Sharpe)
cargo run -q -p sqx-cli -- analyze --timeframe M1 --strategy examples/rsi-mean-reversion.json --data data/eurusd_sample.csv

# AlgoCloud-style daily signals for a folder of strategies
cargo run -q -p sqx-cli -- cloud --symbol EURUSD --timeframe M1 --strategies /tmp/sqx_strats --data data/eurusd_sample.csv

# build -> retest -> optimize pipeline
cargo run -q -p sqx-cli -- pipeline --symbol EURUSD --timeframe M1 --data data/eurusd_sample.csv --count 200 --keep 20

# MCP server over stdio (JSON-RPC 2.0) for LLM clients
cargo run -q -p sqx-mcp

# dashboard on http://127.0.0.1:8080
cargo run -q -p sqx-cli -- serve --data data/eurusd_sample.csv --symbol EURUSD --data-timeframe M1
```

### Strategy JSON format

A strategy is a transparent rule tree (serde-serialized). See
`examples/rsi-mean-reversion.json`. Pseudo-code is rendered by
`sqx-cli pseudocode` and by the backtest output.

### Data format

CLI `backtest` and `analyze` now require `--timeframe` for CSV candles. Optional `--calendar`
loads an explicit UTC calendar JSON; `--allow-data-gaps` acknowledges unresolved
gaps for exploratory research and cannot override invalid or unfinished candles.
Tick execution also uses the declared timeframe; optional legacy `--tick-bar-ms`
must agree with it. Successful output includes a JSON `Research context` line
with source quality, calendar, cutoff and resolved execution settings. This
contract currently applies to `backtest`, `analyze`, `generate`, `optimize`, `montecarlo`, `robustness`, `portfolio`, `pipeline`, `assistant`, `evolve` and `start-evolution`;
other CLI research commands still need migration. `generate`, `optimize`, `montecarlo`, `robustness`, `portfolio`, `pipeline`, `assistant`, `evolve` and `start-evolution` also
require `--symbol` and accept `--config`; they no longer silently assume EURUSD. `analyze --config settings.json` loads a full BacktestConfig
(including account and transaction costs) and rejects a conflicting candle duration. CSV bar and tick timestamps are Unix **seconds**, not milliseconds.

`unexecuted_entry` preserves a final queued entry signal when no following opening
quote exists, including its direction, source candle and recorded signal values.
It is separate from entry attempts: no size, margin check or fill is claimed.
Single-candle inputs are evaluated for this boundary signal while remaining
untradeable without a next opening quote. The dashboard shows this evidence below
Entry decisions and distinguishes older records without boundary evidence.

`account_snapshots` samples balance, gross open PnL, commission reserve, accrued
financing, equity, required margin and free margin after each candle's execution.
An open position includes its ID, liquidation-side price and conversion rate.
Actual tick timestamps are recorded separately from candle labels. Terminal
liquidation adds a separate flat-account snapshot; the preceding mark-to-market
snapshot remains available, including any difference from exit slippage. These
are sampled valuations, not every intrabar margin check or a broker cash ledger.

Filled entry attempts include `initial_protection`: installed stop-loss and
profit-target levels, trailing/break-even flags and their fixed initial distance,
plus the completed candle reference used for ATR resolution. Unconfigured levels
are null; rejected entries have no installed protection record. This captures
engine state at entry, not broker order acceptance messages.

`stop_changes` records effective trailing/break-even amendments only on the chosen
execution path, with trade ID, reached price, previous/new stop and reason.
Observed ticks retain their quote timestamps; OHLC records use the candle label,
path name and ordered path step without inventing intrabar times. Alternative
OHLC scenarios and price observations after closure do not create executed
amendment records. These are effective protective-stop changes, not a full broker
order-message history.

`stop_out_decisions` records the modelled threshold, liquidation price before
slippage, final fill, applied conversion rate, balance, unrealized PnL, reserved
commission, accrued financing, equity, required margin and minimum permitted
equity for each stop-out. A continuous OHLC-path liquidation is modelled at the
threshold; a tick or gap may execute beyond it. Timestamp precision follows the
underlying execution mode. These records explain executed stop-outs, not every
margin check while the position was open.

Entry-attempt evidence includes an optional `margin_check`: normalized lots,
proposed price, conversion rate, leverage, required margin, spread/slippage and
commission reserves, equity after entry costs, free margin and admission result.
It is absent when earlier gates or lot constraints prevent reaching that check;
older records may lack the field entirely. The dashboard exposes the calculation
under Entry decisions. A proposed fill is not evidence of an executed order, and
this admission estimate is not a later margin-level history.

Backtest execution evidence includes `cash_settlements`: one record per closed
trade with gross quote-currency PnL, conversion rate, gross account-currency PnL,
commission, signed financing charge, net change and balances before/after. This
follows the engine's existing settlement-at-close model; it is not a broker's
daily cash-posting ledger. Spread and slippage are already included in trade
prices. `financing_accruals` records nonzero daily accruals with UTC rollover day,
recognition quote timestamp, lots, signed daily rate, multiplier and cumulative
charge. Gaps can delay recognition; these are accrued position costs, not separate
cash postings. Zero-charge days do not create records. Bar timestamps retain their
candle-label semantics, and end-of-data liquidation remains a reporting convention.

Walk-forward reports retain all in-sample candidate evaluations in original order,
including successful nonselected strategies. Each record contains the strategy,
metrics or execution errors, eligibility, exclusion reason and selection flag;
`selected_candidate_index` identifies the winner within its window. This records
selection evidence, not independent validation or a complete order ledger.

CLI `montecarlo`, `optimize` and `robustness` exit unsuccessfully if their
requested calculations report errors. The available research context is printed
before that failure. The combined `robustness` context includes stage-labelled
`errors`, including unavailable Monte Carlo distributions and incomplete retest
coverage. A successful exit means the calculations completed; it does not certify
sufficient trade evidence, an untouched holdout or suitability for live trading.

Standalone CLI `robustness` saves its exact executable identity, input data,
strategy, configuration and seeds before simulation (`--archive-dir` defaults to
`data/research`). Its journal retains the base result, each what-if configuration
and result, ordered Monte Carlo trade selections, retest strategies and offsets,
skipped/invalid outcomes, and walk-forward candidate and OOS executions. Storage
failure stops the study. The shared recovery commands above verify the recorded
prefix before appending missing events; recovery repeats computation and requires
the original executable. Portable backup/restore and process interruption are
tested; power-loss durability and large-study resource limits remain unverified.
The walk-forward stage searches fresh generated candidates; it does not validate
the supplied strategy out of sample. Standalone `montecarlo` and other entry
points do not inherit this archival coverage automatically.

Web stress tests now use the same checkpoint/recovery protocol for their six
cost scenarios and 1,000 trade-resampling paths. They save inputs before running,
and preserve completed steps if execution stops. In **Saved research runs**,
inspect the robustness study before retrying a request whose response was lost;
it may still be running. The whole-study backup includes the ordered journal.
Recovery requires the matching application build and repeats the recorded prefix
before continuing. Earlier single-report backups retain their original replay
route. This adds interruption evidence, not independent execution validation;
power-loss and large-study performance remain open. A real server-kill test also
checks partial-backup restoration and resume after removing the original CSV and
archive. To open an empty recovery workspace, use `serve --data-dir <empty-directory>
--data-timeframe M1`; an empty directory now permits dashboard startup for import
and recovery. Missing or malformed source files still fail their source checks.

CLI `crosscheck` uses `--manifest` instead of inferring instruments from a folder.
The schema-1 manifest contains a `markets` array with explicit `symbol`, `data`
and `timeframe` per entry; optional `calendar`, `config` paths and
`allow_data_gaps` apply per market. Relative paths resolve beside the manifest.
`examples/crosscheck-markets.json` contains the single bundled EURUSD dataset;
add explicitly declared datasets to compare multiple markets. All sources are
checked before evaluation. Invalid simulations remain in the output, are excluded
from eligible-market counts and make the command exit unsuccessfully. Too few
trades are reported separately and do not certify robustness.

For multiple dashboard datasets, prefer
`serve --data-manifest examples/dashboard-data.json`. This schema-1 manifest has
`datasets`, each with explicit `symbol`, `data`, `timeframe` and optional
`calendar` JSON path. Paths resolve relative to the manifest. Different timeframes
and arbitrary filenames are supported; duplicate symbol/timeframe entries and
invalid datasets reject the entire load before server startup. Gaps remain
visible for inspection and require separate disclosure before research. The
manifest is a source mapping, not a backtest-settings or crosscheck manifest.

Dashboard startup with `serve --data` requires `--symbol` and
`--data-timeframe`. Directory loading requires `--data-timeframe`, shared by all
CSV files; its existing convention uses each filename stem as the instrument,
so stems must be actual intended instrument identifiers. Mixed-timeframe folders
must not be loaded under one declared interval. CSV and QuestDB source flags are
mutually exclusive. Starting without a source still loads labeled M1 demo data.

CLI `cloud` requires symbol/timeframe and audits closed source candles before
producing raw entry indications. `FLAT` means no entry condition, not a request to
close an existing position. Disabled directions are respected; evaluation errors
and unsupported higher-timeframe confirmation show `UNAVAILABLE`, with a failing
exit status. This preview is not a position-aware trading or execution service.
Strategy directories are loaded in sorted order and rejected as a whole if any
JSON strategy cannot be read, parsed or validated.

For MCP research tools, a `data` path selects CSV and requires explicit `symbol`
and `timeframe` fields, for example `{"source":"csv","data":"prices.csv","symbol":"USDJPY","timeframe":"M1"}`
(with the tool's strategy/settings fields added). Without `data`, `symbol` selects
QuestDB and `timeframe` defaults to M5. Optional `source` accepts `csv` or
`questdb`; conflicting source/path combinations are rejected. CSV requests that
previously omitted symbol/timeframe must now provide them. Fixed candle duration
is derived from the timeframe; conflicting `config.bar_duration_ms` is rejected.
Calendar months are not assigned a nominal fixed duration.
MCP research also audits candles against the current server time. Invalid,
overlapping, duplicate or still-open candles are rejected. Unresolved gaps require
`allow_data_gaps: true` for exploratory runs, or an explicit `calendar` with UTC
weekly sessions and optional date overrides that explains market closures.
Successful research responses include `research_context` with the quality report,
full supplied calendar, cutoff time, symbol, timeframe and simulation settings.
These checks do not establish broker execution parity or an untouched holdout.

MCP `generate` automatically retains its engine, frozen candidate family and input
data, per-candidate checkpoints and full results before ranking. Storage defaults
to `data/research` relative to the server's working directory; the server operator
can set `SQX_MCP_RESEARCH_DIR` to an absolute persistent directory. Storage failure
aborts the search. `generation_history` discovers saved searches after a lost
response or process restart, without loading the original CSV or contacting QuestDB.
Keep this archive private: it contains market data, strategy rules and account
settings. Repeated requests receive distinct search identities and are not
automatically independent trials. Missing results do not prove that a process
stopped. `generation_trial` with `run_id` replays a saved result; with a trial
checkpoint ID and explicit `restart: true`, it restarts only that candidate from
frozen inputs. The running executable must match the original engine. Candidate,
search and result relationships are checked before execution; conflicting saved
outcomes are never replaced. This does not advance other candidates or complete
the search, and successful replay is not independent correctness evidence.
`generation_export` with `search_id` returns one JSON backup containing the frozen
search and all linked checkpoints, candidate results and completion records.
Version 2 transport stores shared datasets and strategies once, keyed by their
existing content hashes. Version 1 backups remain importable.
Export takes the search writer lock, rejects missing/conflicting relationships,
and preserves incomplete status. Failed and rejected candidates remain included.
It does not run strategies or include engine binaries, unrelated experiment or
holdout records. Content hashes are not authenticated provenance. `generation_import` accepts the returned object as `backup`, checks its complete
graph in a private temporary archive, then checks the merged destination history
under its writer lock before publication. Conflicting results are never replaced.
Repeated import is idempotent. Storage failure may leave a partial import; retry
the same backup after resolving the error. The response reports engine-copy
availability separately; original-engine replay remains a distinct operation.
Import currently expands shared objects internally for graph validation; compact
transport is not a guarantee of bounded peak memory.
Representative-size transport, disk/power-loss tests and UI integration remain
open. Neither operation restores unrelated holdout or experiment evidence.

`generation_resume` with `search_id` replays recorded candidate outcomes and then
finishes missing candidates from the frozen family. It requires the original
engine and an exclusive process lock; an active search is refused. Repeating it
verifies completed work without creating a new search or duplicate result records.
A completion record means all evaluations were saved; failures still produce a
tool error and do not become valid strategies. Recovery replays the saved prefix
rather than skipping its computations. Durable capture of other MCP tools remains
unavailable.
Dashboard downloads preserve one record, not the whole linked search; JSON
research backups do not include the retained executable.

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
cargo run -q -p sqx-cli -- generate --symbol EURUSD --timeframe M5 --data data/mt5/EURUSD_M5.csv --count 300
cargo run -q -p sqx-cli -- crosscheck --strategy examples/rsi-mean-reversion.json --manifest examples/crosscheck-markets.json
cargo run -q -p sqx-cli -- serve --data-dir data/mt5 --data-timeframe M5
```

Export ticks with `--ticks` (writes `<symbol>_ticks.csv`). MT5 bar times are
broker server time; the engine uses relative bar intervals, so backtests are
unaffected. `data/mt5/` is gitignored (your broker data stays local).

### QuestDB koersdata (192.168.10.33)

Reads OHLCV candles straight from the AlgoTrader QuestDB `candles` table
(`symbol, timeframe, ts, open, high, low, close, volume`, base timeframe
`1m`). Higher timeframes are aggregated server-side with `SAMPLE BY`.

Connection defaults: `http://192.168.10.33:9000` (HTTP REST), optional Basic
auth `admin`/`quest`. Override with env vars `SQX_QUESTDB_HOST`,
`SQX_QUESTDB_PORT`, `SQX_QUESTDB_USER`, `SQX_QUESTDB_PASS`.

```bash
# list symbols in the candles table
cargo run -q -p sqx-cli -- questdb symbols

# fetch the last 5000 M5 bars and print a summary
cargo run -q -p sqx-cli -- questdb bars --symbol EURUSD --timeframe M5 --limit 5000

# ... and write them to CSV, then feed any existing command
cargo run -q -p sqx-cli -- questdb bars --symbol EURUSD --timeframe M5 --limit 20000 --out data/questdb/EURUSD_M5.csv
cargo run -q -p sqx-cli -- generate --symbol EURUSD --timeframe M5 --data data/questdb/EURUSD_M5.csv --count 300

# serve the dashboard straight from QuestDB (no CSV needed)
cargo run -q -p sqx-cli -- serve --questdb --questdb-symbols EURUSD,GBPUSD --questdb-timeframe M5
```

`serve --questdb` loads the most recent `--questdb-limit` (default 5000) bars
per symbol into the dashboard. If the host is unreachable the command fails with
a clear transport error rather than silently serving synthetic data.


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

Remaining reliability work and release priorities: [TODO.md](TODO.md).

## Tests

Run `cargo test --workspace` and `cargo build --workspace`. The broader local
gate, `bash ci/check.sh debug`, also checks dashboard JavaScript regressions.
The latest debug gate passed 377 Rust tests, 16 frontend checks and ten Python
export/normalization tests, plus the workspace build. The previous release gate
passed 376 Rust tests, 16 frontend checks and five Python exporter tests; the
latest broker-clock header changes have not yet had a full release rerun. The
profile comparison also passed four execution fixtures and two seeded evolution
fixtures with byte-identical evidence (excluding executable identity in evolution
headers).
Passing tests alone does not establish independent execution parity; see
`RELIABILITY.md` for the evidence and remaining completion requirements.

New engine results carry account-evidence schema 1. Before returning a valid
result, the engine reconciles cash settlements, currency conversion arithmetic,
financing accruals, sampled account components and final equity. Metrics reject
inconsistent schema-1 evidence. Numeric reconciliation uses a relative tolerance
of 1e-10 with a unit floor. Legacy and aggregate schema-0 results remain usable
for metrics, but cannot pass the strict account-evidence validator. This is an
internal consistency check, not independent verification of market prices, every
order transition, or broker execution.

## License

MIT. See Cargo.toml.

CLI backtests automatically retain full research evidence in `data/research`
relative to the working directory. Override it with
`backtest ... --archive-dir path/to/research`. The content-addressed archive stores
the parsed candles/ticks, instrument, strategy, account/execution settings, source
quality context, executable identity and full result (including trades and equity).
A receipt is printed only after storage succeeds; storage failures return nonzero.
This default applies to the standalone `backtest` command; it does not imply
complete recording of every other research command. CLI records support archive inspection/backup. Replay them with
`replay-backtest --archive-dir path/to/research --run-id <id>`. Replay requires the
exact recorded executable and compares the full engine result using only archived
inputs; mismatches return nonzero. Dashboard replay is not yet supported.

Archived CLI backtests in an experiment's research archive are included in its
prior-data-use audit, including recorded failures. A CLI subset count identifies
their source; it is not an additional independent trial count. Archives in other
directories and unrecorded work remain outside that audit.

Tick CSV accepts `timestamp,bid,ask` (integer Unix seconds) or the explicit
`timestamp_ms,bid,ask` header (integer Unix milliseconds). Headerless input
remains seconds; units are never guessed from magnitude. The MT5 exporter now
writes the millisecond form from `time_msc` and uses an explicitly UTC request
date. Existing second-resolution files cannot recover discarded milliseconds.

MT5 bar export excludes the terminal's current candle. If any requested symbol
cannot be selected or returns no data, the exporter reports a partial export and
returns nonzero; successful symbols may already have been saved. Received row
counts do not guarantee that the terminal supplied all requested history.

A CLI backtest publishes an immutable input checkpoint
before simulation and prints its ID. Restart a checkpoint with
`replay-backtest --resume --archive-dir path/to/research --run-id <checkpoint-id>`.
This restarts the calculation from frozen inputs using the matching executable;
it does not continue from an intermediate candle. Completed records link to their
checkpoint, and repeated restarts converge on the same content-addressed result.
Checkpoints count as conservative prior data exposure even without a result.
Find checkpoint IDs after losing terminal output with
`list-backtests --archive-dir path/to/research`. The JSON lists checkpoint identity,
linked result IDs, execution validity and whether the current executable matches.
`result_not_recorded` does not prove the process stopped: it may still be running.

`list-backtests` and `replay-backtest` use the same `data/research` default. An
unwritable archive stops the operation before simulation; storage failure is not
silently converted to an unrecorded run. Each working directory has its own
default archive, so use a shared explicit archive for unified holdout auditing.

### Explicit forex margin conversion

Account configuration supports `margin_model: "forex_base"` for declared forex
base-unit margin. It uses contract units × lots × base-to-account rate / leverage,
separately from profit conversion. For a third-currency account, supply historical
`margin_conversion_rates` entries with `time` (normalized epoch milliseconds),
`from` (instrument base currency), `to` (account currency), `long_rate` and
`short_rate` (account units per base unit). Missing, stale, duplicate or invalid
observations fail validation. The existing `max_conversion_age_ms` also limits
margin observations. Direct base-account conversion is one; a quote-account uses
the contemporaneous traded price.

The default `quote_notional` model preserves the prior price × profit-conversion
margin assumption. It is not broker parity for every currency combination. The
new policy is applied to entry admission, account samples and stop-out, and its
actual margin conversion rate is recorded separately in execution evidence.
It does not implement broker-specific tiered, hedged, exchange or CFD margin.
For third-currency profit conversion, `profit_conversion_model: "directional"`
uses `profit_conversion_rates` with the same time/from/to/long_rate/short_rate
structure, but from the instrument's quote currency to the account currency.
Both directions can coexist in one strategy/configuration. Missing or stale
observations fail; there is no fallback to single-rate observations in directional
mode. The default `single_rate` retains the prior convention. Direct quote-account
conversion remains one; direct base-account conversion uses the current price.
Rates are explicit assumptions, not an automatically inferred broker convention. MT5 snapshots currently
cover one-lot EURUSD/EUR and USDJPY/EUR calculations, not actual fills.

### MT5 broker-clock normalization

New MT5 exports use `timestamp_broker` / `timestamp_broker_ms`; SqxLabs rejects
these raw headers until normalized. MT5 timestamps must not be assumed UTC.
Preserve the raw export and convert
with an explicit policy before importing the normalized CSV:

```bash
python3 scripts/mt5_normalize.py --input data/mt5/EURUSD_M5.csv --server TickmillEU-Demo --policy examples/tickmill-clock-2026.json --bar-duration-ms 300000 --out data/mt5/normalized
```

For ticks, omit `--bar-duration-ms`. For older exports bearing ordinary
`timestamp`/`timestamp_ms` headers, `--legacy-broker-clock` explicitly declares raw
broker time; never use this switch on UTC data. Without this declaration, ordinary
UTC headers are rejected by the normalizer to avoid double conversion.
The command prints the immutable bundle and
`utc.csv` paths. Bundles retain raw data, exact policy, normalized CSV and hashes.
The example deliberately supports bounded periods and excludes uncertain clock
transition days; unsupported rows fail rather than being dropped or guessed.
Do not use this policy for another broker. Normalization alone does not prove
price quality or tick completeness. Automatic provenance-aware app import remains
unfinished; see MT5_VALIDATION.md.

CLI research commands that expose `--normalization-bundle` can bind the original
clock evidence to the supplied candles:

```bash
sqx-cli backtest --strategy /path/to/strategy.json --data /path/to/bundle/utc.csv --symbol EURUSD --timeframe M5 --normalization-bundle /path/to/bundle
```

Use your actual strategy path. The bundle verifier checks hashes, clock arithmetic,
unchanged non-time fields, chronology, candle duration and exact supplied bars.
When the research route archives its source context, the original raw CSV, policy,
normalized CSV and manifest are embedded, not merely referenced by a local path.
CLI backtest replay/resume rechecks embedded provenance without the original files.
This is transformation evidence, not authenticated broker provenance. Dashboard imports still need integration.
For tick backtests, also pass `--ticks /path/to/ticks/utc.csv` and
`--tick-normalization-bundle /path/to/ticks/bundle`. This verifies and archives
the exact tick sequence, including order within equal timestamps. It does not
certify that the broker supplied every market tick.


Cross-check manifests accept an optional `normalization_bundle` per market.
Both data and bundle paths are resolved relative to the manifest. Every market
is validated before execution; a mismatched bundle aborts the entire preflight.
The report retains the verified bundle in each market's context. This does not authenticate the declared symbol.
Dashboard data manifests do not yet support this field.

Cross-checks now archive their complete input before execution and their full
per-market execution results plus summary afterwards, including invalid runs.
Use `--archive-dir` to override the default `data/research`. The result references
its starting checkpoint. Storage failures are fatal. Use `replay-crosscheck --run-id ID --archive-dir DIR` to compare a result,
or add `--resume` for a starting checkpoint. This requires the exact original
executable, revalidates embedded provenance and compares the full result. Resume
restarts all markets from frozen input. A checkpoint alone does not indicate
whether a process is running or interrupted. Exact replay is not independent
broker validation.


For a read-only common historical window, run `scripts/mt5_capture_window.py`
with the SiliconMT5 venv Python, `--symbols EURUSD,USDJPY,EURJPY`, explicit
`--broker-start` / `--broker-end` epoch seconds and `--out DIR`. Bounds must be
minute-aligned and at most one hour apart. The command uses only local read APIs
and closes its own connection. It preserves full raw responses, numeric CSV,
selected account/instrument metadata and host request times in a content-addressed
capture.json. The requested window is half-open; the inclusive tick API response
is retained separately from ticks inside the window. Missing candles or invalid
prices fail the capture. Sequential reads are not an atomic snapshot; tick
completeness, clock correctness and conversion freshness still require validation.
This bounded capture is not yet a resumable live recorder.

The window collector additionally retains a preceding tick interval, default
60 seconds (`--warmup-seconds`, 0–3600), separately as raw_warmup_ticks and
warmup_ticks_csv. This can supply already-known conversion quotes at the first
trading timestamp. Consumers must still enforce their declared freshness limit
and cross-symbol equal-timestamp ordering convention.


Run `python3 scripts/mt5_reference_check.py --capture CAPTURE_DIRECTORY --policy
CLOCK_POLICY_JSON` from the repository root for an independent Decimal check of
eight USDJPY tick trials (constant-entry, close-versus-SMA(2), stop-loss and signal exit, each long/short) on a captured EUR account dataset. The script
requires the built CLI, normalizes source bundles, freezes account conversion
rates, archives and replays each trial, and writes reference/comparison.json.
If a SMA(2) direction has no executable signal in the fixed capture, the script
verifies zero trades and unchanged equity, continues the remaining cases, and
returns nonzero `coverage_incomplete`. Correct no-trade behavior is not evidence
that entry/fill/cost execution for that direction was exercised. Per-case progress
is retained if a later check fails; do not select a different subwindow merely to
turn missing coverage into a passing report.
It independently calculates the first eligible signal and SMA(2) value, and checks
entry/exit prices and times, commission, net profit, final equity and
entry margin. Scope is fixed 0.1 lot, EUR 10,000, 1:30 leverage, 0.4-pip stops for stop cases (one entry per day),
terminal liquidation otherwise, EUR 3.50 round-trip commission per lot and one broker-point
slippage per fill. These are explicit test assumptions, not certified broker fees.
SqxLabs FX points are pips: USDJPY point=0.01, whereas the captured MT5 broker
point=0.001; the reference therefore passes --slippage 0.1. Equal-time conversion
quotes use last-in-response at-or-before-event, with a 10-second age limit.

Signal-exit reference cases close at the first tick after the exit-signal candle,
with one entry per day. A new checker attempt replaces the previous summary with
verification_not_completed before executing trials; success records the checker
and executable SHA-256 hashes. Python -O is rejected so assertions remain active.
A passed result certifies only the explicit fixture checks listed in the report.


For a future-data invariance check, run `python3 scripts/mt5_prefix_check.py
--capture CAPTURE_DIRECTORY --policy CLOCK_POLICY_JSON --out OUTPUT_DIRECTORY`.
It creates an explicitly synthetic derivative with only the last USDJPY candle
and its bid/ask ticks shifted by 0.050, reruns all eight reference cases and
compares earlier entry evidence, settlements and tick-timed account snapshots.
At least one terminal result must change, so an ineffective mutation cannot pass.
The original capture is preserved. This is a bounded regression, not a general
proof against every possible look-ahead mechanism.

### Restartable MT5 window recording

Use the SiliconMT5 venv Python to run `scripts/mt5_record.py --symbols
EURUSD,USDJPY,EURJPY --server TickmillEU-Demo --broker-start BROKER_EPOCH_SECONDS
--windows 10 --out RECORDING_DIRECTORY`. Windows default to one minute, with
60 seconds of preceding conversion ticks. Restart with the exact same start,
symbols, server, window and warmup settings; the immutable registry derives the
next window from committed records. `--follow` waits for newly closed windows,
bounded by `--windows` for each invocation. Without it, an unavailable next window
returns an explicit waiting status. The bridge is local and read-only.

Each capture is stored by SHA-256 before its commit record is published. A process
failure between these writes leaves an orphan capture but does not advance the
cursor. Startup verifies committed hashes, continuity and account context. One
writer holds an OS lock; missing candles, corrupt records or changed configuration
stop recording. ConnectionError, TimeoutError and EOFError trigger bounded reconnect attempts
(default 3, configurable with --reconnect-attempts 0–10), with backoff up to
30 seconds. The same uncommitted window is retried. Other validation/storage
errors stop recording. Exhausted retries require restarting the command.
Trading-calendar gaps and long-duration live validation remain unfinished. This is closed-window polling, not a certified lossless tick
stream. Power-loss durability and deletion of a final record are not certified.

Export a committed recording range offline with `python3
scripts/mt5_recording_export.py --journal RECORDING_DIRECTORY --broker-start START
--broker-end END --out EXPORT_DIRECTORY`. The writer may remain active: export snapshots atomic committed records and
ignores uncommitted capture blobs. Select complete contiguous windows, at most
one hour. The export verifies the journal, preserves each original capture and commit record, retains
all main ticks in order, and uses only the first selected window's warmup to avoid
duplicating overlapping warmup observations. The output is a content-addressed
capture directory accepted by mt5_reference_check.py. It does not certify complete
broker ticks or historical instrument settings; exact original responses are in
recording_provenance, while the top-level response is explicitly reconstructed.

Inspect an existing recording without connecting to MT5 or modifying its files:
`python3 scripts/mt5_recording_status.py --journal RECORDING_DIRECTORY`.
The JSON report verifies committed captures and shows window count, broker-clock
range, next cursor, account context and instrument specifications. It can run while
the recorder is active. It deliberately reports writer_status=not_observed: a
recording directory or lock file alone does not prove a recorder is running.
Corrupt or missing input produces integrity=not_verified and a nonzero exit code.
