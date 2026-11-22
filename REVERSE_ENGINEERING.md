# Reverse-engineering findings — StrategyQuant X 1.45 (arm64)

Target: /Volumes/E-512GB/Applications/StrategyQuantX145_arm.app (1.0.1,
com.strategyquant.strategquantx). Scope: shipped-artifact inspection only
(packaging, bundled docs, class-name inventory, native launcher strings).
No proprietary bytecode was decompiled — the clean-room boundary from the
workspace DECISIONS log was respected.

## 1. Runtime model

- **Java application** with a bundled Azul Zulu JDK 25 (Resources/j64).
- **Native launchers are Go binaries** (Resources/StrategyQuantX, sqcli,
  CodeEditor). Strings confirm they decrypt the proprietary jars
  (extension .jar.enc, obfuscated symbol (*BYIs54Vi).Encrypt) and boot the JVM
  via JNI (GetJVM -> com.strategyquant.boot.SQAppBoot), then
  com.strategyquant.strategyquant.SQStarter (GUI) / SQConsoleStarter (CLI) /
  CEStarter (CodeEditor). Launcher (MacOS/) is a universal x86_64+arm64 stub.
- Config files carry the JVM entry points and -Xms4g / native-access flags.

## 2. Packaging

- internal/libs: ~130 third-party jars (commons-*, jackson, guava, fastutil,
  freemarker, h2, artemis/ActiveMQ, agentscope, caffeine, commons-math3, ...)
  plus the encrypted proprietary libs: SQAppLib, SQDataLib, SQLib,
  SQPluginLib, SQGridLib2, SQJobsLib, StrategyQuant, CodeEditor (.jar.enc).
- Snippets.jar is unencrypted: 1069 classes that define the open domain model.
- internal/plugins: 182 plugin folders mapping the modules (AppBuilder,
  AppOptimizer, AppRetester, AppPortfolioMaster, AppPortfolioComposer,
  CrossCheck*, DataManager*, DataSource*, CryptoExchange*, Connection*).

## 3. Domain model (from Snippets.jar class inventory + docs/)

- Building blocks: 250+ across ~52 indicator families (MACD, CCI, RSI,
  Stochastic, Bollinger, Keltner, ATR, ADX/DI, Ichimoku, AnchoredVWAP, VWAP,
  COT, VolumeProfile, SuperTrend, ParabolicSAR, Aroon, AwesomeOscillator,
  Williams %R, DeMarker, Momentum, ROC, LinReg, ...) plus comparisons, price
  fields, bar/time blocks, order blocks (EnterAtMarket/Limit/Stop/Reverse),
  strategy-control blocks, candle patterns, and functions.
- Databank columns: ~120 metrics (profit/return, drawdown/risk, ratios/quality,
  trade statistics, consistency/robustness, time/cost).
- Money management: FixedSize, FixedAmount, RiskFixedBalancePct,
  RiskFixedPctOfAccount, ATRRiskBased, SimpleMartingale, Stocks/Crypto/Picker
  size-by-price.
- Exit methods: StopLoss, ProfitTarget, TrailingStop, MoveSL2BE, ExitAfterBars.
- Strategy styles: SQ3 (IF-THEN), SQ X (signal rule), SQ X + fuzzy logic.

## 4. Architecture (from bundled docs/)

App -> Projects -> Tasks -> Settings, with per-project Databanks (source ->
target), Progress, and Results. Persistence: user/projects/<name>/project.cfx =
flat ZIP with config.xml (ordered task list) + one *-TaskN.xml per task, plus
databanks/. Builder / Retester / Optimizer are single-task projects.

Lifecycle: Data (QDM) -> Build (Builder) -> Store (Databank) -> Validate
(Retester: cross-checks, Monte Carlo, WFO) -> Tune (Optimizer) -> Combine
(Portfolio Master/Composer) -> Deploy (export to MT4/MT5/NinjaTrader/EasyLanguage/
JForex via FreeMarker templates).

## 5. Mapping to this port

The Rust crates implement the core of this model: the strategy AST and
building-block catalog (sqx-strategy), the indicator engine (sqx-indicators),
the backtester (sqx-backtest), the statistics (sqx-metrics), random + genetic
discovery (sqx-optimizer), persistence (sqx-project) and a CLI (sqx-cli).
Not re-implemented: the Electron/web UI, the encrypted data sources and
broker integrations, the cloud (AlgoCloud/Stockpicker) platform, and the
platform code exporters.
