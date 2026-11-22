# StrategyQuant X — complete feature inventory

Source: https://strategyquant.com/features/ (rendered) + the site's documentation
pages discovered with `katana` (ProjectDiscovery katana v1.8.0, depth 3,
root-domain) + the shipped-app screenshots. This is a reference inventory,
not a claim that every item is implemented in this port.

## Products / modules

| Product | Role |
|---|---|
| StrategyQuant X | Generate, backtest, optimize and stress-test strategies |
| Q AI Assistant | AI assistant / MCP server (talk to SQ from an LLM client) |
| AlgoCloud (+ Stockpicker) | Cloud stock-trading platform and its SQX engines |
| QuantAnalyzer | Post-analysis of strategies and portfolios |
| QuantDataManager (QDM) | Market-data download, import, cleaning, export |
| AlgoWizard | No-code visual strategy editor (drag/condition builder) |
| CodeEditor | Edit/compile custom Java snippets, indicators, blocks |

## Core capabilities (from /features/)

1. Strategy creation / generation (random + genetic evolution, unlimited strategies)
2. Overfitting protection and robustness testing
3. Integrated free data download (Dukascopy forex, Yahoo Finance stocks)
4. Data subscription feed
5. Very fast backtesting engine with real-tick precision
6. No-code approach
7. Multi-timeframe and multi-symbol strategies (signals per chart)
8. All standard technical indicators + candle formations + 4 entry types
9. Custom blocks (own snippets)
10. Export full source code: MT4, MT5, TradeStation, NinjaTrader
11. Portfolio support
12. Existing-strategy Improver
13. Retester and Optimizer modes for existing strategies
14. Display trades on chart / candle chart
15. Multiple OOS periods (unlimited)
16. MAE / MFE / daily equity chart
17. Fit-to-portfolio functionality
18. Automatic recognition of strategy problems (flaw detection)
19. Extendable with own indicators and models
20. Integrated environment: SQ + QuantAnalyzer + QDM + AlgoWizard

## Robustness / cross-checks

- What-if simulations (vary spread/slippage/commission)
- Monte Carlo trade manipulation
- Monte Carlo retest methods (random start / parameter jitter)
- Higher backtest precision
- Backtests on additional markets (multi-market)
- Sequential optimization
- Opt. profile / system parameter permutation
- Walk-forward optimization (WFO)
- Walk-forward matrix

## Optimizer

- Types: simple, sequential, walk-forward, walk-forward matrix
- Period type: percent / days / bars (TradeStation compatible); floating / fixed
- Out-of-sample %, walk-forward runs
- Automatic parameter settings: max optimizations, up/down % distribution, steps
- Strategy selection: file or all strategies in a databank

## Money management (10 methods)

Fixed size, Crypto fixed % balance, Risk fixed % balance, ATR risk-based sizing,
Risk fixed % of account, Crypto fixed amount, Fixed amount, Crypto size by price,
Stocks size by price, Simple Martingale MM.

## Trading engines

MetaTrader4, MetaTrader5 (netting / hedged), TradeStation, MultiCharts, JForex,
NinjaTrader8, AlgoCloud Stockpicker, AlgoCloud Single-asset.

## Data Manager (QDM)

- Sources: Dukascopy, TickDownloader import, File import, SQ Equity data,
  SQ Futures data, Darwinex Tick Data, Crypto, Yahoo, MT5 import
- Instruments, Sessions, External indicators, Stock groups, Broker profiles
- Update all / update selected / mass delete / save / load
- Export: MT4 FXT/HST, MT5, configurable CSV

## Strategy styles

- SQ3: separate IF-THEN long/short entry and exit rules
- SQ X: one signal rule computes all signals; entries/exits react to signals
- SQ X + fuzzy logic: a signal is valid when N% of its sub-conditions are true

## Results / databank

- 120+ databank metrics (profit, drawdown, ratios, trade stats, consistency)
- Equity chart, trades on chart, MAE/MFE, daily equity, multiple OOS periods
- Ranking / fitness (Net profit, Return/DD, R-Expectancy, CAGR/MaxDD, weighted)
- Custom conditions (hard filters), custom analysis hooks

## Extensibility

- Custom Java snippets (indicators, blocks, money management, databank columns)
- Custom blocks / custom data indicators (AlgoWizard XML)
- External MCP servers (e.g. MetaTrader 5) so the app can use other tools

## Mapping to this Rust port

| Area | Port status |
|---|---|
| Generation (random + genetic) | done |
| Backtesting engine | done (bar-level; intrabar path option) |
| Optimization | done (genetic + walk-forward) |
| Robustness | done: what-if, MC trade, MC retest, multi-market, walk-forward |
| Multi-timeframe | done (higher-TF confirmation signal) |
| Money management (10) | done |
| Trading options (EOD/Friday/max trades) | done |
| Data Manager (multi-symbol CSV) | done (CSV import; no Dukascopy/Yahoo/MT5 download) |
| Portfolio (combine, correlation, best subset) | done |
| Custom projects pipeline | done (Build -> Retest -> Optimize, GoToTask) |
| AlgoWizard-style editor | done (signals/conditions + MM/trading-options sidebar) |
| Export MQL4/MQL5/PineScript | done |
| Export NinjaTrader / EasyLanguage | missing |
| Tick-precision backtests | partial (needs tick data) |
| Indicators | 76 base (228 building blocks with Rising/Falling variants) of ~250 |
| MCP server (engine as tools) | done (sqx-mcp) |
| Q AI Assistant / AlgoCloud / QuantAnalyzer | not ported |
| Custom Java snippets / AlgoWizard custom blocks | missing (Rust blocks are compiled in) |

## How this inventory was produced

1. `katana -u https://strategyquant.com/features/ -d 3 -cs rd -jc -jsonl`
   (25,325 records; 48 clean 200 pages: docs in en/de/fr/es/it/pt/ja + wp-json).
2. Headless re-crawl with system Chrome produced no output here (driver).
3. The rendered `/features/` page was read with a real browser (the site
   blocks plain HTTP clients with 403).
4. Documentation pages found by katana (walk-forward optimization/matrix,
   multi-orders, custom blocks, custom analysis, system requirements) were
   used to confirm the advanced features.
