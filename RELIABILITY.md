# Professional reliability programme

Status: in progress. Passing existing tests does not certify trading correctness.
Existing databanks were produced by an execution model with known material defects;
they must be rerun after the execution model is validated.

## Completion requirements

1. Accounting and execution: applied spread/commission/slippage; account-currency
   conversion; margin, leverage, rejected orders and stop-out; causal ATR sizing,
   exits, trailing and break-even; executable gap fills; financing. Evidence:
   hand-calculated long/short fixtures and account-ledger reconciliation.
2. Explicit simulation contract: quote basis, information availability, order
   lifecycle, time/session boundaries; OHLC ambiguity reports and adverse/favourable
   bounds; tick execution with documented data coverage and limitations.
3. Independent evidence: golden scenarios, future-perturbation tests, deterministic
   reruns, currency/margin fixtures, independent indicator and strategy reference
   implementations, regression coverage for every repaired defect; release gates.
4. Reproducible data and runs: data quality reports (including market-calendar-aware
   gaps), immutable archived datasets, content identities, engine/model versions,
   parameters/seeds/splits, and complete order/fill/trade/results records; replay.
5. Research validity: pre-OOS warmup without pre-OOS trades, capital continuity,
   untouched holdout, configurable evidence thresholds, cost/parameter stress,
   candidate trial accounting and consistent portfolio currency/time coverage.
6. Operational workspace: persisted resumable jobs; versioned strategies/runs and
   comparisons; per-trade explanations; evidence-based validation state; atomic
   storage, backup/restore; appropriate authentication/access controls; bounded AI
   actions referencing persisted evidence; paper-trading reconciliation workflow.

All six sections remain open until supported by implementation and inspected
verification evidence. Scope must not be reduced to the currently passing tests.

## Current completion audit (supersedes historical pending notes below)

This table is an incomplete-work audit, not certification. Later sections are an
append-only implementation history; an older “pending” statement may have been
resolved by a later increment. The numbered requirements above remain unchanged.

| Requirement | Current inspected evidence | Still required before completion |
| --- | --- | --- |
| 1. Accounting/execution | Account fixtures and engine/tick tests exercise costs, conversions, margin, stops and financing. Frozen MT5 calculation references cover four EURUSD/EUR and four USDJPY/EUR cases; the opt-in forex-base margin model separates margin conversion from profit conversion (not actual-fill parity). Schema-1 results reconcile settlement chains, financing accruals, sampled account components, admission/exit/stop-out correspondence, effective stop chains and final equity; inconsistent evidence is rejected by metrics. | Independent end-to-end execution comparisons across supported instrument/account combinations; complete auditable account/order event reconciliation. |
| 2. Simulation contract | `ExecutionEvidence` stores entry attempts, scheduled exit decisions, tick coverage and currency; `BarAmbiguity` explicitly limits two-path comparisons to local scenarios. | Full intrabar order lifecycle and state transitions; distinguish local path scenarios from global adverse/favourable bounds; remaining confirmation/calendar coverage cases. |
| 3. Independent evidence | Indicator cross-checks use pinned Rust reference packages with seed/convention exceptions. Separate Decimal references cover eight scoped USDJPY/EUR strategy cases, and integer-price fixtures cover walk-forward and symmetric/dynamic stop cases. Run 45392742726 passed Linux/macOS debug/release checks and profile comparisons for f15c790; later commits require their own evidence. The isolated MT5 mathematical tester runs, but no market-mode strategy comparison has completed. | Coverage of every advertised indicator/convention and supported execution feature; independently sourced MT5/broker strategy/fill fixtures; durable release artifacts and successful required checks for each released revision. The four matrix checks are now required on main with strict/admin enforcement; green checks do not prove the broader validation requirements. |
| 4. Reproducible records | Immutable inputs/results, restore/replay and process-kill tests cover selected CLI, assistant and evolution routes. Browser imports persist as immutable versions and can be explicitly reloaded after restart, including optional calendar/normalization evidence. Selected routes automatically retain exact engine bytes; CLI evolution inspection distinguishes missing and verified copies. MCP generation preserves every candidate checkpoint/full result and supports single-trial and whole-search recovery with engine checks, replay and writer exclusion; recovery reproduces all 64 outcomes of a separate uninterrupted fixture. MT5 journals support verified export and process resume; new captures bind server/login through a private fingerprint. | Complete recording/conversion provenance and durable accounting on all research routes; legacy missing-engine recovery, provider-conversation recovery and bounded retained-result memory. Executable copies do not preserve runtime dependencies. |
| 5. Research validity | Walk-forward warms from history, carries capital and records candidate outcomes/selection; an independent integer-price SMA fixture checks segmented selection and capital. Holdout audits include declared candle extents, assistant/crosscheck/comparison/evolution exposure and MCP generation phases; record counts remain distinct from independent trial counts. Commitment evidence survives experiment restore. | Complete durable trial accounting across every entry point and independent boundary/evidence review. Search-history completeness and no-known-overlap labels do not establish untouched data outside the registry. |
| 6. Operational workspace | Resumable evolution and MCP generation, raw backups, saved comparisons and bounded assistant tools have tests. UI trade explanations show protection, financing, stop-out and settlement; overview explains limited evidence. A 15-window read-only live capture matched all 45 bid candles but failed the declared conversion freshness limit at 13 USDJPY events; it retained legacy account binding. A separate fingerprint-bound recording resumed across processes. A prospective five-window capture passed observed OHLC/freshness checks; seven execution cases matched independent arithmetic, while one absent SMA signal left execution coverage incomplete. HTTP remote addresses are rejected. | Broader live recording/outage/power-loss validation, complete evidence/recovery UI, AI provider-conversation recovery, known-strategy paper reconciliation and export parity. Remote authenticated service is not supported. |

### Recovery integration evidence (2026-11-24)

Integration PR #1 merged into `main` as `072f4f7`, retaining all commits originally
reviewed separately in PRs #2–#5. All four required platform/profile checks passed
on its head `027a048`; the post-merge main run is a separate verification step.
The earlier draft PRs are superseded. This establishes source integration, not
a packaged release or broad trading validation:

- PR #1 freezes complete CLI pipeline inputs and ordered events, supports exact-
  engine replay/resume and portable whole-search backup validation. Source-deletion
  and build-phase process-kill recovery match uninterrupted execution.
- The original PR #2 adds a hand-calculated EURUSD/USD holdout fixture: SMA3 warmup, first
  holdout fill, long/short spread and commission, initial capital, future-data
  isolation and persistent commitment. Debug and release references pass locally.
- The original PR #3 records every pipeline walk-forward training and OOS execution before
  proceeding. A real process kill followed by export, source-archive deletion,
  import and resume matches all 48 candidate outcomes and complete event IDs.
- The original PR #4 shares pipeline archive/recovery code between CLI and dashboard. Server
  and browser tests cover grouping, inspection, raw backups, replay, execution
  errors and build mismatch. A desktop browser check on an isolated synthetic
  fixture visibly confirmed replay; broader responsive/accessibility coverage
  remains open.

The original PR #5 avoids retaining all archived execution payloads during
structural inspection and replay. A real filesystem permission-failure fixture
checks that recovery publishes no further event and can resume after write
access is restored. Full export and simulator memory remain unbounded.

These strengthen requirements 4–6 but do not close them. Large-search memory,
power loss/disk-full interruption, runtime packaging, all-route trial accounting,
independent broker execution and real paper reconciliation remain unproven.
Further MT5 work is temporarily deferred at the user's request, not completed.

Standalone optimization, generation recovery and clearer saved-research labels
are consolidated in [PR #8](https://github.com/ronald1711/sqxlabs/pull/8), based on
main `072f4f7`. PRs #6 and #7 are closed as superseded, not separately merged;
their commits remain in the combined head `871daf7`. Optimization passed 430
local workspace tests and four remote checks at the earlier `f9bffe9`.
Generation's 64-candidate process-kill/restore comparison passed, but a subsequent
workspace run exposed colliding timestamp-based test directories under parallel
execution. Route-specific random directory identities fix that isolation defect.
The replacement full workspace run at `066235d` passed 433 tests with zero failed
or ignored; its workspace build passed. The later presentation-only `871daf7`
passed four relevant web tests, JavaScript syntax/recovery checks and a build.
A browser check confirmed the search name, inspection controls and retained
“Not independently validated” warning. These checks do not establish responsive
or accessibility coverage.

Combined-head GitHub run `45447951438` remains pending as of this audit. Required
Linux/macOS debug/release checks must pass before merge; earlier component checks
do not substitute for those gates. Superseded component runs were cancelled to
avoid duplicate work; neither the combined run nor main's post-merge run was
cancelled. No independent MT5 execution evidence was added.

The successful nonselected walk-forward candidate gap found during this audit has
now been addressed in the report (see the later verification entry). Remaining
priorities include durable per-execution checkpoints and complete account/order
records. Actual broker correspondence remains unproven without independent fixtures.

## MACD reference scope (2026-11-24)

The former MACD check in `talib_crosscheck.rs` compared local MACD components
with local EMA outputs. It is now named as an internal-consistency check rather
than independent reference evidence. `reference.rs` additionally fixes rational
expected values for SMA-seeded MACD(2,3), signal EMA(2) and histogram on seven
nonlinear prices. It checks every truncated history, the first defined line and
signal values, both histogram signs, and invariance after appending extreme
future prices. All 13 arithmetic-reference tests pass locally in debug and
release. The fixture supports that declared convention only: it does not prove
TA-Lib/MT5 seed parity, all periods, missing-value behavior, or all indicators.

## Execution contract migration

The legacy `intrabar_up_first` flag describes an assumed OHLC path, not a
side-independent pessimistic fill policy. A replacement ambiguity model is pending.
Bar prices are treated as bid quotes for the initial spread correction: long entry
pays ask (bid + spread); short liquidation pays ask. Tick bid/ask already contain
spread and must not receive another synthetic spread. Market/stop execution incurs
adverse slippage; resting profit targets are limit orders. Close-derived signals
are executable at the next available bar open. End-of-data liquidation is a
reporting convention, not a signal or evidence of an executable live order.

## Verified increment: causal execution foundation

Implemented and checked with `cargo test --workspace` and `cargo build --workspace`:
- Synthetic spread on bid-based bar entry/short liquidation, adverse market and
  stop slippage, and final equity reconciled with final liquidation costs.
- Close-derived exits queued for the next open.
- ATR stops/targets/sizing use the previous completed bar, independent ATR periods,
  warmup gating, and correct price-to-point units in ATR risk sizing.
- Trailing/break-even updated chronologically along the assumed bar path or ticks;
  excursions stop accumulating after an exit.
- Tick gaps fill stop-market orders at the observed quote; an interval with ticks
  does not fall back to contradictory OHLC extrema when no tick triggers an exit.
- Five new regression tests, including three demonstrated failures before repair.

Not a completed execution model: tick coverage currently permits OHLC fallback in
empty intervals, tick entry/time recording and final-interval bounds still need
revision; explicit path ambiguity, account currency/margin/financing, session-close
causality and input validation remain open. UI run metadata now describes signal
exits as next-open. Existing historical results are not automatically migrated.


## Verified increment: account model

Implemented account currency, causal dated cross-currency conversion with explicit
missing/stale-rate failures, leverage/margin admission, volume constraints with
rounding down, recorded rejected orders, dynamic margin stop-out, signed UTC daily
financing and trade/equity reconciliation. Direct FX pairs use the current price
for quote-to-base conversion. Historical cross rates are explicit user inputs.
Invalid datasets/configuration produce invalid results, not fabricated zero profit.

Evidence: eight account regression fixtures (USDJPY hand calculation, 20-lot
rejection, continuous/gapped stop-out, short/base-currency stop-out, causal/stale FX,
financing reconciliation, minimum size and malformed inputs). Workspace: 116 tests
pass; workspace build passes. Browser verification: changing deposit to 10 produces
seven explicitly recorded insufficient-margin rejections and zero trades. Settings
and downloaded run context include the account model. CLI backtest accepts
--account-config; MCP simulation tools accept a config object.

Remaining account work: broker session/calendar rollover rules and financing-rate
history; conversion bid/ask costs; explicit instrument specification selection for
non-FX/suffixed markets; full CLI option parity; richer account/order ledger and
reconciliation evidence. Default leverage is a configurable simulation assumption,
not a claim about the user's broker. No reliability section is yet fully closed.


## Verified increment: explicit OHLC path policy

Replaced the misleading boolean with adverse_first (default, side-aware),
favorable_first, high_first and low_first. The engine evaluates both monotonic
OHLC paths locally and records candles with different exits or protective-stop
states. The dashboard exposes policy selection and ambiguity details; run context
records the selected policy. Legacy boolean requests fail with migration guidance.
Two added fixtures verify long/short symmetry, explicit alternate outcomes,
deterministic reruns and independence of completed executions from later candles.
These local scenarios are NOT proven worst/best portfolio bounds; evaluating
full-run sensitivity and tick-coverage provenance remains required.


## Verified increment: tick provenance and complete scenario reruns

Tick execution now requires an explicit candle duration, checks non-overlapping
intervals and rejects out-of-interval ticks. The final candle has its full declared
horizon. Missing executable-interval ticks fail unless hybrid fallback is explicitly
enabled. Reports record supplied tick count, missing intervals, duration and mode.
A nonempty interval does not prove complete tick coverage: source completeness is
still an external data-quality requirement, never inferred from the count.
Entries use the first available bid/ask and actual tick timestamp; stop exits and
final liquidation use observed quotes/timestamps without adding synthetic spread.
Historical conversion checks include tick times. CLI exposes --tick-bar-ms and
--allow-tick-fallback. Sparse legacy fixtures explicitly opt into hybrid execution.
Four new tick fixtures exercise late-final-candle execution, gap time/price,
missing-data failures/hybrid evidence and exclusion of out-of-interval quotes.

Complete-run path sensitivity replays all four OHLC policies with identical inputs;
it is exposed in the dashboard and report. A regression verifies differing actual
executions and known account outcomes; the web test verifies all four scenarios
and execution provenance reach the report. It is sensitivity analysis, not a proof
of global bounds. Session-close scheduling, calendar-aware funding and replayable
research storage remain open.

## Verified increment: immutable research archive and replay

Successful dashboard backtests now archive the actual dataset snapshot (including
instrument/timeframe), typed strategy, fully resolved execution/account config,
optional scenario selection and report. SHA-256 content addresses identify each
object; atomic no-overwrite publication, integrity checks and deduplication protect
saved records. Complete JSON backup bundles can be restored only after all hashes
validate. Float-roundtrip parsing preserves archived finite f64 inputs exactly.

Replay uses archived data, not the live store, and compares the complete report.
The server caches the executable identity at startup; replay explicitly refuses a
mismatched engine. The executable itself is not bundled or executed from a backup;
retaining historical matching builds remains an operational requirement.

Evidence: four archive tests cover deterministic identity/reopening, corruption,
path traversal, backup restoration and eight concurrent writers. A web test clears
the current data before replay and checks restore and engine-mismatch refusal.
Browser verification created an archived run, restarted the server, reloaded the
page and confirmed exact replay without browser errors. Test startup now waits for
actual server readiness, not a fixed sleep. Temporary test paths use atomic unique
suffixes to avoid coarse-clock collisions discovered by the concurrency tests.

Scope still open: archives for discovery/optimization/robustness/CLI/MCP runs,
failed-job records, portable build retention, pagination/large-data storage,
version comparison workflows, data-quality/calendar checks and resumable jobs.
This increment does not close the full reproducibility or operations requirements.


## Verified increment: walk-forward warmup, capital and evidence

Added backtest_window: indicators/confirmation warm on the available historical
prefix, without any trades/equity samples before the evaluation boundary. A signal
on the last historical close may execute at the first evaluation open. Each fold
receives only data available through its own end. Candidate ranking uses training
results only; OOS runs use the carried account equity. Combined drawdown now uses
the bar-by-bar OOS equity paths rather than only closed-trade balances.

Walk-forward reports retain fold dates, selected strategy, attempted-candidate
count, starting/final capital, trade counts, explicit thresholds and incomplete
fold/error reasons. OOS outcomes are retained even when trade-count evidence fails;
no ex-post filtering of losing windows. Dashboard thresholds are configurable;
matrix rows and CLI/MCP results expose evidence limitations and trial counts.
Default training/OOS minimums are research settings, not statistical guarantees.

Evidence: an SMA warmup fixture trades immediately in the warm evaluation window
while its cold counterpart cannot; no pre-window trades/samples exist. A seeded
three-fold fixture verifies capital continuity, retained insufficient-evidence
outcomes, trial accounting, deterministic reruns and no change to earlier folds
when later test data is perturbed. Existing tests and workspace build pass.

Remaining validation requirements include an untouched final holdout, experiment
registry across repeated searches, uncertainty estimates, independent references,
calendar-aware fold boundaries and portable archived optimization jobs. Current
walk-forward is segmented: each OOS fold liquidates at its end and remains flat
between OOS segments; it is not represented as continuous rolling deployment.

## Verified increment: data quality and unambiguous dataset selection

Added an OHLCV audit with structural/ordering errors, incomplete-candle detection,
calendar-aware missing/open versus scheduled-closed intervals, unclassified gaps
when no calendar exists, and irregular interval counts. Calendars are explicit UTC
weekly sessions plus date overrides for holidays/DST; no broker schedule is guessed.
Monthly closing boundaries use real calendar months. CSV rejects malformed rows,
unknown volume, non-finite values, duplicate timestamps and timestamp overflow.
Directory imports stage and validate all files before publishing any and use sorted
file order. QuestDB queries limit direct candles to completed intervals, reject
malformed server rows and remove incomplete aggregate buckets on return.

Dashboard imports accept the declared timeframe and optional UTC calendar. Data
quality is visible; incomplete/invalid data blocks simulation. Unresolved gaps
require an explicit exploratory override, recorded alongside the quality report.
Archived datasets retain their calendar and replay retains the original as-of
cutoff. A wrong H1 label on minute-data test fixtures was caught and corrected.

Dataset selection now uses symbol+timeframe identities. Unknown symbols and
ambiguous bare symbols no longer silently select the first dataset. Deletion also
targets the exact series. The header displays the actual symbol/timeframe, while
staleness checks include calendar and timeframe as well as prices and costs.

Evidence: four quality fixtures cover closed-boundary/leap-month behavior, weekend
versus missing weekday/holiday classification, invalid calendars and CSV errors.
Three web fixtures cover dataset identity, atomic rejection of incomplete imports,
and explicit gap acknowledgment. A local mock QuestDB HTTP test verifies the
closed-candle SQL cutoff and exclusion of an incomplete returned candle. Workspace
tests and build pass. This is not a live QuestDB integration certification.

Remaining: complete provenance/source snapshot metadata,
exchange-calendar data distribution and dynamic broker session
schedules, full CLI/MCP quality-policy parity, and large-dataset audit scalability.

## Verified increment: aggregate boundaries and local HTTP containment

QuestDB aggregation excludes both leading and trailing partially sampled buckets
before applying the requested result limit. A mock HTTP regression checks this
ordering. This does not prove completeness of the underlying bars inside a bucket.

The embedded server now permits loopback binding only. It checks Host and Origin,
rejects cross-site requests, duplicate headers and unsupported request framing,
requires JSON for POST requests, and bounds header/body size and concurrent
connections. Socket timeouts limit individual blocking operations. HTTP fixtures
exercise accepted same-origin requests and rejected hostile/malformed requests.

Verification: 139 workspace tests pass and cargo build --workspace succeeds.
The build reports one deprecated AtomicUsize::fetch_update warning. Remote user
authentication, a whole-request deadline, durable jobs and bounded AI authorization
remain open; loopback containment is not an authenticated multi-user service.


## Verified increment: causal scheduled exits

Daily/Friday exits no longer inspect the timestamp of the next candle or infer a
session close from the end of a dataset. Strategies enabling either option must
provide session_close_utc_minute (0..1439); legacy strategies without a declared
cutoff fail explicitly. Deadlines are derived from entry time and the configured
UTC daily/Friday schedule. Bars fill at the first observed open at/after the
cutoff, ticks at the first observed quote, including deadlines inside a candle.
Missing quotes delay execution rather than backdating a fill. Entries after the
applicable cutoff are suppressed. Builder and session profiles expose the cutoff;
archived typed strategies retain it.

Evidence: three regression fixtures verify explicit configuration, future-candle
independence, missing-cutoff gap execution, Friday-only scheduling/entry suppression,
and an intrabar tick cutoff at its actual quote/time. All 142 workspace tests and
the workspace build pass; inline dashboard JavaScript passes syntax validation.
The new controls have not yet received browser interaction verification.

This is an explicit UTC schedule, not a broker calendar: holiday/early-close/DST
schedules and calendar-aware financing remain required. Bar execution deliberately
does not invent intrabar timestamps or assume the last quote before a closure was
known to be the final quote. Users requiring finer cutoff execution need suitable
bar granularity or supplied ticks. Exporter parity for this schedule remains open.


## Verified increment: explicit export capability boundary

Review found that platform exporters are reference skeletons, not equivalent
executables: MQL4 omits protective/signal-exit routing and per-bar guards; MQL5
leaves order actions as comments and emits MQL4-style indicator access; Pine
changes fixed-lot sizing and has unverified indicator syntax; Ninja forces session
exit/warmup defaults; EasyLanguage/Ninja silently substituted unmapped indicators.

Added a shared structured export report. Scheduled exits, trading-hour/daily-entry
limits, confirmation, additional exits, fuzzy/duplicate signal blocks, non-fixed
sizing, protective orders and unmapped indicators block platform source generation.
Unknown mappings no longer fall back to SMA. Raw renderers are no longer publicly
re-exported. Remaining platform source is explicitly a reference draft with
limitations embedded in downloads. CLI reports blockers without writing output;
web returns status/blockers/limitations and deployment_ready=false. Candidate
exports open review rather than immediately downloading. Unknown engine names fail
instead of selecting MQL5. MQL4 downloads now use .mq4. Human-readable rules retain
money-management parameters, trading options, exits and confirmation.

Evidence: export regressions cover all five platforms' missing schedules,
protective orders and unmapped indicators, retained draft diagnostics and complete
schedule documentation. The web contract checks draft/blocked/unknown-engine
responses. Workspace tests (144) and build pass; JavaScript syntax passes.
Browser interaction of the revised export controls remains unverified.

This prevents silent unsupported exports but does NOT fulfil platform parity.
Implementation, actual target compilation and independent execution comparisons
remain required for each target; no platform is marked deployment-ready.


## Verified increment: frozen development/final-test experiments

Added a persistent experiment protocol and dashboard workflow. Creation snapshots
bars/instrument/provenance, execution/account config, original executable identity,
split and minimum development/final trade thresholds. Development evaluation only
receives the prefix before the split. Random search records seed, candidate index
and generator settings; manual candidates are recorded too. Every attempt is
journaled before execution, including repeated searches and insufficient-evidence
results. Complete execution/trade/equity reports are persisted.

The append-only ledger uses hash-linked immutable events, atomic publication and
an OS file lock for cross-process serialization. A final candidate is committed
before reading held-out candles. Later selection changes/development trials are
refused; the same candidate can resume after an interrupted commitment or retrieve
its saved result. Final tests warm on historical prefixes, create no earlier
positions, start flat with the frozen initial deposit, and retain losing or
insufficient-trade results. They do not carry a development account/position into
the final period. Plans and ledgers survive restart and require the original build.

Evidence: development reports remain identical when only held-out prices change;
all final trades/equity occur inside the final period; competing commitments select
exactly one candidate; restart/resume cannot change selection; invalid splits,
corrupted events and insufficient development evidence fail closed. Web tests clear
live data, change supplied costs and repeat seeded searches: snapshots/costs remain
fixed and attempts accumulate. All 150 workspace tests and workspace build pass.
Browser verification created an experiment, tested the example, committed its
candidate, retained a losing final result, reloaded and verified development
controls remain disabled, with no console errors. The revised export blocker was
also verified in the browser. Scheduled-close hydration now restores the saved UTC
minute when loading a strategy into the visual editor.

Scope remains explicit: this reservation isolates only operations inside an
experiment. Ordinary backtests, CLI/MCP and other experiments can still access the
same data. Prior exposure is disclosed by the user, not independently proven.
Cross-experiment overlap/exposure detection, integration of all search modes,
registry-wide multiple-testing accounting, experiment backup/restore, scalable
pagination, uncertainty estimates, resumable search batches and stronger release
validation remain required. Hash chains detect modified/reordered records, not a
malicious owner deleting a journal suffix. No statistical or deployment certificate
is implied by meeting a trade-count threshold.

## Verified increment: experiment backup and non-rewinding restore

Experiment backups now include the frozen plan, full dataset/provenance, complete
candidate history, commitment/result and head hash. A manifest checksum binds these
components together. Restore validates all identities, hashes, sequence and allowed
protocol state transitions before publication. Existing history must agree with the
incoming prefix: older backups cannot rewind local evidence or unlock an already
committed final test, longer matching histories append, divergent histories fail.

Creation, execution and restoration share the experiment's OS lock. A new restore
publishes the plan discovery marker after its journal; interrupted staged restores
can resume from their matching prefix. Backups do not contain an executable and do
not authorize execution of a different build. Checksums protect integrity, not
provenance against a malicious author who can recompute checksums.

Dashboard controls download/restore experiment backups; HTTP handlers expose the
same contract. Tests exercise JSON interchange, retaining a completed commitment
when restoring an older backup, corrupted/truncated backups, divergent histories,
concurrent restores, hash-consistent but invalid state transitions, and the web
backup/restore round trip. All 154 workspace tests and workspace build pass;
dashboard JavaScript passes syntax validation. Browser download/upload interaction
for these new controls remains to be verified. This closes the basic experiment
backup gap, not broader job resumability, off-machine backup operation or the full
six-section programme.

## Verified increment: registry overlap and browser-safe backup transport

Experiment status now reports known overlap with other experiments' development
attempts/final commitments and successful archived dashboard backtests. Detection
uses observation-time ranges on the same named market or FX pair, not only equal
snapshot hashes: changed imports or costs do not hide known reuse. Same-market
experiment attempts and archived backtest records are reported separately; neither
is falsely presented as a complete count of independent hypotheses.

Commitments across experiments are serialized with a registry lock, also shared by
plan creation/restoration. The exposure audit is retained in the commitment before
final evaluation. A later/current audit remains separate. Restoring a backup retains
known reuse from its original commitment even if the other experiment is absent
from the destination. UI labels known/disclosed reuse as exploratory. Absence of a
recorded overlap is explicitly not proof of independence. The current audit does not
cover unarchived searches/data viewing, CLI/MCP, external work, partial candle-time
coverage or simultaneous unarchived executions. Cross-interface provenance and
complete experiment accounting remain open.

Five added fixtures cover changed snapshots/settings, competing final commitments,
ordinary archived backtests, unrelated markets/disjoint periods, and preservation
of exposure evidence after restoring into another store. All 159 workspace tests
and the workspace build pass; dashboard JavaScript syntax validation passes.

Browser backup testing exposed an additional real defect: parsing and reserializing
backup JSON in JavaScript changed numeric representations, invalidating Rust's
content hashes. Both research and experiment backups now download the original
response text and upload the original file text. A Node regression exercises the
production helpers and upload handlers with integer-looking floats, integers beyond
JS's safe range, high-precision values and Unicode:
`node crates/sqx-web/tests/backup_wire.cjs` (passes).

Browser verification downloaded and restored both backup types successfully,
confirmed the recorded overlap appears, and verified final-test locks remain in
place with no console errors. Earlier browser-upload verification gaps are closed.
Executable retention, global job resumability, independent reference evidence and
other requirements in the six-section programme remain open.

## Verified increment: catalog causality and independently checked warmups

Added a catalog-wide prefix/future-perturbation regression. It exercises default
parameters and period overrides 1, 2, 7 and 31 where available, 19 prefix lengths
including empty/single-candle input, and four future-replacement boundaries. Every
historical finite value must remain bit-identical and every undefined value must
remain undefined. All 179 unique catalog indicators pass for these fixtures. This
is not an exhaustive proof over every input, parameter combination or registry-only
indicator, and does not establish formula correctness by itself.

The test first reproduced VortexPlus/Minus panics on empty input. Independent
arithmetic checked against Botes/Siepman's original Excel construction also found
that the first output used an invented zero movement, shortening warmup by a bar.
Vortex now requires n actual movements (n+1 candles), returns empty output for empty
input, and passes hand-calculated fixtures with gaps for periods 1, 2 and 3.
Source: https://technical.traders.com/free/V28C01005BOTE.pdf, Excel sidebar; inspected
via the publisher-credited reprint at
https://static1.squarespace.com/static/5c2fa05dcc8fed682704b044/t/5dd482e44e4f4212e7a9046b/1574208237438/Vortex%2BIndicator%2BArticle.pdf

Cross-checks now require nonempty reference output, complete tail coverage and
matching undefined warmups, with explicit multi-output/first-TR exceptions. This
exposed the same premature first value in MFI. MFI now requires n classified flows;
a separate hand-calculated price/volume fixture and the ta-lib Rust reference check
its values and warmup. The ta 0.5 streaming implementation emits partial-window
values, so comparison begins at the first full classified-flow window, with an
explicit assertion that earlier Sqx values are NaN.
Formula source: https://www.tradingview.com/support/solutions/43000502348-money-flow-mfi/

Removed a duplicate LinRegSlope catalog definition that supplied contradictory
period defaults and incorrectly classified slope as a bounded oscillator. A catalog
uniqueness regression prevents recurrence. Random-generation distributions change
with this corrected catalog; historical builds remain necessary for old-run replay.
Backtests sensitive to the first Vortex/MFI output must be rerun with the new engine.

Evidence: both new defect fixtures failed before repair. All 163 workspace tests,
workspace build and the JavaScript backup-wire regression pass. Reference-test
headers now accurately distinguish Rust-package comparisons from executing an
independent C reference. Full formula/reference coverage, calibrated statistical
validation and automated release enforcement remain open requirements.

## Verified increment: repeatable checks and CI configuration

Added ci/check.sh as one fail-fast entry point for locked workspace tests/builds,
standalone and inline dashboard JavaScript syntax, embedded JSON validation, and
the exact backup-wire regression. It supports debug and release profiles, uses
Rust 1.100.0 through rustup explicitly (rather than an unrelated PATH compiler),
and works with macOS's older Bash. Node and curl versions are printed in the log.

The GitHub Reliability workflow configures Linux/macOS × debug/release checks,
read-only repository permissions, disabled credential persistence, Node 22.23.4,
and commit-pinned official checkout/setup-node actions. Action commits were checked
against their upstream tags. The pinned Rust toolchain was installed and exercised
locally. Runner OS images/system libraries remain platform-dependent; this is not
an assertion of identical executables across machines.

Evidence: bash ci/check.sh debug and bash ci/check.sh release both completed on
local macOS with 163 passing Rust tests each, successful workspace builds, passing
JavaScript syntax/JSON checks and passing backup-wire regression. Workflow YAML
parses and the shell script passes Bash syntax checking. The build still reports
the existing AtomicUsize::fetch_update deprecation warning.

No remote workflow run or branch protection/ruleset has been verified. The workflow
is a local change until published; required checks must be configured after actual
remote success before this becomes an enforced merge gate. Therefore automated
release enforcement remains incomplete, along with the other open requirements.


## Verified increment: durable experiment search jobs

Experiment random searches now freeze their candidate list, generator settings and
seed in a content-addressed search specification before executing candidates. Each
candidate is claimed and its result checkpointed in the experiment journal. A
restart resumes an interrupted claim using its original trial number; completed
candidates are not counted twice. A request key deduplicates job creation and
rejects reuse with different settings. Concurrent workers serialize checkpoints.
Final-test commitment freezes unfinished searches. Resuming requires the matching
engine identity; changing the executable does not silently change an experiment.

The dashboard lists saved jobs and provides resume and pause-after-current-candidate
controls. It executes one candidate per request. Closing the page stops scheduling
new work; a currently executing server request can still finish. This is a resumable
client-driven workflow, not an unattended background worker service. Backup schema
2 includes candidate specifications and validates their hashes and links to trial
records; schema 1 backups without jobs remain restorable.

Evidence: debug and release workspace gates each pass 168 Rust tests, builds,
JavaScript syntax checks and the backup-wire regression. An additional web API
regression passes for duplicate job creation, partial execution, backup restore
with no live dataset, and completion without duplicate trials. Storage regressions
cover process-like reopening, an interrupted claim, concurrent workers, corrupted
specifications, old backups and commitment freezing. The new pause/resume interface
has syntax validation but has not yet received a browser interaction test.

Remaining: persistent jobs for optimization/robustness/other workflows, worker
scheduling and cancellation, scalable journal/status handling, browser recovery
validation, and the broader professional acceptance requirements above. These
changes do not establish complete scientific validation or production readiness.


### Search start recovery after a lost response

The dashboard now stores an unacknowledged start request in tab session storage
before sending it. Retrying after a network failure or page reload reuses its
request key, seed and candidate count, even if controls were changed meanwhile.
The pending request is cleared only after an acknowledged job creation. A local
start guard also prevents simultaneous creation requests from the same page.
Session storage is scoped to the browser tab; closing that tab is not a guarantee
of preserving its pending request. Already saved server jobs remain listed and
resumable independently. Users should inspect saved jobs after losing a tab.

A production-handler regression simulates a lost response and a fresh page context,
verifies identical retries and retained settings, and checks that the next genuinely
new request gets a fresh identity. It is included in ci/check.sh. This complements
server-side deduplication; it is not a rendered browser interaction test.

### Browser acceptance: pause, reload and resume

Verified on the locally built dashboard using only synthetic preview data in
work/ux-preview (not the user's main data directory). Created a fresh frozen
experiment, started 30 candidates with seed 42, and paused after 2 saved results.
Reloaded the page and reopened Builder: the same job showed 2/30 and a Resume
saved search control. Resuming completed that job with 30/30 saved and exactly
30 attempted candidates. Browser error logs were empty. A screenshot is retained
as outputs/sqxlabs-resumed-search.jpg in the chat deliverables directory.

This verifies the actual start/pause/reload/resume user flow. The network-loss
case remains covered by the production-handler regression, rather than a browser
network interruption. Other workflows and the full six-part acceptance scope
remain open.

## Verified increment: bounded assistant execution

Assistant execution now enforces at most 16 configured provider rounds, 8 tool
calls in one provider response, 64 tool calls and 200 backtests over an Assistant
instance's lifetime (including repeated run calls and details requests). Inputs
are bounded to 100000 bars and a 32768-byte question. These are local hard limits;
the provider cannot raise them through tool arguments. The existing web/CLI entry
points each create an instance per question, so this is not a global rate limit.

Malformed tool JSON no longer silently becomes an empty argument object. Missing,
negative, fractional, oversized or unknown generate/details arguments are rejected
instead of selecting defaults or clamping. Invalid calls consume the tool-call
budget but do not run backtests or overwrite the previous candidate list. Failed
budget reservations retain the existing candidates. Four focused regressions pass
for argument validation, aggregate computational accounting, invalid/read-only call
accounting, and rejection of oversized inputs before network or computation.

Remaining security/validity work includes bounded provider response buffering,
whole-session wall-clock/concurrency budgets, avoiding credentials in subprocess
arguments, explicitly carrying the user's frozen simulation settings into assistant
research, archiving assistant trials and enforcing statistical research boundaries.
These limits are not a complete security or scientific-validity certificate.

## Assistant simulation-context consistency

The dashboard now sends its current simulation settings with an assistant question.
The web endpoint freezes those settings into the Assistant instance instead of
backtesting generated candidates with implicit zero-cost defaults. Both generation
and details use that same configuration. CLI callers retain explicit documented
constructor defaults unless they supply a configuration through the new builder.
The assistant endpoint applies the data-quality/gap gate before resolving the
provider. Core simulation input validation precedes provider requests and direct
tool computations; invalid executions are excluded from ranked candidate rows.

Tool responses include the actual configuration, dataset bar count, seed and
attempt counter. The provider receives this research context and the interface
appends the actual cost/account summary and limitations independently of the model's
answer. Full-dataset in-sample assistant searches are explicitly exploratory, with
no reserved final test, independent validation, or archived trial claim.

Regressions compare an assistant details result with an explicit-cost engine run
and show that it differs from a default-cost run; reject invalid costs before
ranking; and reject unresolved data gaps before provider access. No live provider
was contacted to test the UI. Frozen-experiment integration and durable assistant
trial accounting remain open, as do the transport-security requirements above.

## Assistant transport: bounded in-process HTTP

Replaced the assistant's curl subprocess with ureq 2.12.1 using its Rustls TLS
feature. The API key and prompt are no longer passed through process arguments;
LlmConfig debug output redacts the key and endpoint. Requests are limited to 4 MiB,
response reads to 1 MiB plus one detection byte, connection establishment to 10
seconds and each complete HTTP request to 180 seconds. The installed library source
was checked for its documented whole-request/body timeout semantics. Compression
features are disabled. TLS dependencies are recorded in Cargo.lock.

No redirects are followed, including same-host redirects. HTTP(S) endpoint syntax
is validated, userinfo and fragments are rejected, and key control characters are
rejected. HTTP error bodies remain bounded and redacted; transport errors avoid
printing URLs or credentials. HTTP remains available for explicitly configured
local/private providers; this change does not certify plaintext remote connections.

Local-server regressions pass for HTTP success/error, secret redaction, response
limits on both success and error bodies, request limits, unsafe syntax, debug
redaction, and a 307 redirect whose destination listener receives no connection.
These use dummy credentials only. No live provider or external TLS interoperability
was tested. Whole-assistant-session deadlines/concurrency, credential storage at
rest and durable assistant research accounting remain open requirements.

Validation: the complete debug and release workspace gates pass with the new
transport. The first release attempt exposed a fixture race: the one-shot test
server closed with unread request bytes, intermittently resetting the connection.
The fixture now reads headers and the complete Content-Length body before replying;
focused debug tests and the complete release gate pass after that repair.

## Assistant session scheduling limits

The web server now admits one assistant request per server State at a time, across
browser tabs. A nonblocking mutex rejects overlapping requests without holding a
waiting HTTP worker; its guard releases on all normal/error return paths. A poisoned
gate requires server restart. The interface disables submission while its request
is pending. This is per-process admission control, not cross-process rate limiting.

Each Assistant instance starts a five-minute monotonic time budget at its first
run/dispatch. Later run calls do not reset it. Every provider request receives the
remaining session time (capped at the existing 180-second per-request timeout), and
local candidate evaluation checks expiration before each backtest. An in-progress
backtest is cooperative/non-preemptive and may finish after the deadline; therefore
this is not a hard wall-clock bound on arbitrary computation. The generation budget
continues to count reserved candidates, including those skipped after expiration.

Regressions verify expiration prevents subsequent run/dispatch/network work, and
another thread cannot enter an already-held assistant gate. Invalid JSON and early
validation returns release the gate. Full background cancellation, durable assistant
jobs/trials and cross-process limits remain open.

## Robustness validity: failures are not zero-profit observations

What-if rows now retain engine errors and expose undefined metrics for invalid
scenarios. Monte Carlo retests record requested, completed, skipped and invalid run
counts. Invalid simulations no longer enter the profit/drawdown distributions as
zero-profit paths; any invalid run makes aggregate quantiles/losing percentage
undefined rather than conditioning the summary silently on successful runs. Short
sample windows are explicitly counted as skipped. Warmup continuity for this retest
algorithm is still a separate open issue.

Trade resampling rejects empty trades, zero runs, nonfinite PnL and invalid skip
probabilities. Empty-input reports now say zero completed runs and carry explicit
errors. Dashboard, CLI and MCP surface errors/coverage; invalid base backtests stop
the robustness workflow. The dashboard shows unavailable Monte Carlo results and
invalid scenarios rather than only a table of missing numbers. Error text is escaped.

Four core regressions and a web regression cover invalid costs, failed retests,
short skipped windows, absent resampling evidence, and invalid baseline propagation.
The complete debug workspace gate passes; the final scenario-error display edit
also passes inline JavaScript syntax validation. No browser interaction test was
performed for that display change. These checks do not certify statistical power,
multiple-testing correction, broker execution parity or complete robustness.

## Monte Carlo retest: causal warmup across sampled starts

Random-start retests now use backtest_window over the original historical prefix
instead of slicing away all earlier candles. The chosen offset is the first
tradable bar, not an indicator reset. Each scenario starts flat with the configured
deposit; a signal known at the preceding close may fill at the start open. No earlier
positions/equity are carried into the result. SL/PT jitter remains seeded and causal.
Reports retain the seed, every sampled offset (including skipped trials), and the
explicit warmup policy; CLI/MCP display the seed/policy with coverage counts.

A 300-bar ascending fixture with SMA(10) checks the complete percentile calculation
against hand-computed entries: max(10, sampled offset), exit at final close, zero
costs, one 0.1-lot EURUSD position. This distinguishes a valid historical warmup from
both cold-start delay and pre-window trading. The test also verifies zero drawdown
and bit-identical repeated median results with the same seed. All five robustness
regressions pass. The earlier note that this retest algorithm still resets indicator
warmups is superseded by this repair. This does not establish statistical power or
continuous live-position carry across scenarios; starts are intentionally flat.

## Shared dashboard research data-quality gate

Backtests, robustness analysis and assistant requests now use one shared data-quality
check. The robustness endpoint previously bypassed calendar/completion checks and
could stress-test data rejected by the ordinary backtest. It now rejects unresolved
gaps unless explicitly acknowledged for exploration, and always rejects malformed
or incomplete candles. The response retains its quality report and acknowledgment;
the dashboard displays the exploratory interpretation and gap disclosure.

A regression verifies blocked gaps, acknowledged exploratory execution with its
quality evidence, and an incomplete last candle still being blocked despite the
acknowledgment. Existing backtest/assistant quality regressions also pass. The full
debug workspace test/build gate and JavaScript checks pass. This unifies these three
dashboard routes only; other research entry points still require a full audit.

## Discovery/pipeline data gates and disclosure retention

Dashboard generation, optimization (including walk-forward and its matrix), and
pipeline execution now pass the shared data-quality gate and core simulation-input
validation before creating candidates or mutating the databank. Their responses
include quality evidence and explicit gap acknowledgment. Selected candidate rows
retain that context when written to the databank; generation/optimization/pipeline
results show the exploratory scope in the interface. This is disclosure retention,
not complete immutable lineage or a trial ledger for these legacy routes.

A route regression covers all three handlers and optimization kinds, verifies gap
rejection, checks that acknowledging gaps still cannot bypass invalid account inputs,
and verifies the in-memory databank stays unchanged on rejection. The complete debug
workspace gate passes. Success-path disclosure is implemented but not yet exercised
through a rendered browser workflow. Portfolio/analyzer, CLI/MCP and other callers
still need the corresponding end-to-end quality/provenance audit.

## Portfolio/analyzer dashboard input consistency

Both portfolio endpoints and the analyzer now apply the shared data-quality and
simulation-input gates. Generated portfolios previously used a default zero-cost
configuration; they now resolve the supplied settings, and both legacy portfolio
buttons send current dashboard settings. Results include the actual configuration,
data-quality evidence, gap disclosure and analysis scope. Selected-portfolio and
analyzer displays surface those limitations.

Invalid component simulations stop portfolio construction with named component
errors. The analyzer refuses an invalid backtest instead of producing a normal
markdown report. A route regression covers data-gap rejection and invalid scheduled
exit configuration in portfolio/analyzer runs. The selected-portfolio regression
also checks returned deposit, spread and commission. The complete debug workspace
gate passes; final disclosure wording/display changes pass JavaScript syntax checks.
These remain weighted independent strategy simulations: shared-account margin,
portfolio order interactions and execution parity are still unimplemented requirements.

## Portfolio alignment and allocation invariants

Portfolio combination now rejects mismatched name/result/weight lengths, nonfinite
or negative weights, allocations not summing to one, differing initial deposits,
invalid component results, nonfinite equity/trade values and mismatched/unsorted
equity timestamps. Previously it silently truncated arrays and combined by row
position, potentially pairing unrelated dates or panicking on missing names.
Errors produce undefined metrics and no combined curve, and dashboard/CLI/MCP
callers surface them. Successful weighted aggregation remains an independent-run
capital-allocation model, not shared-account execution.

Subset search now rejects invalid size ranges and honors the requested size even
when every score is negative infinity (for example flat strategies). The previous
fallback silently chose up to three components outside the requested size. Equal
scores retain deterministic first-combination selection; that is not evidence that
a flat portfolio is statistically useful.

Four regressions cover invalid dimensions/weights, mismatched dates/capital,
hand-computed weighted equity and subset cardinality under undefined scores. The
complete debug workspace gate passes. BacktestResult currently lacks account-currency
provenance, so cross-currency input validation is still an explicit unresolved
requirement for library callers; same-currency dashboard runs do not prove it.

## Account-currency provenance in execution and portfolios

ExecutionEvidence now carries an optional explicit account_currency. Normal,
short/empty and invalid engine results retain the configured denomination; manually
constructed/default evidence has None, not an assumed USD. Portfolio validation
requires a valid known currency shared by all components, and rejects differing or
missing denominations before aggregation/subset selection. Successful portfolio
results and dashboard responses retain the denomination. Weighted aggregation also
propagates it into the internal combined execution evidence.

Regressions cover same-currency success, unknown/mixed-currency rejection, and actual
EUR-configured engine runs (normal and short input). The full debug workspace gate
passes; the additional EUR regression passes and the final response-field addition
is compile-checked. Existing archived results are not rewritten or relabeled; exact
replay continues to require their original engine identity. This closes the missing
currency-provenance validation noted above, but does not implement cross-currency
portfolio conversion, shared-account execution or authenticated provenance.

## Portfolio correlation: undefined evidence stays undefined

Correlation now includes the initial-capital-to-first-equity-sample return, so a
window's initial spread/commission exposure is not silently omitted. Non-positive
prior equity produces undefined returns instead of fabricated zeros. Pearson rejects
nonfinite or mismatched input series, and the diagonal is computed using the same
variance requirements instead of being hardcoded to one. Constant-return strategies
therefore have undefined self-correlation. Dashboard correlation panels explain that
undefined values are not diversification evidence.

Three regressions cover flat-series diagonals, hand-calculated opposite two-period
return paths (-1 correlation, including the first sample), and non-positive equity.
The complete debug workspace gate passes; final explanatory UI wording passes
JavaScript syntax checks. This estimates sample correlation only, without confidence
intervals, regime stability or a statistical diversification certificate.

## Generator range validation

GeneratorConfig now offers explicit validation and a checked try_random_strategy
entry point. Validation rejects reversed/nonfinite stop/target and parameter ranges,
invalid condition counts, nonpositive size, disabled directions, duplicate parameter
rules and unknown indicator/parameter names. Invalid checked calls consume no RNG
state. Dashboard generation/optimization and library walk-forward validate settings
before sampling; generation rejects zero/out-of-limit candidate counts and no longer
clamps invalid condition counts into different values.

Two core regressions cover invalid inputs without RNG consumption and fixed-range
seed reproducibility. A dashboard regression verifies reversed/zero inputs are
rejected without databank mutation. The legacy infallible random_strategy and
mutation APIs still require callers to supply validated configuration; migrating
all CLI/MCP/evolution entry points and semantic parameter bounds is outstanding.
This increment does not claim every generator input path is now protected.

## Pipeline false-validation defect and evolution preflight

Found a correctness defect in Pipeline Optimize: for every existing candidate it
ran a fresh random-strategy walk-forward search, then used that unrelated search's
profit to retain/discard the original candidate. Removed that false-validation path.
Pipeline preflight now explicitly refuses candidate-linked Optimize as unimplemented
before doing any build/retest work. Consequently the current default demo pipeline
and dashboard pipeline command report an error until real candidate-linked
optimization is implemented. This is a deliberate incomplete-functionality state,
not completion of the required discovery/optimization pipeline.

Pipeline reports now carry errors; invalid task sizes, percentages and jump targets
are rejected, and exhausting the step limit is an incomplete/error outcome rather
than silent success. Dashboard/CLI refuse to persist/report a successful failed
pipeline. Genetic evolution now returns Result and validates generator settings,
probabilities, population/generation limits, elite counts and backtest inputs before
allocation or RNG operations; its CLI caller propagates errors.

Two pipeline regressions prevent unrelated-search certification and silent loop/
invalid-step success. An evolution regression covers invalid probabilities and empty
populations. Candidate-linked pipeline optimization must be implemented and verified
before the default pipeline can be shipped. Legacy direct random/mutation APIs and
semantic parameter limits still require the broader follow-up audit.

## Candidate-linked pipeline optimization replacement

The temporary Optimize rejection above is superseded by a real candidate-linked
implementation. Added walk_forward_candidates, which selects only from an explicit
frozen family instead of generating unrelated rule trees. The existing random
walk-forward API uses the same execution/IS-only selection core without changing
its candidate source. Warmup, chronological OOS capital and evidence thresholds are
shared between both paths.

Pipeline Optimize now builds a seeded SL/PT-distance family for each original
candidate: original first, then ±20% distances/multipliers; ATR periods, all signals,
sizing, schedules and confirmation remain unchanged. Strategies without SL/PT have
only their original variant. The family is fixed before evaluating windows. Each
window selects on IS only and then evaluates that selected member on OOS. Survivors
require sufficient per-window trade evidence and positive aggregate OOS profit;
the retained strategy is the last window's IS selection, not the best retrospective
OOS variant. The default pipeline is executable again.

Reports preserve original candidates, complete variant families and window reports,
including failed/insufficient evaluations. Dashboard responses return these records
alongside logs. This is exploratory exit-distance optimization: discovery already
used the full dataset, so it does not establish an untouched holdout. General
indicator/threshold parameter optimization, durable pipeline job/attempt storage,
and independent final validation remain required; this limited parameter family is
not claimed as completion of those broader requirements.

Regressions verify candidate-family membership and rule preservation, bounded seeded
exit perturbations, no-parameter family behavior, supplied-family IS selection and
insensitivity of the first selection to later OOS changes. The end-to-end library
pipeline regression exercises Build -> Optimize with recorded originals/families.
The complete debug workspace gate passes. Rendered dashboard pipeline execution and
release verification of this replacement remain outstanding.

## Monte Carlo selection: zero is not positive

Pipeline Retest previously calculated its positive percentage as 100 minus losing
percentage, counting zero-PnL paths (including skipped-all resamples) as profitable.
MonteCarloReport now records strictly-positive, flat and negative percentages
separately, and the pipeline uses an explicit validated positive-threshold predicate.
The dashboard and text renderer expose the distinction. Invalid base results are
rejected before resampling. Arithmetic overflow in a resampled net/drawdown invalidates
the distribution rather than yielding usable percentiles or a passing filter.

Three regressions cover zero/skipped-all paths, mutually exhaustive sign categories
and seed reproducibility, plus invalid probabilities and finite-input overflow.
The full debug workspace gate passes; the pipeline tests also pass after moving
base-error rejection before the resampling call. This fixes classification and does
not estimate statistical significance or correct the original strategy search's
selection bias.

## Explicit candidate parameter families

Added parameter_family for seeded candidate-linked indicator parameter searches,
combined with the existing bounded SL/PT variants. The original remains the first
control even when outside the requested ranges. Other variants apply explicit
indicator/parameter ranges to all matching occurrences, including confirmation
signals, while preserving rule structure. A requested parameter absent from the
candidate is an explicit error, not a silently ignored setting. Integer endpoints
use the generator's existing rounded-sampling convention.

Pipeline optimization uses this family builder, and dashboard pipelines now receive
the actual generator configuration rather than a hardcoded default. Parameter parse
errors, wrong types and malformed range records are rejected by the shared optimizer/
pipeline parser instead of silently discarded. With a global range applying to only
some generated candidates, nonmatching candidates produce explicit pipeline errors;
that behavior still needs a more user-friendly scoped range-selection interface.

Regressions verify reproducibility, unchanged rule structure, bounds, preservation
of the original control and absent-parameter rejection, plus strict request parsing.
The complete debug workspace gate passes. Indicator-specific domain constraints,
frontend textual-range parsing, complete parameter coverage, durable trial lineage
and independent final validation remain open; this is not a full optimizer completion
claim.

## Frontend generator ranges: no silently discarded input

The dashboard now parses every nonblank parameter-range line strictly and reports
its line number for malformed syntax, reversed/nonfinite bounds or duplicates.
Signed decimal/scientific notation is accepted. Numeric generator fields reject
blank or partially parsed values rather than accepting parseFloat prefixes. Generate,
Optimize, Pipeline and the overview pipeline action collect settings through a common
error handler and stop before sending a request when parsing fails.

A production-source JavaScript regression checks valid formats, invalid lines,
duplicates, nonfinite bounds, field errors and the actual Generate click handler:
malformed settings produce a visible error and zero POST calls. This regression is
now part of ci/check.sh. Browser rendering has not been retested for these new
messages; the test exercises the real handler with a controlled DOM stub.

Current release verification: the complete release gate passes with 212 Rust tests,
a successful locked workspace release build, inline/standalone JavaScript checks
and all three production-source JavaScript regressions. This includes the recent
portfolio, robustness, generator and candidate-linked pipeline changes. No remote
CI or required-merge-check enforcement is implied by this local evidence.

## Strict simulation assumption parsing

Dashboard simulation configuration parsing now returns Result. Present but malformed
costs, deposit, tick options, intrabar policy and account settings fail explicitly;
only omitted fields use the documented defaults. All current config_from callers,
including archived backtests and frozen experiment creation, propagate errors before
execution or persistence. The frontend numeric settings reader also rejects empty,
nonfinite and partially numeric text instead of silently using parseFloat prefixes
or default zero costs.

Two regressions cover invalid field types/unknown policy, legitimate omitted defaults,
and an invalid-spread request producing no archived run or archive directory. The
production-source JavaScript regression now checks the actual settings reader for
blank/partial/infinite values and legitimate zero costs. The complete debug gate
passes; the extended JavaScript regression also passes independently. Other frontend
actions' error presentation and all non-dashboard configuration paths still require
a broader input-validation audit.

### Completed pipeline reports: immutable archive and replay

Dashboard pipeline computations now archive their frozen dataset, instrument, calendar,
configuration, generator, seed, original candidates and evaluated optimization families
before publishing survivors to the databank. Computed failure reports are archived too;
retained-candidate backtest errors cannot become ordinary metric rows. Saved reports use
the existing checksum-verified backup/restore and matching-engine replay mechanism.
Replaying does not update the mutable databank. The saved-run list distinguishes pipelines
from backtests, and experiment exposure audits count overlapping pipeline reports separately,
including failed reports. One report is explicitly not a count of every research trial.

Validation: full `ci/check.sh debug` passed (workspace tests/build and frontend regression
checks); added a frozen-input replay/JSON backup restoration test for successful and failed
pipeline computations, and a targeted experiment exposure test. No browser visual validation
was performed for the added archive status text.

Limits: this archives completed computations, not a pre-registered durable ledger of every
Build candidate or Monte Carlo sample. A process failure before archive publication is not
recoverable as a pipeline job. Pipeline discovery still reuses the full dataset and is
exploratory, not untouched holdout evidence. Durable whole-pipeline execution, exhaustive
independent formula verification, broker execution parity and remaining professional release
criteria remain open; the six-part goal is not complete.

### Optimization requests preserve declared workload

The dashboard optimization endpoint now uses typed settings: present null, fractional or
wrong-type integer fields, unknown modes, zero trade thresholds and out-of-range window/OOS
settings are errors rather than defaults. Simple discovery accepts 1–2000 candidates;
walk-forward/matrix accept 1–200 per window. Requests above the limit are rejected instead
of silently capped. Matrix cells now use the requested count (previously always 50), and the
matrix response records seed and candidates per window. Missing fields retain explicit defaults.
Actual endpoint regression tests verify two-window candidate totals and all twelve matrix
cells, plus rejection before databank/archive mutations. Pipeline count/keep parsing remains
a separate outstanding input-validation item; this change does not claim to repair it.

### Pipeline workload and seed validation

Pipeline requests now strictly deserialize count, keep and seed. Count must be 1–2000,
keep 1–200 and no larger than count. Present malformed/null/fractional integer values
are rejected before computation or archival; omitted fields retain documented defaults
(300, 20, 42). No workload is silently clamped. An explicit seed now reaches the pipeline
engine, is frozen in its archive request and is used by replay instead of hardcoded 42.
The archive restoration regression now exercises seed 123 for completed and failed reports.
The data-gap regression supplies a valid keep/count pair so it still tests the data gate.
The pipeline UI minimum count matches the endpoint. Full debug workspace gate passed,
including tests, build, JavaScript syntax and frontend regressions. Browser rendering was
not rechecked for the numeric minimum attribute change.

### Walk-forward training execution failures

Training backtest errors were previously passed to metric computation and ranking without
being retained in the walk-forward report. Failed candidates now bypass ranking and remain
recorded per window with candidate index, full strategy and execution errors. Aggregate
errors include window/candidate identity; neither affected windows nor the complete report
can satisfy evidence criteria. Successful candidates may still produce explicitly incomplete
exploratory results. OOS failures retain the selected strategy and identify the window.
A regression exercises a strategy with scheduled exits but no session cutoff across three
windows and verifies every attempted candidate/error survives and no candidate is selected.
Full debug workspace gate passed; subsequent window-evidence wording/gating adjustment was
verified by the optimizer test suite. Full release and browser checks were not rerun here.

### Walk-forward training capital matches execution capital

Each segmented walk-forward window now trains candidate strategies with the same carried
starting balance used for its OOS execution, rather than restarting IS sizing at the original
initial deposit. Only completed preceding OOS windows affect that balance. This keeps
risk sizing, lot rounding and margin eligibility consistent between selection and execution.
Historical prefixes still warm indicators without creating pre-window trades.
An integration regression independently reconstructs the three IS rankings and OOS executions
from a fixed risk-sized family, verifies capital actually changes, and compares selected
strategies and final-equity bits. Full debug workspace tests/build and frontend gates passed.
This remains segmented flat-at-boundary walk-forward, not continuous open-position transfer.

### Consistent break-even trade statistics

Core metrics previously counted zero-PNL trades as profits while analyzer counted them as
losses. Both now use strictly positive wins and strictly negative losses; flat trades remain
in total trade count and the winning-percentage denominator, but neither win/loss averages
nor either streak. A flat trade breaks both streaks. This definition uses net trade PNL,
including the engine's charges, and handles negative zero identically. Hand fixtures verify
mixed outcomes, both signed zeros, averages, streaks and all-flat undefined ratios. Full
debug workspace tests/build and frontend gates passed. Historical saved reports are not
rewritten; replay still requires the matching engine executable.

### Core Sharpe return baseline and undefined periods

Core Sharpe now includes the initial-deposit-to-first-end-of-bar return. Its explicit contract
is zero risk-free simple bar returns, sample standard deviation, square-root annualization;
equity must contain end-of-bar samples without an extra initial deposit row. Nonfinite data,
nonpositive prior equity or invalid annualization make Sharpe undefined rather than silently
omitting affected returns. The terminal return into zero equity itself remains observable.
A hand fixture (+10%, -10%, +20%) checks the exact expected ratio and tests insolvency,
nonfinite observations and invalid annualization. Full debug workspace tests/build and
frontend gates passed. This changes historical Sharpe values and potentially Sharpe rankings;
archived reports remain immutable. Analyzer rolling Sharpe still has a separate implementation
and requires a follow-up consistency audit; no claim of full metric verification is made.

### Analyzer rolling Sharpe shares core formula

Analyzer now calls the core's exported sharpe_from_equity function, providing the balance
immediately preceding each end-of-bar window. This includes the first return consistently
and removes synthetic zero returns after insolvency and synthetic zero ratios for flat
windows. Quarter-year windows use round(periods_per_year / 4), minimum two observations;
insufficient history no longer silently shortens the requested window. These are bar-count
windows based on supplied annualization, not calendar-quarter boundaries. Undefined values
remain aligned in the series. Markdown reports their count and labels aggregate summaries
as defined-window-only, with an explicit no-defined-values case. Hand fixtures cover two
successive windows, short history, flat and insolvent accounts. Full debug workspace tests,
build and frontend checks passed. Browser presentation was not separately inspected.

### Analyzer monthly returns and drawdown baseline

Monthly returns now begin at the initial deposit rather than the first candle's ending
equity. Subsequent months continue from the preceding observed month's ending equity;
nonpositive denominators yield undefined returns rather than fabricated zeros. Drawdown
periods also start from the initial deposit and inspect the first equity observation,
including single-candle losses. For an initial loss the start timestamp is the first observed
bar (no earlier timestamp is invented). Recovery duration remains trough-to-recovery bars.
Hand fixtures verify -20% then +10% compounding to 88% of initial capital, insolvency,
single-observation drawdown, a deeper trough and recovery. Full debug workspace tests/build
and frontend gates passed. Missing calendar months are still absent, not imputed as flat;
this does not certify source-data completeness or calendar alignment.

### Analyzer failure propagation

AnalysisReport now retains execution/validation errors. Failed backtests, invalid annualization,
invalid initial/final capital, nonfinite equity or nonchronological observations suppress derived
monthly/drawdown/rolling tables and yield undefined summary metrics. Markdown renders an explicit
unavailable/error report. CLI Analyze returns an error and the dashboard endpoint exposes the
error instead of a normal analysis response. Fixtures cover execution failure, zero annualization,
NaN equity and duplicate timestamps plus a valid control. Full debug workspace tests/build and
frontend checks passed. This is not a complete audit of all custom BacktestResult invariants
(e.g. equity/trade reconciliation remains separate).

### Reject corrupt numeric observations before metrics

Shared validate_observations rejects nonfinite account/equity and trade fields, overflowing
PNL aggregates and overflowing summed trade durations. Core compute returns undefined metrics
for these inputs (so ordinary ranking cannot select them); analyzer also records the explicit
validation error and suppresses derived reports. Legitimately undefined/infinite derived
ratios are distinct from nonfinite raw observations. Regression cases cover NaN PNL/size/equity,
infinite commission/financing, finite inputs with aggregation overflow and valid flat trades.
Full debug workspace tests/build and frontend regressions passed. This numeric gate is not
a complete reconciliation of trade cash flows, position lifecycle or final equity.

### Numeric execution settings validated before simulation

Strategy::validate_execution_settings checks all sizing variants for finite positive numeric
parameters, protective SL/PT distances and ATR periods/multipliers, and positive bar-count
exits. The backtest entry point invokes it before signal computation or trading, so malformed
settings produce explicit execution errors rather than silently rejected/altered trades.
Engine regressions cover zero/negative/NaN/infinite sizes, invalid stop distances/ATR periods,
sizing stops, targets and zero-bar exits, alongside a valid control. Full debug workspace
gate passed; added targeted engine regression also passed. This is numeric validation only:
full rule/catalog validation and detailed sizing semantics remain separate audit items.

### Signal ambiguity and numeric validation

Execution validation now rejects duplicate signal kinds (previously last-wins overwrite),
negative price shifts (previously silently clamped to zero), nonfinite constants/indicator
parameters, duplicate parameter names and fuzzy thresholds outside finite (0,1]. Confirmation
signals receive the same numeric validation. Empty conditions retain existing false semantics.
Engine regression fixtures verify rejection without trades and a valid fuzzy=1 control.
Full debug workspace tests/build and frontend checks passed. Catalog existence, supported
parameter domains and the low-level standalone evaluator API remain separate audit work;
this change guards the backtest execution path, not every direct compile call.

### Indicator names and declared parameter wiring

Execution validation rejects unknown indicator names and undeclared parameter names using
the catalog, while preserving the supported legacy KAMA/TEMA registry names and their period
parameter. Omitted parameters retain defaults. Engine tests verify misspellings fail rather
than producing a silent all-NaN signal. Catalog/registry comparison also exposed BullsPower
and BearsPower ignoring their declared period: registry execution now computes high/low minus
EMA(close, requested period), default 13. A hand fixture checks EMA(2) warmup/seed values and
that the default period differs. Full debug workspace gate passed, including frontend checks.
General numeric parameter-domain validation remains open; catalog membership alone does not
prove a parameter value is valid or an indicator formula independently verified.

### Indicator numeric domains

Backtest execution now rejects nonpositive/fractional/unrepresentable period/fast/slow/signal
parameters instead of registry rounding or fallback, rejects nonpositive multipliers and SAR
coefficients, and rejects SAR step above resolved maximum (including defaults). Generator
ranges require positive endpoints and whole-number representable endpoints for period fields.
Regression tests exercise invalid SMA periods and invalid generator endpoints. Full debug
workspace gate passed. This covers current catalog parameter names, not a comprehensive bound
on indicator CPU/memory: extremely large otherwise representable periods and joint generator
constraints still need resource/domain review. Low-level indicator functions retain their APIs.

### Release verification after execution/metric/validation changes

Ran `bash ci/check.sh release` on the current working tree: 235 Rust tests passed,
zero failed and zero ignored across 59 test/doc-test invocations, followed by successful
locked release workspace build. Workspace/inline JavaScript syntax and the backup-wire,
search-recovery and generator-input regressions passed. Evidence log:
`work/ux-implementation/reliability-release-current.log`.
This validates the current local release profile, not remote CI, cross-platform execution,
broker parity, deployment or the full six-section completion requirements. Existing compiler
warnings remain (deprecated atomic API and an unused test import). No commit or push was made.

### Independent CMO rolling-window fixtures

Added hand-calculated three-change CMO fixtures based on the published Fidelity CMO formula:
https://www.fidelity.com/learning-center/trading-investing/technical-analysis/technical-indicator-guide/cmo
(accessed 2026-11-23). Mixed changes assert 500/9, 100/7 and 60 on successive windows,
which discriminates against Wilder-smoothed variants. Additional fixtures check exact warmup,
positive affine invariance, all-up/down limits, missing-input windows and recovery. Flat-window
zero is explicitly identified as the implementation's neutral convention, not an independently
specified denominator-zero formula. All 11 reference tests passed in debug and release. No
production code changed; full workspace gates were not repeated for this test-only increment.

### TII original-definition fixtures and oversized-period safety

Checked against M.H. Pee's June 2002 description:
https://traders.com/documentation/feedbk_docs/2002/06/Abstracts_new/Pee/pee
The original uses recent closes against the current longer SMA. New hand fixtures assert
75 then 55 for n=2/SMA4 and 175/3 for a constructed 30/60 sample, with exact first-valid
index. Existing regression already covers the 61.538462 example. TII now checks n>len/2
before computing 2*n, preventing debug overflow/release wrap for oversized inputs and
returning an aligned undefined series. Entire indicator crate tests passed in debug and
release; no full workspace rerun for this local indicator-only change. Other indicators'
large-period arithmetic remains unaudited.

### Confirmation timeframe fail-closed behavior

Tick confirmation previously returned an all-true vector because it has no candle duration.
Execution now explicitly rejects it: OHLC resampling cannot reconstruct tick confirmation.
Defensive missing-confirmation-value fallbacks are false rather than true. A regression uses
an impossible confirmation condition to demonstrate it cannot be bypassed. A separate M5
fixture verifies entry waits for a closed bucket and altering future buckets leaves earlier
entries/equity unchanged. Full debug workspace gate passed; confirmation tests were rerun
following the defensive fallback adjustment. This does not claim tick confirmation support
or settle calendar-month/week alignment and partial source-bucket completeness.

### UTC calendar buckets for resampling and confirmation

Added Timeframe::bucket_bounds: intraday/day fixed UTC buckets, Monday-start UTC weeks,
and true calendar months using chrono (already present in workspace dependencies). Resampling
now groups by these boundaries and stamps each aggregate with its bucket opening time.
Confirmation availability uses the bucket's actual end rather than adding approximate month
minutes. Backtests reject timestamps outside the supported confirmation calendar range.
Tests cover leap/non-leap February, year rollover, Monday weeks, pre-epoch minutes, aggregate
OHLCV and a February 2024 confirmation becoming tradable exactly March 1. Full debug workspace
gate passed plus the added monthly execution test. Broker-specific week/session anchoring and
incomplete-source-bucket detection remain open; resampling does not certify complete coverage.
Timeframe::minutes still represents MN1 nominally for legacy non-boundary callers and requires
further call-site audit. These UTC changes do not claim broker-calendar parity.

### Monthly dataset alignment and shared close boundaries

Monthly data-quality auditing now rejects candle open timestamps other than the UTC first
of the month, rather than accepting a mid-month partial candle as a full monthly observation.
Monthly candle_end delegates to the same core calendar boundaries used by resampling and
confirmation. Fixtures cover leap-February consecutive months, exactly one missing month,
mid-month rejection and matching close boundaries. Full debug workspace tests/build and
frontend gates passed. QuestDB native-duration uses only fixed native timeframes; monthly
aggregation already uses SAMPLE BY 1M. QuestDB weekly anchoring, broker-local month conventions
and aggregation coverage within buckets remain unverified and are not certified by this check.

### Checked resampling API

Core resample now returns Result<Vec<Bar>,String>: invalid/out-of-calendar timestamps are
errors rather than silently omitted rows, tick destinations are explicitly unsupported,
and unordered/duplicate source rows, malformed OHLCV and aggregate-volume overflow fail.
All workspace callers were migrated; confirmation propagates errors before trading and no
longer needs a permissive unsupported-timeframe branch. Regressions cover each error and
existing calendar/OHLCV fixtures still pass. Full debug workspace tests/build and frontend
gates passed. This is a source-breaking API change for external library callers, which must
handle Result. It still does not establish missing-source-interval completeness or broker
session alignment; those require source timeframe/calendar provenance.

### Omit a truncated leading confirmation bucket

When source history starts after the UTC confirmation bucket opening, that first aggregate
is omitted before computing confirmation signals/indicator warmup. ExecutionEvidence records
its timestamp as skipped_leading_confirmation_bucket. A fixture beginning at minute 2 cannot
trade on the partial 0–5 minute candle; it first qualifies at minute 10 after the next bucket.
Full debug workspace tests/build and frontend checks passed. This is conservative without a
session calendar (a market opening mid-bucket also causes omission), and does not establish
internal interval completeness, source-candle durations, or broker session alignment. Inspection
confirmed existing bucket-end gating excludes future timestamp buckets; no general new
look-ahead defect was established in that gating.

### Visible confirmation-history limitation

Dashboard backtest results now show a plain-language notice when the engine omitted a
truncated leading confirmation candle, including its UTC timestamp and the limitation that
later-candle completeness is not proven. Rendering uses textContent. A production-function
JavaScript regression verifies missing/legacy evidence, null, epoch zero and invalid timestamp
handling; it is part of ci/check.sh. Full debug workspace gate passed. Browser visual layout
was not inspected for this text-only addition; archived JSON already retains the evidence.

### Stop-based sizing cannot invent a stop distance

FixedAmount/CryptoFixedAmount and RiskFixedBalancePct/CryptoFixedBalancePct divide their
monetary risk by actual protective-stop distance. They previously used a hidden 100-point
distance when no stop existed. Execution validation now requires an explicit stop for those
variants, and the distance helper no longer supplies 100. Engine regressions cover all four
missing-stop failures and valid explicit stops, while fixed lots and separately specified
sizing distances remain supported. Full debug workspace gate passed. A sizing distance alone
(RiskFixedPctOfAccount or ATR sizing) is not a protective order and does not guarantee a maximum
loss; gaps, costs and margin still affect realized risk. Full sizing semantics remain under audit.

### Price-based allocation respects contract size and account currency

StocksSizeByPrice/CryptoSizeByPrice formerly returned underlying units as if they were lots
and divided account money by an unconverted quote-currency price. They now divide allocation
by entry price times contract size times the causal quote/account conversion rate already
resolved by the engine. Nonfinite/nonpositive denominator cannot produce a usable size.
Model documentation identifies this as notional allocation, not stop-loss risk. Hand engine
fixtures verify both variants on USD contract-size-one and JPY contract-size-ten instruments:
2% of $10000 gives $200 notional and a 10% price rise yields $20 net before costs. Full debug
workspace gate passed. Existing margin/lot constraints still apply; costs can consume capital
in addition to the requested notional allocation.

### Dynamic exit prerequisites and duplicate configuration

TrailingStop and MoveToBreakEven derive their distance from the initial protective stop.
Without that stop their distance was infinite and the requested exits silently never activated.
Validation now requires a stop for those exit modes. Duplicate exit types are rejected,
including multiple ExitAfterBars values that formerly used only the first. Model comments
state the initial-distance trigger and that break-even refers to entry price before costs.
Regression tests cover missing-stop errors, valid explicit stops and conflicting bar exits;
existing dynamic-exit chronology tests remain green. Full debug workspace gate passed.
This does not add independently configurable trailing/BE distances or guarantee net-of-costs
break-even; those would require explicit model/UI options and execution tests.

### Overnight UTC entry-hour windows

Entry-hour windows now support start>end as a midnight-crossing interval. Hours use execution
time with inclusive start/exclusive end; (0,24) is a full day, equal endpoints and invalid
hours are rejected. Previously a 22–02 window silently prohibited all entries. Engine fixtures
cover 21,22,23,00,01,02 execution hours and invalid/full-day settings. Full debug workspace
gate passed. These remain UTC hours rather than broker-local/DST-aware sessions; tick-based
daily trade-cap rollover still requires its separate execution-time audit.

### Daily entry cap uses the executable quote day

A regression with a candle opening before midnight and its first quote after midnight
reproduced two entries despite max_trades_per_day=1. The engine now derives its current
UTC day from entry_time (first quote, or bar open for OHLC) rather than the containing
candle timestamp. Entry-cap reset and initial financing day agree with actual execution.
The fixture also verifies that a position opened and stopped after midnight incurs no
prior-day rollover. Before-fix failure: work/ux-implementation/tick-day-before.log;
full debug workspace gate passes afterward. Rollover occurring later inside the same tick
interval still needs a separate per-tick financing audit; this change only fixes the
interval's executable opening day.

### Financing follows intrabar tick chronology

A before-fix regression demonstrated missing financing when an open trade crossed midnight
and exited within one candle. Shared financing accrual now runs at the executable interval
opening and before every tick's stop/margin/session checks. It records the latest charged UTC
day and cannot charge the same rollover twice or move backward. Opening risk conversion also
uses the executable quote timestamp. Tests distinguish exits before/after midnight and verify
financing alone triggers stop-out on the first post-rollover quote. Full debug workspace gate
passed, followed by all eight tick tests including the added stop-out fixture. Financing still
uses configured UTC daily/per-lot rates and triple-day rules, not historical broker-specific
funding calendars or changing rates.

### Dynamic valuation failures return execution errors

Entry, trade-exit, final-liquidation and equity-valuation conversion errors now propagate as
invalid run results rather than expect panics. Entry conversion is resolved once and reused
for sizing/margin. Inspection confirmed ordinary excessive slippage was already guarded for
both bars and ticks; the actual uncovered numeric case is finite tick ask plus finite slippage
overflowing the executed fill. A regression covers that overflow, plus existing invalid-price
rejection for both directions. Full debug workspace gate passed. No general ordinary-slippage
bug is claimed. Stop-out helper assumptions and comprehensive arithmetic-overflow handling
remain separate audit items.

### Backtest UI settings and uncertain transport failures

The workspace backtest action catches strict settings validation errors and shows them before
sending a request. Network/JSON response failures no longer leave an unhandled rejection and
Running status: the notice explicitly says the server may have completed/archived the run and
tells the user to refresh saved research before retrying. It does not invent a failed-run
record or automatically retry. Production-handler VM regression checks zero requests for
invalid settings, uncertain response wording, unchanged history and explicit server errors;
added to the common CI gate. Full debug workspace gate passed. Browser visual verification
and durable idempotency for standalone backtest requests remain outstanding.

### Detailed batch evaluation preserves failed attempts

Added backtest_batch_detailed with one ordered strategy/metrics/errors result per input,
including execution and numeric-observation validation errors. Parallel evaluation preserves
input order. Pipeline Build uses it, excludes failed candidates and adds step/index/name/error
to its report instead of silently losing failure reasons during ranking. A 12-candidate mixed
valid/invalid fixture verifies order, counts and errors across the parallel path. Full debug
workspace gate passed; optimizer/pipeline suites rerun after numeric-validation propagation.
The compatibility metrics-only batch API remains for existing callers. This is not yet a
durable complete ledger of every successful/rejected candidate across every research route;
that larger provenance requirement remains open.

### Completed pipeline archives retain every Build candidate

PipelineReport now carries all Build evaluations with step and input index, full strategy,
metrics, errors and selected flag. Dashboard pipeline JSON includes these records and the
selection criterion (profit factor, five-trade minimum, keep count) in both successful and
computed-error reports. They therefore enter the existing immutable pipeline archive and
matching-engine replay/backup path. Tests verify all twelve candidates survive reporting even
when none pass selection, seeded repeated identities, and Build-record counts in restored
pipeline replay. Full debug workspace gate passed. Persistence still happens at completed
report publication, not per-candidate checkpoints; process interruption and full retest-sample
history remain open. Nonfinite JSON metrics retain the existing null representation.

### Completed pipeline archives retain Retest decisions

Pipeline reports now retain each Retest candidate's full strategy, execution step, seed,
requested Monte Carlo runs, positive-run threshold, errors, summary statistics and selection
outcome. Dashboard success and computed-error responses include these records, together with
the fixed 10% trade-drop probability, so completed pipeline archives retain rejected decisions
as well as survivors. A nonempty seeded Build/Retest fixture checks candidate coverage and
recomputes Monte Carlo summaries and decisions at thresholds 0% and 100%. Full debug workspace
tests/build and frontend checks passed; the added targeted pipeline suite also passed. This
is summary-level evidence at completed-report publication, not per-sample storage or durable
mid-pipeline checkpoints. Those broader provenance and recovery requirements remain open.

### Monte Carlo path-level evidence

Trade-manipulation reports retain each generated sample in original run order, including
zero-based index, retained trade count, net profit and maximum drawdown. Pipeline report JSON
and the standalone robustness response include these samples; completed pipeline archives
therefore retain the sample distribution rather than only quantiles. Standalone responses now
also disclose seed 42 and skip probability 0.1. Tests use a two-trade hand-checkable fixture,
reconstruct reported quantiles from samples and check exact seeded repeatability; pipeline
recalculation checks the complete sample sequence. Monte Carlo failures now also enter the
pipeline's top-level errors, preventing invalid execution from looking like a normal completed
selection. Full debug workspace tests/build and frontend checks passed, followed by the updated
pipeline suite. Samples do not store individual shuffled trade identities, partial samples are
not retained on arithmetic failure, standalone robustness responses are not yet archived, and
mid-pipeline durable checkpoints remain open. Memory grows by one small record per MC run.

### Standalone dashboard robustness archives and replay

The dashboard robustness route now publishes its computed report through the immutable archive,
including full strategy, dataset/instrument/calendar, actual account configuration, six cost
scenarios, Monte Carlo run count/seed/drop probability and original data-quality context.
Replay uses this frozen specification and the matching executable, without consulting live
market data or writing a databank. The archive list identifies stress-test reports separately.
The exposure audit counts these archives separately and flags overlapping observation ranges,
including computed failure reports, as known data reuse; the dashboard displays that count.
A route-level test runs 1000 samples, restores the backup into an independent empty-data state,
and verifies exact report replay. An exposure regression covers a failed robustness report.
Full debug workspace gate passed; the added exposure test passed in the project suite.
Publication remains after computation, without an interruption checkpoint or request identity.
CLI/MCP/assistant robustness calls are not covered by this dashboard archival route.

### Dashboard stress-test failure visibility

Both robustness entry points now use a shared request wrapper. Invalid settings prevent the
request; a lost response explicitly reports uncertain completion and directs users to saved
research runs before retrying. Both views identify archived reports, including archived base-run
failures. The results-list quick view now surfaces Monte Carlo and cost-scenario errors, data-gap
acknowledgment and exploratory interpretation instead of presenting unavailable MC numbers as
an ordinary summary. Tests execute the actual JavaScript helpers and verify no request on
invalid settings, uncertain-response messaging, escaped diagnostics and report identity on
failed runs. Full debug workspace tests/build and all six frontend regressions passed.
Visual browser inspection and automatic refresh/navigation of saved-run lists remain separate;
this change adds no request deduplication or background execution.

### Incomplete random-start retests cannot publish distribution statistics

A 50-bar regression demonstrated that some sampled starts were skipped while the remaining
runs produced finite percentiles with no error. Monte Carlo retest now reports incomplete
coverage and suppresses distribution percentiles and losing-run percentage whenever any
requested scenario is skipped, just as it already does for invalid runs. Evaluated/skipped/
invalid counts and sampled offsets remain available. Nonfinite net profit, final equity or
equity drawdown also marks a scenario invalid; NaN equity observations can no longer vanish
inside drawdown comparisons. Hand-checked initial-loss/drawdown and nonfinite-observation tests
cover the helper. The partial-coverage regression failed before the fix and passes afterward.
Full debug workspace tests/build and six frontend regressions passed; the robustness suite
was rerun after adding the accounting test. This does not establish statistical sufficiency
or replace independent holdout validation.

### Updated optimized-release verification

After calendar, sizing, pipeline/robustness provenance and incomplete-retest fixes, the full
local release gate was rerun with Rust 1.100.0: 264 tests passed across 60 test invocations,
zero failed and zero ignored. The optimized workspace build, inline/workspace JavaScript
syntax checks and all six frontend regression scripts passed. Log:
`work/ux-implementation/reliability-release-latest.log`. `git diff --check` was also clean.
The existing workflow declares macOS/Linux debug/release jobs, but this local result is not
evidence that remote CI or Linux has run. Existing deprecated atomic API and unused test-import
warnings remain. No release, commit or deployment was performed; these checks do not complete
the six programme requirements or certify correspondence with broker execution.

### Closed-trade reconciliation primitive

Added `sqx-project::reconciliation` for explicit one-to-one correspondence between expected
closed trades and imported observations. It reports direction, entry/exit timestamp, price,
size, net PnL, commission and signed financing discrepancies under explicit absolute tolerances.
Reports retain expected/observed values and tolerances. Missing expected trades and unmapped
observed tickets are separate; duplicate tickets, multiple tickets mapped to one expected
trade, unknown references, reversed timestamps and nonfinite data are rejected. Empty sets
cannot count as matching evidence. Timestamp differences use overflow-safe unsigned distance.
Five tests cover exact boundary tolerances, missing/extra trades, independent cost/direction/
size discrepancies, ambiguous mapping and malformed/extreme observations. Full debug workspace
tests/build, six frontend scripts and whitespace checks passed.
This is a comparison primitive, not the completed paper-trading workflow: instrument/currency/
observation-period compatibility must be established by its caller; import UI, archive linkage,
broker adapters, partial-fill aggregation, order/open-position comparison and real broker
fixture verification remain to be implemented. No external broker results were fabricated or
used, and no deployment/validation badge is granted by this comparison.

### Reconciliation scope compatibility

Added a scoped import-comparison entry point retaining both declared contexts. It requires
exact normalized market identity, quote currency, account currency, contract units per lot
and UTC observation boundaries. Invalid/blank identifiers, nonpositive or nonfinite contract
sizes and empty/reversed periods fail. Trades crossing the declared period are rejected rather
than filtered, even when timestamp tolerances would otherwise accept the discrepancy. The
seven-test reconciliation suite includes each context mismatch and boundary-invalid trade;
full debug workspace tests/build, six frontend regressions and whitespace checks passed.
These declarations still require importer provenance and linkage to a frozen archived run.
Broker aliases, currencies and lot units are deliberately not guessed or silently converted.
The low-level field comparator remains available, while imported reports should use the scoped
entry point. Open positions, partial fills, archive-backed UI and real broker evidence remain
outstanding; matching declarations do not prove complete broker coverage.

### Archive-linked closed-trade comparison

`compare_archived` now derives expected trades and economic context from a verified immutable
dashboard backtest bundle, retaining run/dataset/strategy/executable identities. It never
reruns the strategy or reads current market data. Failed/non-backtest reports, missing trade
fields, inconsistent counts and duplicate expected IDs are rejected. End-of-data liquidation
IDs are exposed separately so comparison consumers cannot treat them as ordinary live signals.
The observation interval covers the archived bar data through the final bucket's last
millisecond; Tick datasets without a reconstructible bucket end are rejected. Tests verify
exact comparison after backup restore, reject incomplete/wrong-kind reports and detect archive
corruption. Full debug workspace tests/build, six frontend regressions and whitespace checks
passed. This remains a library entry point: observed-file import/provenance, persisted comparison
artifacts, dashboard controls, open-position/order matching and real broker verification are
still outstanding. Content integrity does not independently certify the broker's declarations.

### Portable immutable comparison records

Paper comparisons now have a strict versioned JSON import format and content-addressed atomic
publication in a separate comparisons namespace. A record embeds the complete reference
backtest bundle, exact UTF-8 source JSON and its hash, explicit tolerances and derived result.
Identical inputs reuse the same ID; source whitespace changes remain distinguishable evidence.
Load/restore verify all embedded hashes and recompute the comparison before accepting a report.
A comparison backup restores independently without creating a new research-run/exposure record.
Tests cover identical-save identity, exact source retention, portable restore, unknown import
fields and a modified result even when its outer hash is recomputed. Imports are bounded to
4 MiB/10000 closed trades with a nonblank source label. Full debug workspace tests/build, six
frontend regressions and whitespace checks passed. Labels and normalized import data are user
assertions, not broker authentication. No UI/API import route, comparison listing or raw broker
format adapter exists yet; real broker evidence and order/open-position matching remain open.

### Comparison server routes

The dashboard server exposes comparison import, validated listing, read/backup and restore
through `/api/comparison/import`, `/api/comparisons`, `/api/comparison?id=...` and
`/api/comparison/restore`. Requests use strict typed JSON and the existing local HTTP access
and body-size controls. Import/read/restore preserve source bytes and reference evidence;
listing reports missing/extra counts, correspondence and end-of-data liquidations with
`deployment_ready: false`. Comparison storage remains outside the research-run namespace.
A handler regression imports an actual archived empty-strategy backtest, restores without
live data, compares exact backup content, ensures empty sets never pass, and rejects invalid
tolerances and altered backups. Nonempty comparisons are covered by project-level tests.
Full debug workspace tests/build, six frontend regressions and whitespace checks passed.
Dashboard form/list controls, browser-level flow verification and real broker imports remain
open. The current routes import the normalized JSON format, not raw MT5 broker exports.

### Paper comparison dashboard controls

The backtest workspace now includes archived-run selection, normalized JSON file import,
explicit absolute tolerances, saved comparison listing, discrepancy inspection, complete
backup download and restore. It shows missing/unexpected IDs, expected versus observed field
values and end-of-data liquidation warnings. Imported text is escaped, empty/invalid
nonnegative tolerances are rejected, time tolerance requires a safe integer, and a lost import
response reports uncertain completion. Matching is consistently qualified as closed-trade
correspondence, not live readiness. `docs/PAPER_TRADING.md` documents the strict normalized
import schema and limitations. Full debug workspace gate passed; a seventh frontend test
then passed for actual tolerance/parser/render helpers and was added to the gate. Browser-level
visual/end-to-end verification, raw broker adapters and real broker evidence remain open.

### Browser inspection of paper comparison panel

Opened the current local build on an isolated synthetic-data preview (port 18090). Verified
that the panel expands, loads an archived reference option and empty comparison list, and
submitting without reference/file displays an actionable message. Narrow-screen inspection
revealed the long reference selector overflowing its card; scoped CSS now constrains labels,
select/file controls and wraps long evidence identifiers. After rebuilding/reloading, the
selector's measured right edge equalled its container edge (297 px), and the visible missing-
input message remained correct. CLI build, inline-script syntax, comparison UI regression and
whitespace checks passed. This was panel/validation/responsive inspection, not a complete
browser import/download/restore cycle or real broker reconciliation. Preview data are synthetic;
no production databank was modified. The temporary preview server remains at port 18090.

### Browser import/download/restore exercise

Using the isolated synthetic preview on port 18090, imported a labelled synthetic fixture
with six mapped trades against a seven-trade archived backtest, one deliberate +1 PnL change
and the final expected trade omitted. Browser output correctly showed a PnL discrepancy,
missing trade ID 6, six matched pairs and no correspondence approval. Downloaded the comparison
backup through the UI and restored that file through the UI; the visible integrity/recomputation
success message appeared. Independent inspection confirms the downloaded source string matches
the fixture byte-for-byte and its record equals the stored comparison. A screenshot records
the discrepancy table. Inspection also exposed ambiguous wording where a missing ID could be
read as a count; the label now explicitly says "Missing expected trade IDs", covered by the
frontend regression. Syntax and comparison regression passed after this wording change.
This is synthetic workflow evidence, not independent broker execution validation; no real
paper-trading claim follows. User production data was not modified.

### Portable comparisons retain known data exposure

A regression demonstrated that restoring only a paper-comparison backup lost exposure evidence:
the embedded historical backtest was absent from the separate research-run namespace, so an
overlapping final-test experiment incorrectly reported no known registry overlap. Exposure
assessment now verifies saved comparisons and examines their embedded reference datasets,
records comparison/reference IDs and counts comparison records separately. The dashboard shows
this count. Record counts are explicitly not independent trial counts and may share a reference.
The regression failed before the fix and now passes; unrelated markets and nonoverlapping
periods remain clear. No duplicate research run is created by restore. Full debug workspace
tests/build, seven frontend checks and whitespace checks passed; the expanded boundary test
was rerun afterward. This closes a known-registry evidence gap, not exposure outside the tool.

### Comparison provenance visible in dashboard

Saved-comparison summaries now include the user-declared source label and number of paired
trades with differences, separately from missing/unexpected trades. Detailed reports display
that source label, normalized market, quote/account currencies, contract units and full UTC
observation period alongside tolerances and source/reference hashes. A synthetic source label
therefore remains visibly synthetic instead of being hidden behind a content ID. Labels are
rendered as text/escaped HTML and explicitly remain unauthenticated declarations. Handler
checks verify source labels/counts; the frontend test verifies escaped labels, UTC interval,
currency and differing-pair counts. Full debug workspace tests/build, seven frontend checks
and whitespace checks passed. This presentation change has not yet been visually inspected
in the browser; it adds no broker authentication or additional trading-validity claim.

### Shared observation validation for robustness scenarios

Cost-sensitivity rows and random-start retests now apply the same numeric-observation validator
as ordinary metrics, covering account/equity values, all numeric trade fields, gross-PnL
aggregation and duration overflow. Cost rows additionally reject nonfinite net aggregation;
invalid rows retain their scenario settings/errors and expose undefined metrics instead of
apparently valid results. The robustness crate now directly depends on sqx-metrics (lockfile
updated offline). A targeted fixture checks valid no-trade semantics, NaN equity, finite-input
aggregation overflow, preserved scenario settings and original execution errors. Full debug
workspace tests/build, seven frontend checks and whitespace checks passed. This unifies numeric
validity checks; it does not add broker reconciliation or certify accounting assumptions.

### Genetic evolution evaluation observer

Genetic evolution now evaluates through the detailed batch path and exposes an ordered observer
with generation, population index, full strategy/metrics/errors and ranking eligibility for
every evaluation, including the final population. Invalid evaluations are explicitly excluded;
an observer failure aborts instead of returning a successful search. If no candidate qualifies,
the already-evaluated population is no longer redundantly evaluated a second time. The original
ranked-return API remains a wrapper. CLI evolution uses the observer to report total/invalid/
eligible/valid-rejected counts and first ten invalid diagnostics, with explicit repeated-elite
count semantics. Tests verify 12 early-rejected attempts are observed exactly once, a 60-attempt
three-population seeded trace repeats exactly and observer failure stops reporting success.
Full debug workspace tests/build, seven frontend regressions and whitespace checks passed.
The observer runs after each population batch; it is not a per-candidate pre-execution claim,
durable ledger or resumable evolution job. The CLI currently prints bounded diagnostics rather
than archiving the full stream. Those provenance/recovery requirements remain open.

### Optional durable CLI evolution evidence stream

`evolve --audit-log <new-path>` now creates a JSONL stream with frozen bars/configuration,
generator/GA settings, annualization, seed, fitness rule and executable content identity.
Every observed candidate includes strategy, all serialized metrics, errors and eligibility;
a terminal completed event contains totals and ranked output. Events use write-all/sync-all,
file creation refuses overwrite, and persistence failure aborts the search. Failure status is
recorded when the sink remains usable. Metrics now implement serialization; nonfinite metrics
are explicitly documented as JSON null. A subprocess test exercises the real CLI: six rejected
candidates, complete inputs/final status, refusal to overwrite, and failed configuration without
a completion marker. Full debug workspace tests/build, seven frontend checks and whitespace
checks passed. `docs/EVOLUTION_AUDIT.md` explains use and limitations. This is optional evidence
logging, not automatic dashboard registration, tamper-proof storage, pre-execution candidate
claims or resumable GA state; an interrupted batch can contain unrecorded attempted work.

### Full candidate-stream replay for CLI evolution

`verify-evolution --audit-log` now reads a complete log, requires the original executable,
reconstructs frozen inputs and replays every evaluation in order. Strategy/metric/error/
eligibility records and terminal ranking/counts must match exactly; missing/truncated events,
trailing events and a different engine are rejected. Events stream from disk with a 64 MiB
per-event limit; no stored code is executed or log rewritten. GeneticConfig now has strict
serde serialization/deserialization. Real CLI subprocess tests verify both early rejection
and a nonempty 40-evaluation evolved population, plus modified eligibility, incomplete log,
wrong counts, extra events and wrong executable identity. Full debug workspace gate passed;
both CLI integration tests were rerun after adding the nonempty replay fixture. This proves
reproducibility, not authenticity, independent validation or resumability. CLI-log exposure
registration and recovery of interrupted evolution remain open.

### Serializable genetic generation checkpoints

Evolution now runs through `EvolutionState`, a versioned generation-boundary checkpoint with
frozen bars/instrument/account config, generator/GA/fitness/annualization, population, generation
and exact ChaCha12 RNG state. `advance` evaluates one population, notifies observers and only
then commits the next population/RNG state. Observer failure leaves the checkpoint unchanged;
completed or structurally invalid checkpoints cannot advance. Rand 0.8's current StdRng uses
ChaCha12 (verified in cached dependency source); strategy-generation helpers now accept generic
Rngs. A compatibility test generates 100 strategies from both streams and checks remaining RNG
output. A four-population test serializes/restores after every boundary and compares all future
candidate evidence, ranking and checkpoint bytes against uninterrupted execution. Invalid
annualization/settings are rejected. Full debug workspace tests/build and seven frontend
regressions passed, including existing CLI replay integration. This is an in-memory/serializable
state machine, not yet atomic on-disk job publication or UI/CLI resume. Engine identity binding,
checkpoint/evidence transactionality and interrupted-generation duplicate handling remain for
the persistence layer; candidate-level checkpoints are not implemented.

### Durable generation-boundary evolution job store

`sqx-project::evolution` persists initial jobs (engine identity, request key, frozen state) and
hash-linked generation records containing every evaluation, ranked population and next RNG/
population checkpoint together. Publication uses the existing durable content-addressed store,
then a single exclusive journal hard link and directory synchronization. OS file locks serialize
advances; expected-sequence checking rejects duplicate/lost-response retries instead of running
an extra generation. Restart reads the committed chain; orphan objects never advance its head.
Original engine identity is required, completed states cannot advance, and broken hashes/chains
are rejected. Tests cover reopening, simulated orphan-object publication interruption, exact
recovered evidence/final ranking, concurrent advances, stale requests and corruption. Full debug
workspace gate passed; targeted job tests rerun after parent-directory synchronization was added.
This is the persistence API, not a CLI/dashboard resume flow. Complete generation inputs are
currently repeated in checkpoints. Running generations have no durable pre-execution attempt
claim yet; interrupted work can be recomputed. Job backup/export, registry exposure integration,
request-key conflict semantics and candidate-level recovery remain open. Tests simulate journal
boundaries, not actual power loss or a killed process during disk writes.

### CLI start/status/resume for durable evolution

Added `start-evolution`, `resume-evolution` and `evolution-status`. Start freezes the requested
market/data/settings and returns its job identity without evaluating candidates; resume uses
only its checkpoint and original executable. An explicit expected sequence prevents repeated
CLI requests after lost responses from silently advancing an extra generation. Bounded steps
and final ranking/status are available. Registry locking now binds each request key to one
frozen job: identical retries deduplicate, changed inputs/engine are rejected. A real subprocess
test starts and resumes across separate processes, rejects stale retry, completes two
populations and retrieves a nonempty final ranking. Full debug workspace tests/build and seven
frontend regressions passed. Docs describe use and limits. Dashboard integration, durable
pre-generation attempt claims, exposure auditing, job backups and candidate-level recovery
remain open; the CLI is not yet a complete professional research workflow.

### Pre-execution evolution claims and holdout exposure

A generation claim is now durably published before evaluating its frozen population and is
reused on retry of the same checkpoint. Claims record job/sequence/generation/population size
and predecessor identity; committed generation records still publish evidence and next state
together. Claim publication shares the experiment final-commit registry lock, with consistent
registry-before-job lock order. Exposure auditing reads immutable claims without waiting for
an active generation, includes same-market intersecting observation ranges, and falls back to
legacy committed history. Dashboard audit counts now show claimed evolution generations.
A regression simulates process loss just after claim publication: no generation is committed,
but final-test data reuse is known; retry retains one logical claim and then completes. Full
debug workspace gate passed; project suites rerun with the new interruption/exposure regression.
Claims are conservative evidence of possible use, not proof every candidate finished. Physical
recomputations within an interrupted generation are not individually counted. Other stores,
plain JSONL logs, job backup portability and dashboard job controls remain separate/open.

## Verified increment: frozen evolution web API

Added start, advance, status and list routes for durable genetic jobs. Starts freeze the
resolved dataset, execution settings and generator; request keys reject changed inputs.
Advance requires the caller's observed generation sequence. A handler regression removes
live data after start and still evaluates the frozen population, verifies repeated start
identity, rejects a changed seed and malformed seed, rejects a duplicate advance, and
checks that all six evaluated candidates remain inspectable without changing the databank.
Status and latest evidence now use one locked snapshot, avoiding mixed checkpoint reads.
This is server-side integration; interactive dashboard controls are still pending. Start's
data-quality report is returned to the caller but is not yet part of the frozen job metadata.
Verification: full debug workspace tests, workspace build, JavaScript syntax checks and
all seven frontend regression scripts passed (`evolution-api-gate.log`). This does not
establish browser interaction coverage for the new routes or broker execution parity.

## Verified increment: generation-consistent delayed responses

An advance response previously reread current status after releasing the job lock while
returning evidence from its own generation. Another writer could commit in between,
producing mismatched progress and results. Responses now derive status and checkpoint
identity from the exact committed generation record returned by advance. A regression
retains the first response, commits the next generation, then verifies the delayed response
still describes the first checkpoint and cannot advance again with its stale sequence.
The CLI status command also reads status and ranking under one snapshot lock.
Verification: all three durable evolution integration tests and the full debug workspace
build/test/frontend gate passed (`evolution-response-gate.log`); after the CLI snapshot
change, its separate-process start/resume/stale-retry integration test passed again.
This closes response consistency, not dashboard interaction, job backup portability or
real-platform execution verification; those remain open.

## Increment: dashboard controls for saved evolution jobs

Builder now lists saved genetic searches with explicit generation counts and completion
state, provides a saved-evidence inspector, and advances one generation only on user action.
The action carries the observed sequence, is disabled immediately and remains disabled until
refresh creates a new button. An uncertain network outcome never triggers an automatic retry;
the message explains that execution may still be running or already committed. Jobs whose
engine executable differs are inspectable but their advance action is disabled. API rejection
remains authoritative for stale clients. The new frontend regression checks sequence binding,
uncertain-response messaging, disabled buttons and absence of automatic retry. This controls
existing jobs only; creation from dashboard inputs and user-friendly candidate tables remain
pending. Browser rendering and actual interaction for this section have not yet been inspected.
Verification: full debug workspace tests/build and all eight frontend regressions passed
(`evolution-dashboard-gate-retry.log`). The first gate attempt failed with a missing temporary
fixture directory in parallel evolution tests. Timestamp-only fixture names could collide;
an atomic per-process counter now guarantees distinct paths. The subsequent full gate passed.

## Increment: readable generation evidence

Saved evolution results now show a candidate table with trade count, net profit, profit
factor, drawdown and explicit eligible/below-criteria/invalid outcomes. Invalid candidates'
performance cells say Not valid; nonfinite/null ratios say Undefined rather than zero.
Counts cover the entire saved population; candidate order is explicitly evaluation order,
not ranking, and eligibility is not represented as selection for breeding. Error reasons
and strategy names are escaped. Full evidence and ranking remain available in collapsed
details. Both advance responses and status snapshots use the same renderer, including the
not-yet-evaluated case. A frontend regression covers these distinctions and hostile strings.
Visual browser inspection of the new section and dashboard job creation remain pending.
Verification: full debug workspace tests/build, JavaScript syntax checks and all nine
frontend regressions passed (`evolution-report-gate.log`).

## Increment: dashboard evolution creation and recovery

The saved-search panel now creates frozen jobs from the selected market, current execution
settings and generator configuration, with explicit seed/population/round/minimum-trade fields.
Creation performs no evaluation. Crossover/mutation defaults and elite retention are disclosed;
elite retention leaves at least one breeding slot in a small population. Requests persist in
localStorage before sending. Recovery first checks saved jobs by the original request key,
so a lost response can be recovered even after live inputs change. Clearing the pending
request explicitly leaves already-created jobs intact. Invalid integer inputs send nothing.
The new frontend regression simulates a successful server creation with lost response, then
changed local inputs; recovery finds the existing job without a second POST or rereading the
changed settings. Full debug workspace tests/build and ten frontend regressions passed
(`evolution-creation-gate.log`); final elite-limit adjustment was rebuilt and the creation
regression and JavaScript syntax check rerun. Actual browser interaction remains unverified.

## Verified increment: actual browser evolution lifecycle

Using the isolated synthetic-data preview on port 18090, created a six-candidate job with
seed 42, one breeding round and minimum one trade through the dashboard. Creation showed
zero generations. First advance showed six evaluated, five eligible and one below criteria.
Reloading the page and opening saved results preserved that evidence and the one-generation
checkpoint. A second advance completed the job with two saved generations and six candidates;
refresh showed Complete and a disabled advance button. No root project/databank was modified.
The browser exposed a stale card count immediately after advance. Successful advances now
refresh the job list; failures to refresh explicitly say the generation was saved. Uncertain
advance responses still never auto-repeat. The recovery regression verifies refresh occurs
only after a successful response. Narrow viewport inspection also showed compressed columns;
the candidate table now has minimum width and a keyboard-focusable horizontal scroll region.
These final UI corrections passed both evolution frontend regressions, JavaScript syntax and
workspace build. The browser lifecycle proof predates those last presentation corrections;
post-correction browser inspection remains open. Screenshot saved as evolution-browser.jpg.

## Increment: immutable evolution provenance context

Evolution job headers can now include a bounded object of provenance context, covered by the
job's content identity. Web creation stores the seed, declared timeframe, complete market
calendar definition (if supplied), data-quality report and explicit gap allowance. Metadata
inspection reads the saved context after live data have been removed. The CLI stores its seed
and explicitly records unavailable timeframe/calendar-quality evidence instead of implying a
calendar-aware audit. The frozen dataset and execution/generator state remain unchanged.
Legacy headers omit null context on serialization, preserving their original encoded shape;
metadata reports missing evidence as null. New job context participates in duplicate-key
validation, so a changed context cannot silently replace the original. Reopening, changed
context rejection, content tampering and legacy roundtrip have regression coverage. This
provenance is not authenticated broker origin, nor does a saved report certify data quality.
Verification: full debug workspace tests/build and all ten frontend regressions passed
(`evolution-context-gate.log`). Broader six-step completion remains unproven.

## Increment: visible frozen evolution assumptions

Job metadata now derives execution configuration, genetic configuration and ranking objective
from the immutable initial state. Both web advance/status responses and CLI status include
that metadata. The result inspector presents frozen market/timeframe/seed, first and last bar
timestamps, account currency/balance/leverage, costs, intrabar policy, minimum trades and
quality findings, explicitly distinguishing them from current controls. Missing legacy or
CLI quality evidence says not recorded, not clean. UTC epoch zero remains a valid timestamp;
source strings remain escaped. The frontend regression verifies frozen amounts, costs, dates,
gap disclosure and absent-evidence behavior. Full saved configuration remains inspectable in
raw evidence; the visible summary does not purport to enumerate every execution setting.
Verification: full debug workspace tests/build and all ten frontend regressions passed
(`evolution-context-display-gate.log`). Browser inspection of this latest context panel is
still pending; the six completion requirements remain open.

## Verified increment: current release gate and cross-profile evidence

The current optimized release passed 288 Rust tests across 65 reported test/doc-test
invocations, zero failures and zero ignored tests, workspace release build, JavaScript syntax
checks and all ten frontend regressions (`reliability-release-current.log`). This supersedes
the older release count; it does not imply untested features have been independently verified.
Added `ci/check-profile-parity.cjs`: run actual debug and release CLI binaries over the same
1500-bar fixture, seeds 42 and 731, 20 candidates and two breeding rounds. Both seeds produced
60 evaluations and a nonempty ranking. Frozen inputs agree except executable identity;
every subsequent candidate/error/metric/completion JSONL line is byte-identical. The checker
requires distinct executable identities and does not normalize floating-point results.
The macOS local comparison passed (`profile-parity.log`). Release CI matrix jobs now also
build the debug CLI and run this comparison on their own OS. Remote CI and Linux execution
are not verified here; neither cross-OS parity nor comprehensive strategy correctness follows
from these two fixtures. Full six-step completion remains unproven.

## Increment: portable evolution archive export

Added a locked backup snapshot containing the immutable job header, every committed generation
and persisted generation-start claims. Raw JSON strings preserve original content hashes and
floating-point spellings. Verification checks job/generation/claim content identities, schema,
sequence/predecessor chains, generation population coverage and claim correspondence to its
source checkpoint, without consulting live data or the original store. Tests cover a two-step
search, all candidate evidence, JSON roundtrip, unavailable original storage, modified bytes,
reordered/missing generations and rehashed but inconsistent claim metadata. The web API and
saved-job card expose a raw archive download. This is export plus structural verification;
restore/publication and browser download roundtrip are not yet implemented/verified here.
Hashes establish consistency, not trusted origin or independent correctness of research results.
Verification: full debug workspace tests/build and ten frontend regressions passed
(`evolution-backup-gate.log`). The interrupted-claim unit test was extended and rerun to prove
that a zero-committed-generation backup retains its one pre-execution claim.

## Increment: append-only evolution archive restore

Implemented restore with structural/hash verification before publication, request-key conflict
checks, and generation/claim byte-conflict rejection. Existing newer history is retained;
restoring an older prefix cannot roll it back. Restore shares the final-test commit registry
lock and evolution registry/job locks. Claims are published before result links, and new job
headers last, with directory synchronization. Retrying a partially staged new-job restore
reuses identical evidence. A crash during extension may temporarily make exposure checks
fail closed until restoration is completed, rather than classify used data as untouched.
Tests cover exact continuation against the original run, older backups over newer history,
request conflicts, corrupt input with no target creation, retained staging without a header,
and interrupted pre-execution claim restoration with zero committed generations but known
exposure. These are simulations, not physical power-loss tests. Dashboard restore submits raw
JSON unchanged with a 100 MiB limit and no automatic retry after uncertain response.
Full debug gate passed (`evolution-restore-gate.log`), followed by all project tests after the
final directory-sync/pending-claim assertions, workspace build and the new exact-transport
frontend test (now included in CI). Browser download/restore roundtrip remains pending.

## Verified increment: browser archive download/restore/continuation

Ran the actual browser flow against isolated synthetic-data stores on ports 18092 and 18093.
The source created a six-candidate, seed-42, one-round job and evaluated its first population.
Downloaded the archive through the page's download control, then imported that file through
the file chooser into the previously empty target store. The target showed one saved
generation and resumed to a complete two-generation search. The source was also advanced;
all job, generation and claim object filenames and bytes match across the two stores.
The downloaded archive contains exactly one generation/claim and preserves original job and
generation JSON bytes. The automatic card refresh now updates saved counts after the response.
The context display exposed a market-name binding bug (`symbol` vs serialized Instrument
`name`); corrected the binding and fixture, reran the report regression and syntax check,
and rebuilt the workspace. Screenshot evolution-restore-browser.jpg captures the completed
restore proof before that final label correction. Post-correction browser rendering is not
claimed. This verifies synthetic archive recovery, not broker parity or full programme closure.

## Increment: recomputed durable evolution evidence

Archive replay now recomputes each committed population from the frozen initial checkpoint,
comparing exact serialized candidate/error/metric evidence, ranking and next population/RNG
state against every stored generation. It requires the same engine identity, rejects empty
history as unverifiable and never advances pending claims or publishes progress. The web
route and an explicit saved-job button expose replay with the precise checked checkpoint;
the message distinguishes repeatability from independent strategy validation. Concurrent
new generations are outside the captured snapshot and are not implicitly certified.
Tests verify nonempty two-generation replay, unchanged archives, wrong-engine rejection and
rehashed changes to metrics, ranking and next-state content that pass structural checks but
fail replay. Tampered records are reserialized through the typed record to retain canonical
field ordering; an unchanged typed roundtrip is checked, avoiding order-only false positives.
Full debug workspace tests/build and eleven frontend regressions passed, followed by the
strengthened tampering test (`evolution-archive-replay-gate.log`, `evolution-replay-tamper-test.log`).
Replay begins from the archived initial checkpoint; it does not authenticate the original
source or independently certify indicator/strategy formulas. Browser replay-button interaction
and durable replay-attestation storage remain unverified/unimplemented respectively.

## Verified correction: exit-after-bars held one extra candle

A boundary regression demonstrated ExitAfterBars(1) entered at t=60000 but exited at
180000 instead of the next open at 120000. The engine counted the entry candle as zero
when deciding at close whether to queue the next-open exit. It now counts that candle as
one, with the model contract documented explicitly: after N occupied candle closes, exit
at the next executable opening price/quote. Positive N remains enforced by validation.
Regressions check N=1/2/3 exit times, prices and held-bar counts, plus tick execution at the
next candle's first quote rather than its last quote. A separate final-candle fixture ensures
that a missing next bar remains an explicit EndOfData reporting liquidation, not a fabricated
executed time-exit signal. The original regression failed before the fix. Full debug workspace
tests/build and eleven frontend checks passed (`exit-after-bars-gate.log`), with the final
engine suite rerun after the terminal-candle test. Results for strategies using ExitAfterBars
must be regenerated; the prior release-gate evidence predates this correction. No broker
parity or comprehensive execution correctness is implied by these boundary fixtures.

## Increment: evidence for close-derived exit decisions

Execution evidence now records each queued signal/time exit with trade identity, source candle
index and opening timestamp, all simultaneous triggering reasons, chosen reporting reason and
actual execution timestamp when a subsequent opening quote fills it. Existing signal-over-time
reporting precedence is retained, while the time trigger is no longer lost from the evidence.
The source timestamp explicitly labels the candle opening; it does not claim close information
was available at that instant. A queued exit at the end of data retains no executed timestamp,
separate from the synthetic EndOfData reporting liquidation. This is scoped to close-derived
signal/time requests, not a complete order/fill lifecycle ledger or every intrabar stop decision.
Regressions link simultaneous reasons to the closed trade, distinguish unfilled terminal
requests, and check the actual tick timestamp rather than the containing candle label. The
serialized execution report carries this evidence into archived backtests. A dedicated visual
per-trade explanation remains pending.
Verification: full debug workspace tests, workspace build and all eleven frontend regressions
passed (`exit-decision-gate.log`). This evidence extension has not yet had its own release or
cross-profile gate run; earlier release results must not be treated as covering later changes.

## Increment: per-trade explanation of queued exits

The backtest trade ledger now includes IDs, lot size, commission, financing and expandable
exit explanations linked by trade ID to saved execution evidence. Explanations retain all
simultaneous triggers, identify the source candle as an opening label rather than an actual
close-decision timestamp, and distinguish executed next-opening requests from unfilled final
requests and EndOfData reporting liquidation. Exact observed tick timestamps are displayed
without implying exact intrabar times for OHLC simulations. Missing legacy evidence is
separated from an explicitly empty request list. Strings are escaped and absent/nonfinite
numbers are not fabricated as zero. The regression checks competing triggers, trade-ID
isolation, epoch-zero times, delayed tick times, terminal unfilled requests and legacy absence.
This UI covers queued signal/time exits; indicator-value traces and full intrabar order/fill
explanations remain incomplete. Browser visual inspection of the new ledger is pending.
Verification: full debug workspace tests/build, JavaScript syntax and all twelve frontend
regressions passed (`trade-explanations-gate.log`). The six programme requirements remain open.

## Increment: entry-attempt evidence and corrected rejection times

Execution evidence now records evaluation of a queued entry signal at the next opening quote:
source candle index/open label, actual attempt timestamp, direction, all gate failures,
requested lots when sizing was reached, and filled lots/price/trade ID on success. Previously
silent balance, ATR-readiness, session, trading-hours, daily-cap and confirmation gates now
have explicit reason codes. Volume/margin rejections appear in the same trace. This records
attempts, not last-candle signals without an executable next quote, nor indicator operand
values; it is not yet the full requested order lifecycle.
A failing tick fixture showed rejection time 60000 instead of the actual quote at 60007;
OrderRejection now uses entry_time. Regressions link successful fills to their trades and
confirm blocked attempts have no fabricated size/fill/trade, with day-cap and trading-hours
examples. The original rejection-time fixture failed before correction. Serialized execution
evidence carries the trace into archived backtests; a user-facing blocked-entry inspector
remains pending. Existing volume/margin fields retain their prior meaning.
Verification: full debug workspace tests/build and twelve frontend regressions passed
(`entry-attempt-gate.log`). Current release/cross-profile verification still needs refreshing
after these execution changes; the professional-reliability goal is not complete.

## Increment: readable entry-attempt inspector

Backtest results now include an expandable entry-decision table with filled/blocked counts,
source candle index/open label, actual attempt time, direction, readable gate reasons,
requested/filled lots and fill price. Trade ID zero is a valid filled outcome. Missing sizing
is explicitly not reached/not recorded, not zero, and older reports without evidence are
not represented as having no blocked signals. Multiple blocking reasons remain visible and
unknown reason strings are safely escaped. The explanatory scope excludes indicator-level
condition traces, signals suppressed by an existing position and terminal signals without
an opening attempt. The frontend regression checks these distinctions, epoch-zero dates,
sub-candle attempt timestamps and trade ID zero. Browser visual inspection is pending.
Verification: full debug workspace tests/build, syntax checks and all thirteen frontend
regressions passed (`entry-explanations-gate.log`). Full professional validation is not complete.

## Verified correction: infinity could trigger a trading signal

While inspecting operand traceability, a fixture with finite OHLC inputs whose TypicalPrice
sum overflows demonstrated a true comparison against positive infinity. Signal comparisons
and cross/rising/falling/level helpers had excluded NaN but accepted infinities. Helpers now
require finite operands (including constant levels). Compiler diagnostics identify infinite
resolved operands and their first affected bar; backtests and confirmation compilation reject
such numerical evidence instead of silently scoring a strategy. Normal NaN warmup retains
its undefined/false semantics. The public checked compiler returns diagnostics; the legacy
boolean-only helper fails closed on numerical errors. This detects infinities, not every
possible NaN-producing arithmetic overflow indistinguishable from defined NaN warmup.
The original regression failed before correction. Tests cover invalid current/previous values
on either side, nonfinite levels and finite positive controls. Full debug workspace tests,
build and thirteen frontend regressions passed (`signal-overflow-gate.log`), followed by a
workspace check after exposing the checked API. Indicator-value tracing remains pending;
this correction does not itself provide independent formula validation or full completion.

## Increment: frozen primary-signal operand evidence

Compiled primary signals now retain the resolved operand series and per-condition outcomes
used during the run. Entry attempts and queued signal exits capture their source candle's
condition definitions, current operands, previous operands when crossing/rising/falling needs
them, each boolean result, join/fuzzy configuration and combined result. Evidence is attached
from the same compilation, not recomputed later against mutable data. Undefined operands are
represented as absent values, not fabricated zeroes; unused right operands in unary conditions
are also absent, with the comparator retaining that distinction. Time-only exit requests have
no fabricated signal trace. Confirmation traces and signals never reaching an attempt remain
outside this increment, as do full intrabar order explanations. Compilation temporarily retains
additional operand series proportional to bars and conditions; larger-data performance remains
to be assessed. Regressions assert crossing values come from the signal candle rather than
fill candle, previous values agree, later data perturbation preserves earlier explanations,
and an OR signal retains its unmatched undefined condition. UI operand-detail rendering is
still pending. An initial test compile referenced an unavailable test-only JSON dependency;
that redundant serialization assertion was removed before rerunning the full gate.
Verification: full debug workspace tests/build and all thirteen frontend regressions passed
(`signal-evidence-gate-retry.log`). This establishes trace consistency, not independent formula
correctness or professional-programme completion.

## Increment: visible primary-signal operand explanations

Entry attempts and queued-exit explanations now share a Signal values disclosure showing
condition definitions, indicator parameters/price shifts, current and required previous
operands, individual outcomes, combined outcome, source index and AND/OR or fuzzy aggregation.
Fuzzy threshold zero is shown explicitly, including its override of AND/OR semantics. Unary
conditions distinguish unused right operands from undefined values. Nonfinite/missing values
are never displayed as zero; older/time-only records explain missing primary traces rather
than invent them. Names and parameters are escaped. Regression coverage checks crossings,
previous values, false conditions, actual zeroes, fuzzy zero, unary conditions and legacy
absence. This is primary-signal evidence; confirmation values and intrabar order lifecycle
remain outside this view. Browser visual inspection is pending.
Verification: full debug workspace tests/build and fourteen frontend regressions passed
(`signal-explanations-gate.log`). Full programme completion remains unproven.

## Increment: closed-bucket confirmation provenance

Entry attempts now distinguish no configured confirmation from configured-but-unavailable
confirmation. Available confirmation records include timeframe, source aggregate opening,
actual calendar bucket completion time, and its compiled signal/condition values. Compilation
retains the same checked numerical behavior; mapping still selects only buckets closed by
the base execution candle opening, including conservative tick-mode mapping at that boundary.
The UI displays source/availability times and the condition trace, explicitly stating that
closed-bucket timing does not certify internal data coverage. Tests verify initial unavailable
buckets, M5 source/availability, future-perturbation stability, February's actual month boundary,
and exclusion of the truncated leading aggregate. Frontend checks distinguish missing legacy
evidence, no filter and unavailable source, while preserving epoch-zero source timestamps.
Confirmation explanations do not resolve market-calendar gaps or full intrabar event tracing.
Browser visual inspection of these latest details remains pending.
Verification: full debug workspace tests/build and fourteen frontend regressions passed
(`confirmation-trace-gate.log`); the expanded entry-rendering test and syntax check also passed.
The six completion requirements remain open.

## Verified correction: overlap was overridable as a data gap

A data-quality fixture demonstrated that increasing M5 candle timestamps at 0 and 60000
produced only an irregular-step count, with no error. The exploratory gap override could
therefore admit overlapping source candles. Overlap now produces a hard error, separately
from genuine missing intervals. Regressions retain valid adjacency and ordinary gaps; the
web creation handler rejects overlapping data both with and without allow_data_gaps and
publishes no job. The original overlap fixture failed before the fix.
The core input validator also checks any declared fixed bar_duration_ms in every mode:
positive duration, representable end timestamps and no overlap with the next candle. Tick
mode retains its requirement to declare a duration. Bar mode without a declared timeframe
cannot infer source duration unambiguously; missing interior coverage of confirmation buckets
remains unresolved and must not be claimed as certified by this change.
Verification: full debug workspace tests/build and fourteen frontend checks passed for the
quality/web correction (`overlap-gate.log`). After adding common engine duration validation,
all backtest tests and workspace build passed (`declared-overlap-tests.log`,
`declared-overlap-build.log`). Full professional completion remains unproven.

## Verified correction: source candles must fit confirmation buckets

A failing engine regression demonstrated that declared two-minute source candles
were accepted for one-minute confirmation candles. With a declared fixed source
duration, the engine now rejects any source interval ending beyond its target
confirmation bucket. This also rejects misaligned smaller candles that straddle
a boundary; aligned one-minute source candles inside five-minute buckets remain
valid. Calendar bucket bounds are used rather than nominal target durations.
Verification: the reproducer failed before the fix; full debug workspace tests,
workspace build and fourteen frontend regressions passed afterward
(`confirmation-straddle-gate.log`). Undeclared source durations and incomplete
interior coverage remain unresolved; this check does not certify those inputs.
All six professional completion requirements remain open.

## Confirmation coverage contract for declared fixed source durations

A regression reproduced acceptance of a closed five-minute confirmation bucket
missing its first one-minute source interval. The check now requires contiguous
coverage from bucket opening to calendar bucket end for each represented closed
bucket that can inform a decision. Tests remove the beginning, middle and final
source interval separately. An omitted truncated leading bucket and a still-open
terminal bucket retain their existing behavior. Failure invalidates the research
run instead of silently filling missing prices or skipping a hole in indicator
history. The scan is linear in source and aggregate candle counts.
This is a conservative continuous-coverage contract: without market-calendar
input, scheduled closures inside a bucket cannot be distinguished from missing
data and are rejected too. Whole absent buckets, stale confirmations across
closures, unknown source duration, and calendar-aware coverage remain open.
Verification: full debug workspace tests/build and fourteen frontend checks passed
(`confirmation-coverage-gate.log`). An additional leap-February fixture confirms
29-day coverage is accepted and removing February 29 is rejected; all 37 engine
integration tests then passed (`confirmation-calendar-tests.log`). Release-mode
and external platform validation for these changes remain pending. All six
completion requirements remain open.

## Fresh web research binds declared source duration

A regression showed an ordinary archived M1 web backtest reporting null source
duration, bypassing the engine's declared-duration checks. Fresh web configuration
now binds fixed source duration from SymbolSeries.timeframe across backtests,
analysis, portfolio research, generation, optimization, robustness, assistant
simulation settings, saved evolution and experiment creation. A conflicting
caller duration is rejected, and null cannot erase a known fixed duration.
Resolved archived configurations remain unchanged for engine-bound replay.
Calendar MN1 data deliberately retain no fixed duration: a nominal 30-day width
is rejected rather than treated as calendar truth. Tick is rejected as OHLC
source metadata. Monthly source-interval validation, calendar-aware confirmation
coverage, and explicit timeframe contracts for CLI/MCP remain open; no inference
from adjacent CSV timestamps was introduced.
Verification: the archived-backtest reproducer failed with null instead of 60000
before the fix. It now passes, rejects a conflicting 30000 request without an
archive, and fixed/calendar/null-binding fixtures pass. Full debug workspace
tests/build and all fourteen frontend checks passed (`source-duration-gate.log`).
The six programme requirements remain open.

## Next source-contract issue located in MCP

Inspection of resolve_bars in sqx-mcp shows that any nonempty symbol selects
QuestDB before inspecting data. CSV inputs without symbol are assigned EURUSD;
the resolved tuple also drops timeframe metadata. This prevents explicit,
unambiguous CSV instrument/timeframe contracts and bypasses the web duration
binding. Required follow-up: separate source selection from instrument metadata,
reject ambiguous source requests, retain declared timeframe through all MCP
research tools, and test CSV non-EURUSD resolution without network access.
This is an inspected open issue, not a completed correction.

## Refreshed optimized-build evidence

The complete release gate passed on the current local macOS worktree after the
execution evidence, finite-operand, exit timing, source overlap, confirmation
coverage and web source-duration changes: 310 Rust tests passed, zero failed or
ignored across 67 test invocations; workspace release build and fourteen frontend
checks passed (`reliability-release-latest.log`). Two actual CLI evolution runs
(seeds 42 and 731, 60 candidate evaluations each) produced byte-identical candidate
evidence and nonempty rankings between distinct debug and release executables
(`profile-parity-latest.log`). Only executable identity differs in their headers.
This is scoped reproducibility evidence, not independent formula correctness,
all-strategy equivalence, broker parity or Linux/remote-CI verification. The six
completion requirements remain open.

## MCP source/instrument separation corrected

MCP CSV paths now select CSV even with an explicit non-EURUSD symbol; symbol is
instrument metadata, not an implicit network switch. CSV requires both symbol and
timeframe. Optional source=csv/questdb rejects contradictory path combinations;
malformed/empty source metadata fails before I/O. Legacy QuestDB symbol-only calls
retain their documented M5 default. The resolver retains timeframe through all
four research tools, binds fixed candle duration, rejects conflicting duration,
and validates common engine inputs before candidate evaluation. Tool schemas and
README describe the stricter CSV contract. Raw CSV calls without instrument or
timeframe must migrate; silently assigning EURUSD is no longer supported.
Regressions cover pure source selection and actual local USDJPY CSV resolution,
then all four handlers reject conflicting duration without accessing QuestDB.
This does not yet supply calendar-aware MCP data-quality gating, complete trial
archives or source authentication. Monthly source duration and the CLI contract
also remain open.
Verification: full debug workspace tests/build and fourteen frontend checks passed
(`mcp-source-gate.log`). After finalizing advertised schema requirements, MCP tests
and build passed again (`mcp-source-final-tests.log`, `mcp-source-final-build.log`);
the actual stdio tools/list response exposes required symbol and conditional CSV
timeframe for all four tools. Full professional completion remains unproven.

## MCP research now audits source quality

All four MCP research handlers share a resolver that audits source candles using
the declared timeframe and current server time before simulation. Optional UTC
calendar metadata is parsed and included in evidence. Gaps require an explicit
boolean allow_data_gaps or calendar evidence of a scheduled closure; invalid,
duplicate, overlapping and incomplete candles remain fatal even with opt-in.
Successful responses append a machine-readable research_context JSON object to
the text result with full calendar, quality counts, server cutoff, gap disclosure,
symbol, timeframe, resolved configuration and exploratory scope. Quality failures
retain the report in the error text. No caller-controlled future cutoff can make
an unfinished current candle appear complete.
Tests distinguish a known Thursday session break from unknown missing data,
exercise hard-invalid data and strict metadata types, and run all four handlers
against a local gapped USDJPY CSV. A disclosed exploratory backtest returns the
exact instrument/timeframe/configuration and gap count. This improves source
validation, not complete trial persistence, broker parity, calendar authenticity
or calendar-aware confirmation aggregation. Those requirements remain open.
Verification: full debug workspace tests/build and fourteen frontend checks passed
(`mcp-quality-gate.log`), including actual handler coverage of local CSV selection,
quality rejection and disclosed evidence. Release verification of this newest MCP
increment remains pending. All six completion requirements remain open.

## Calendar coverage uses candle intervals, not only opening labels

A failing daily-data fixture exposed a false market-closure classification:
a missing Tuesday candle labeled midnight was counted as closed even though the
supplied weekday session traded from 09:00 to 17:00 UTC. Quality auditing now
checks whether any declared session intersects the full half-open candle interval.
This applies both to missing intervals and supplied candles. A supplied candle
with no session overlap is a hard error; calendars cannot explain gaps while
contradicting the data that remain. Daily/monthly opening labels may still fall
outside sessions when the represented interval contains trading hours.
Regressions cover the missing weekday, a supplied weekend candle, exact session
opening/closing boundaries, a full-day holiday override, and a month beginning on
a closed Saturday but containing open Mondays. Calendar date overrides take
precedence throughout the interval. Unrepresentable calendar intervals fail
explicitly. This establishes interval overlap, not complete within-candle tick
coverage or the authenticity/accuracy of a user-supplied market calendar.
Verification: the missing-daily-candle assertion failed before correction
(`calendar-interval-before.log`). Full debug workspace tests/build and fourteen
frontend checks passed afterward (`calendar-interval-gate.log`), including the
new interval/holiday fixtures and existing web/MCP quality gates. All six
professional completion requirements remain open.

## CLI backtest source contract

The backtest subcommand now requires an explicit --timeframe, uses its fixed
source duration for both candle and tick modes, and rejects a conflicting legacy
--tick-bar-ms. Calendar months retain no invented fixed duration. Optional
--calendar supplies the UTC quality calendar and --allow-data-gaps discloses
exploratory use; malformed/overlapping/incomplete data remain fatal. Source quality
is audited at current server time before execution. Successful output includes
Research context JSON with instrument, timeframe, full calendar, cutoff, quality,
gap disclosure, resolved configuration and execution evidence.
This is an intentional CLI migration: old backtest commands must add their source
timeframe. README's sample is updated. Other CLI research subcommands have not yet
been migrated and must not be described as covered. CSV timestamps remain seconds;
the first integration fixture mistakenly supplied milliseconds and correctly
failed as future/incomplete data, then was corrected to the documented CSV units.
The process-level regression covers required metadata, successful candle/tick
runs, conflicting legacy width, explicit gap acknowledgment and non-overridable
overlap. This change does not yet archive CLI backtest runs or certify parity.
Verification: the CLI process-level regression passes with documented seconds-based
fixtures (`cli-timeframe-tests.log`). Full debug workspace tests/build and fourteen
frontend checks passed (`cli-timeframe-gate.log`). Release verification for this
latest CLI increment remains pending. All six completion requirements remain open.

## CLI analysis shares the source and execution contract

Backtest and analyze now use a shared SourceOptions audit (declared timeframe,
optional UTC calendar, explicit exploratory gap disclosure). Analyze requires
--timeframe and supports --config with a full BacktestConfig, rather than always
silently using zero-cost/default-account execution. Supplied fixed duration must
agree with source metadata. Engine errors are rejected before analysis. The
analysis output includes the same source/configuration/execution context as a
backtest, while keeping its report format.
A process-level fixture compares both commands' resolved configuration, execution
evidence and quality report with nondefault deposit/spread/slippage/commission.
It also checks missing timeframe, conflicting duration, rejected gaps and explicit
gap disclosure. This fixture tests contract consistency, not independently verified
trade pricing (the strategy deliberately makes no trades). Existing account/cost
fixtures provide separate pricing evidence. Other CLI research commands remain
unmigrated; this increment does not close all six programme requirements.
Verification: both source-contract process tests and the full CLI test suite
passed (`cli-analysis-tests.log`, `cli-analysis-all-tests.log`); workspace build
passed (`cli-analysis-build.log`). No frontend code changed in this increment.

## CLI generation and walk-forward source/configuration binding

Generate and optimize now require explicit --symbol and --timeframe and share
ResearchOptions: source quality audit, optional calendar/gap disclosure, optional
full --config, duration binding and engine input validation before research.
Previously both commands hardcoded EURUSD and default execution costs regardless
of the CSV instrument. Their output now carries Research context with instrument,
source audit, resolved configuration and seed; generation records attempted count
and optimization includes the full walk-forward report. Existing candidate/result
printing and optional generator project export remain available. README examples
have been migrated, including M5 MT5/QuestDB CSV examples.
Process-level tests use USDJPY with a JPY account and nonzero costs, check the
reported resolved configuration for both commands, and reject missing metadata,
overlapping data despite gap opt-in, and undisclosed gaps. This verifies source
binding, not an independent profitable strategy fixture or all-trial persistence.
The initial test compared JSON floating 7.0 to integer 7; its numeric assertion
was corrected without changing the implementation. Other CLI commands and full
research provenance remain outstanding; all six completion requirements stay open.
Verification: full CLI suite passed, including both new research-command process
checks and existing backtest/analysis/evolution regressions
(`cli-research-all-tests.log`); workspace build passed (`cli-research-build.log`).
No frontend implementation changed in this increment.

## CLI stress and portfolio execution binding

Montecarlo, robustness and portfolio now use ResearchOptions with required symbol
and timeframe, optional full configuration, quality audit and disclosure. They
no longer hardcode EURUSD/default execution. Monte Carlo checks backtest errors
before resampling; a failed simulation cannot become an empty-trade stress report.
Portfolio eligibility explicitly excludes execution errors and emits indexed
invalid-candidate reasons plus context before early no-portfolio returns. Its
context states independent-account weighting, not shared margin execution.
Monte Carlo context includes seed/run count/skip probability/errors; robustness
context includes the MC/retest seeds, coverage/errors and full walk-forward report.
Process tests exercise all three commands using USDJPY/JPY configuration, reject
overlap despite opt-in, and verify invalid strategy sizing stops Monte Carlo before
any stdout report. The first test exposed a missing newline between a rendered
Monte Carlo error and Research context; output now uses a separate line. These
fixtures cover contract/error handling, not independent stress-statistic formulas.
Complete per-candidate archives and remaining CLI migrations stay open.
Verification: full CLI tests passed (`cli-stress-all-tests.log`) and workspace
build passed (`cli-stress-build.log`). No frontend code changed. All six
professional completion requirements remain open.

## CLI CSV ingestion unified with the strict shared reader

A process-level regression proved that a CLI backtest accepted a CSV volume of
"unknown" as zero. Inspection also found that the duplicate CLI parser skipped
short rows, accepted surplus columns and multiplied seconds by 1000 unchecked.
All CLI load_bars callers now use sqx_data::read_csv_bars_str with file context;
empty input retains an explicit error. This removes the divergent parser rather
than maintaining a second set of validation rules. CSV units remain Unix seconds.
The regression checks malformed volume, short/extra rows, timestamp overflow,
negative volume, NaN and duplicate timestamps: all must fail with line 3 identified
and no panic. This is syntax/value consistency across ingestion routes, not a
replacement for declared timeframe/calendar quality auditing on remaining CLI
commands. All six professional requirements remain open.
Verification: the malformed-volume fixture failed before correction
(`cli-csv-before.log`). Full CLI tests then passed (`cli-csv-tests.log`), followed
by the expanded seven-case ingestion regression (`cli-csv-expanded-tests.log`)
and workspace build (`cli-csv-build.log`). No frontend code changed.

## CSV headers are validated as schema declarations

A failing fixture demonstrated that timestamp,close,high,low,open,volume was
silently discarded as a header, after which values were interpreted in the
opposite open/close positions. Both shared bar and tick readers now validate the
exact documented column sequence instead of skipping any line starting with
"timestamp". Unit-renamed timestamp_ms and substituted fields are rejected with
an explicit line-1 diagnostic; the documented timestamp unit remains seconds.
Headerless numeric input remains supported, and canonical headers accept trimmed
field whitespace and an optional UTF-8 BOM. This is not general quoted CSV support.
Regressions cover swapped OHLC fields, renamed volume, swapped bid/ask with equal
quotes (so price validation cannot hide the header defect), renamed tick fields,
timestamp units, canonical successful decoding and retained headerless/BOM input.
The stricter shared reader is used by CLI, web and MCP ingestion. All six
professional requirements remain open.
Verification: reordered-header reproducer failed before the correction
(`csv-header-before.log`). Full debug workspace tests/build and fourteen frontend
checks passed afterward (`csv-header-gate.log`), including the newly shared CLI
parser and both header regressions. Release verification of these latest changes
remains pending.

## CLI pipeline and assistant use the research source contract

Pipeline and assistant now require explicit symbol/timeframe and accept the shared
calendar, gap-disclosure and full simulation configuration options. Pipeline no
longer hardcodes EURUSD/default costs; --seed controls the run (default 42), and
its context includes source/configuration, seed, stage evaluation counts and
pipeline errors, including on a failed pipeline report. The assistant performs
source/configuration validation before building its provider request and passes
the resolved BacktestConfig to its engine tools. Provider credentials remain
outside research context. README examples reflect the migration.
A local process fixture runs a seeded USDJPY/JPY pipeline, verifies both build
attempts and the requested seed, and rejects a subsequent gapped source. A local
nonblocking listener proves a gapped assistant request fails without making a
provider connection. This does not test a successful external LLM interaction or
archive its tool trials. Complete pipeline resumption/trial persistence and the
remaining CLI migrations remain open, as do all six completion requirements.
Verification: pipeline/provider-preflight process test and full CLI suite passed
(`cli-pipeline-tests.log`, `cli-pipeline-all-tests.log`); workspace build passed
(`cli-pipeline-build.log`). No frontend code changed in this increment.

## Cross-market CLI comparison requires an explicit manifest

Crosscheck replaces folder/filename inference and hardcoded M1/default costs with
--manifest schema 1: each market declares symbol, CSV path, timeframe and optional
calendar/config paths and gap disclosure. Relative paths resolve beside the
manifest; unknown fields, empty lists, unsupported schema, duplicate symbol/timeframe
and nonpositive trade thresholds are rejected. All sources/configurations are
prepared before any comparison evaluation. Each result retains its context and
is classified invalid, insufficient_trades or eligible. Invalid executions retain
errors without fabricated metrics, never enter profitable/eligible counts, and
cause nonzero exit status after the complete result is printed.
The process fixture covers arbitrary filenames, explicit EURUSD and USDJPY/JPY
settings, relative paths, duplicate rejection, a missing final source before any
report, and two invalid strategy executions retained with zero eligible markets.
The sample manifest intentionally contains only the bundled EURUSD data; users
must add independently declared datasets for a multi-market comparison.
This removes source ambiguity, not data authenticity or shared-account portfolio
semantics. Calendar correctness, independent validation and the remaining broader
requirements are still open.
Verification: crosscheck integration and full CLI tests passed
(`cli-crosscheck-tests.log`, `cli-crosscheck-all-tests.log`), workspace build passed
(`cli-crosscheck-build.log`), and the README sample command executed successfully
with its one market explicitly insufficient in trade count (not a robustness pass)
(`cli-crosscheck-example.log`). All six completion requirements remain open.

## Evolutionary CLI research freezes source evidence and configuration

Evolve and start-evolution now use the shared required instrument/timeframe,
calendar/gap audit and full simulation configuration. Direct evolution logs include
research_context alongside frozen bars/configuration; durable jobs persist the
same source evidence and resolved execution settings. Resume and verification
continue to use frozen inputs rather than mutable CSV/config files.
The current-time audit still rejects unfinished candles before publication. Frozen
identity records validated_through_ms (the final candle close) rather than volatile
as_of_ms, so repeated identical start requests deduplicate and debug/release logs
can retain identical semantic headers. This horizon is not claimed as wall-clock
execution time. Replay verifies computations under the matching executable, not
calendar authenticity or independent correctness of ancillary context.
Existing CLI evolution fixtures and profile-parity CSV now use documented seconds
instead of their previous milliseconds-as-seconds values, which the new quality
gate correctly rejects as unfinished future data. Required symbol/timeframe flags
are explicit in those fixtures. Tests additionally inspect frozen source metadata,
use nondefault spread, and delete/change source/config files before resume.
All six professional completion requirements remain open.
Verification: full debug CLI suite passed (`cli-evolution-source-tests.log`), then
expanded frozen-header/replay/resume regressions passed
(`cli-evolution-frozen-tests.log`). The complete release gate passed 324 Rust tests,
zero failed or ignored across 73 invocations, workspace release build and fourteen
frontend checks (`release-source-contracts.log`). After rebuilding the debug CLI,
the corrected seconds-based parity fixture passed both seeds (42/731): 120 candidate
evaluations and two nonempty rankings byte-identical across distinct executables
(`profile-parity-source-contracts.log`). This local macOS evidence does not establish
Linux parity, broker parity or independent formula correctness. All six completion
requirements remain open.

## Dashboard startup retains declared CSV timeframe

Serve no longer labels imported CSV files/folders as M1 unconditionally. A single
CSV requires explicit --symbol and --data-timeframe; directory mode requires a
shared --data-timeframe. CSV/file/folder and QuestDB selections cannot silently
shadow each other, and a CSV timeframe without a CSV source is rejected. Demo
sources remain explicitly M1. Directory mode still uses filename stems as symbol
identifiers under its documented convention; explicit per-file manifest mapping
for dashboard startup remains a separate open requirement.
A process integration test starts an isolated real server with two M5 USDJPY
candles and inspects /api/datasources over HTTP: symbol, timeframe and row count
are retained. It also checks missing metadata and conflicting CSV/QuestDB flags.
The server runs in a temporary working directory and is killed/waited on through
a guard; user datasets and running servers are untouched. README startup commands
now include the required interval/instrument metadata.
Verification: real-server source metadata integration and full CLI tests passed
(`serve-timeframe-tests.log`, `serve-timeframe-all-tests.log`); workspace build
passed (`serve-timeframe-build.log`). All six professional requirements remain open.

## Raw signal preview fails explicitly instead of fabricating FLAT

Cloud preview previously ignored enabled long/short directions, silently turned
checked-evaluation failures into false/FLAT, ignored higher-timeframe confirmation,
and silently skipped unreadable/malformed strategy files in filesystem order.
It now respects enabled directions, uses checked signal compilation, distinguishes
UNAVAILABLE plus errors from genuine no-entry FLAT, and rejects unsupported
confirmation rather than presenting an unconfirmed direction. Empty source data
are unavailable. Directory loading stages all parsed/validated strategies before
mutation and sorts paths deterministically.
CLI cloud now requires symbol/timeframe and applies source quality/calendar/gap
checks before loading strategies or printing a signal table. An unfinished last
candle is fatal even with gap disclosure. The CLI returns a failure when any
preview is unavailable and labels prices as candle closes, not executable quotes.
The library API documents its caller-validated closed-candle precondition.
Tests cover disabled direction, overflowed operands, unsupported confirmation,
empty data, atomic malformed-directory rejection and deterministic load order.
Process tests cover the actual CLI's valid preview, malformed strategy directory,
required metadata and future/incomplete candles with no emitted signal table.
This explicitly remains a raw entry preview: position state, account/order gates,
full confirmation evaluation and broker-ready signal delivery are not implemented
by this increment and must not be claimed. All six programme requirements remain open.
Verification: full debug workspace tests/build and fourteen frontend regressions
passed (`cloud-safety-gate.log`), including all new library/CLI preview cases.
Release-mode verification of this newest increment remains pending.

## Explicit per-file dashboard data manifests

Serve accepts --data-manifest with schema 1/datasets, each declaring symbol, CSV
path, timeframe and optional calendar path. Relative paths resolve beside the
manifest; arbitrary filenames and mixed timeframes no longer require filename
inference or a shared directory interval. Unknown fields, empty lists, unsupported
schema, empty symbols, duplicate symbol/timeframe and invalid source data reject
the complete staged load before server startup. Calendar metadata is retained in
SymbolSeries. This source is mutually exclusive with CSV/folder/QuestDB options.
Gaps remain inspectable in Data Manager; importing them does not grant future
research gap consent. The legacy folder convention remains available and explicitly
documented, while the new manifest provides the explicit mapping path.
The real-server regression loads M1 EURUSD and M5 GBPUSD from an arbitrarily named
fixture, checks both identities over /api/datasources, then checks the retained UTC
calendar and four missing open intervals over /api/data. Duplicate and missing
final-source cases fail without startup output. Example dashboard-data.json and
README instructions document the source manifest separately from crosscheck input.
All six programme requirements remain open.
Verification: both real-server source tests and full CLI suite passed
(`serve-manifest-tests.log`, `serve-manifest-all-tests.log`); workspace build passed
(`serve-manifest-build.log`). No dashboard JavaScript changed in this increment.

## Checked signal compilation validates definitions

The engine already validates complete strategies, but the public
compile_signal_checked API only inspected computed infinities. A failing external
API regression showed it accepted an unknown indicator as an all-false series;
negative price shifts were also silently clamped by the underlying resolver.
Checked compilation now calls the shared Signal::validate_execution_settings
before evaluation. Unknown indicators/parameters, negative shifts, nonfinite
constants and invalid periods are diagnostic errors; ordinary NaN warmup remains
valid and false. The compatibility boolean-only API still fails closed to false,
so research callers should use the checked API to preserve error explanations.
The resolver also records a diagnostic if a declared indicator lacks a registry
implementation instead of silently replacing it with NaN. This is not a claim of
complete indicator formula correctness or input dataset validation by the standalone
signal API. Tests cover four invalid definitions, compatibility behavior and a
valid SMA warmup. All six professional requirements remain open.
Verification: the unknown-indicator API fixture failed before correction
(`checked-signal-before.log`); full debug workspace tests/build and fourteen
frontend checks passed afterward (`checked-signal-gate.log`).

### Browser verification: execution explanations

Entry and exit explanations now occupy separate, left-aligned full-width table rows, with bounded explanation width on narrow screens. Exact recorded numeric values remain available in horizontally scrollable signal tables. Verified the synthetic EURUSD RSI example through the browser: seven filled attempts, seven trades, unchanged net -593.24; expanded the first entry's recorded RSI cross. Three existing explanation frontend checks and the CLI build passed. Narrow tables still require horizontal scrolling; this is not a full responsive-interface certification or broker parity test. Preview data was isolated under work/ui-proof-current.

### Invalid CLI walk-forward status and refreshed release evidence

`optimize` now returns a nonzero process status when the walk-forward report contains computation errors, after printing the full research context. Previously an invalid request such as zero windows printed validation errors but exited successfully. The new process-level regression reproduced that failure before the fix and passes after it. Insufficient trade evidence alone remains an exploratory result, not a computation failure or a validation certificate; consumers must inspect `evidence_sufficient` separately.

Current local verification: `ci/check.sh debug` and `ci/check.sh release` both passed, each with 332 Rust tests, zero failures/ignored tests, workspace build, JS syntax checks and all 14 frontend checks. Profile comparison passed for seeds 42 and 731, each with 60 candidate evaluations and nonempty rankings byte-identical between debug and release (executable identity intentionally differs). Logs: work/ux-implementation/wf-exit-before.log, wf-exit-after.log, wf-exit-debug-gate.log, wf-exit-release-gate.log, wf-exit-profile-parity.log. An earlier release run overlapped the regression's introduction and correctly failed against the pre-fix binary; the complete post-fix release run supersedes it. This is local macOS evidence, not a remote CI/Linux run or independent broker validation.

### CLI stress failures propagate to automation

Monte Carlo now exits unsuccessfully when the resampling report contains errors, retaining the research context. Combined robustness aggregates labelled errors from every what-if scenario, trade resampling, retest and walk-forward into `research_context.errors` and returns failure after emitting available reports. Previously these paths could print unavailable/invalid results while returning success. No-trade inputs and incomplete retest coverage are unavailable evidence, not a successful stress test.

Process regressions reproduced success-on-invalid before repair. Checks cover zero runs, an out-of-range skip probability, no-trade inputs, all three failing stages in a combined short-data report, and a valid repeated-entry/exit Monte Carlo run that still succeeds. Existing instrument/currency/quality checks remain active. Full `ci/check.sh debug` passed: 333 Rust tests, zero failures/ignored, workspace build and 14 frontend checks. Logs: work/ux-implementation/stress-exit-before.log, stress-exit-after.log, stress-exit-debug-gate.log. The preceding release/profile-parity evidence predates this CLI error-propagation change; no new release or remote-platform certification is claimed. Successful command status alone remains distinct from sufficient trade evidence and independent validation.

### MCP stress errors remain errors at the protocol boundary

The MCP robustness tool now aggregates stage-labelled what-if, trade Monte Carlo, retest and walk-forward errors into its research context and sets `isError: true` whenever any requested stage reports errors. Its text reports, source metadata and resolved configuration remain available. Previously unavailable distributions and invalid walk-forward reports were wrapped as ordinary successful tool results, permitting clients to mistake failed calculations for completed checks.

A regression through `call_tool` failed before repair and passes after it, asserting the protocol flag and all three failing stages on a two-bar/no-trade dataset. All four MCP unit tests and the MCP binary build passed. An actual stdio JSON-RPC `tools/call` invocation against the built binary also confirmed request-ID preservation, `isError: true`, and retained research context/errors. Logs: work/ux-implementation/mcp-stress-before.log, mcp-stress-after.log, mcp-stress-build.log, mcp-stress-transport.log. The existing valid-backtest test continues to check an ordinary successful response. This does not certify how external LLM clients interpret the evidence, nor complete AI trial accounting.

### Assistant candidate references survive later generation calls

Assistant generation previously replaced its candidate vector, although earlier tool responses and model messages still referred to its numeric indices. A later, smaller generation call could make an earlier index disappear; reusing an index could bind a previous reference to a different candidate. Generation now appends candidates, assigns session-stable indices/names, and evaluates/ranks only the newly requested batch. The existing session-wide backtest budget bounds retained candidates and includes detail requests. Deterministic generation still starts from seed 7 per batch; duplicate rule trees across batches remain possible and are not independent evidence.

A regression failed before repair and now verifies retained prior strategies, the new batch's index, and details retrieval of both old and new candidates. All assistant tests and the full debug workspace gate passed (335 Rust tests, zero failures/ignored, workspace build, JS checks and 14 frontend checks). Logs: work/ux-implementation/assistant-index-before.log, assistant-index-after.log, assistant-index-gate.log. This establishes reference stability only within the assistant instance; durable all-trial archives and cross-session identifiers remain unfinished.

### Full assistant generation outcomes before ranking truncation

Assistant instances now retain generation evidence in original candidate order before the top-five response filter: exact strategy, stable index, evaluated/invalid/not-evaluated status, reported metrics or error, seed and generation batch start. Subsequent generation calls append evidence. CLI prints the evidence with simulation context after the session returns, even on an error; the web API includes candidate_evaluations plus context and data quality on success and error responses. No credential or prompt is included in this evidence.

A regression verifies eight candidates are retained despite a five-row leaderboard, a second batch extends the evidence to ten with correct identities, and a subsequent deterministic session-validation failure or invalid generation request does not erase completed outcomes. Full debug gate passed: 336 Rust tests, zero failures/ignored, workspace build and 14 frontend checks (work/ux-implementation/assistant-candidates-gate.log). This remains session-local generation evidence, not persistent all-trial storage, details-rerun accounting, complete trade records, or a user-facing evidence viewer. Provider/network failures and restart recovery are not newly certified by this regression.

### Dashboard assistant summaries persist with source snapshots

Dashboard assistant sessions with generation outcomes now save a typed assistant_candidates research record through the immutable Archive, retaining frozen bars/instrument/timeframe/calendar, resolved research context, data quality, engine identity, all generation summaries and session completion status. Both successful and failed returned sessions archive their outcomes; storage failure becomes an explicit response error. Provider credentials, prompts and answers are not archive inputs. Existing list/download/restore handles the record. Saved-research labels identify AI generation summaries and disable unsupported replay; the replay endpoint independently rejects this type with an explicit limitation.

The holdout exposure audit recognises these records separately as archived_assistant_candidates and counts assistant records independently of backtests. It conservatively reports overlapping data as known reuse, including unsuccessful sessions. Counts are records, not independent trial counts.

Verification: a real eight-candidate assistant generation was archived for both completion statuses, restored into a fresh archive, reloaded after removal of live datasets, and compared in full. The test checks credential exclusion and explicit replay rejection. The full debug gate passed with 337 Rust tests and 14 frontend checks. A subsequent dedicated exposure regression and all project tests passed, followed by a workspace build. Logs: work/ux-implementation/assistant-archive-test.log, assistant-archive-gate.log, assistant-exposure-tests.log, assistant-archive-final-build.log. No new browser visual verification or live provider integration was performed. CLI automatic archives, crash-safe candidate checkpoints, details-rerun accounting, full execution records and deterministic summary replay remain open; archiving after a returned session cannot recover a process crash during that session.

### Assistant archive failures and receipts are visible in the interface

The assistant handler previously preferred `answer` over `error`; an archive failure accompanying a completed model response could therefore be hidden. Errors now take precedence, with any accompanying answer explicitly marked unresolved. Generation totals distinguish evaluated, invalid and not-evaluated candidates. An archive receipt shows the full run identifier, directs the user to Builder / Saved research for the backup and refreshes that list; outcomes without a receipt are explicitly unconfirmed as saved. AI-summary replay remains labelled unavailable.

A new frontend test executes the actual askAssistant handler with DOM/transport doubles and verifies simultaneous answer/error handling, all three outcome counts, missing receipts, retained receipts on failed sessions, saved-list refresh, button recovery and ordinary answers. Added to ci/check.sh (now 15 frontend tests). JS syntax, backup transport, the new test and workspace build passed. Build log: work/ux-implementation/assistant-evidence-ui-build.log. This is automated handler coverage; no new live provider or browser visual check was performed.

### CLI assistant generations receive automatic archive receipts

CLI assistant sessions now archive nonempty generation summaries under data/research in their working directory, using the same assistant_candidates record type as the dashboard. Records include frozen bars/instrument/timeframe/calendar, source quality/context, execution settings, engine executable hash, all candidate summaries and completion status. A receipt is emitted only after successful storage. Provider errors after generation still preserve completed outcomes and return failure; archive failures return failure without claiming a saved run. Prompts, answers, provider settings and credentials are excluded from the stored payload.

An end-to-end process test uses a bounded local HTTP provider fixture that requests eight generated candidates and then either answers or returns an error. It verifies the second provider request actually includes the tool response, reloads all eight candidates and their source/settings from the saved archive, checks credential/prompt/answer exclusion, and separately verifies failure when a file prevents archive directory creation. No external provider was contacted. This remains end-of-session summary storage, not recovery from a process crash during research, per-execution trial accounting, or deterministic replay.

Verification for the CLI increment: full debug gate passed with 339 Rust tests, zero failures/ignored, workspace build, JS syntax and all 15 frontend checks. Logs: work/ux-implementation/cli-assistant-archive-test.log and cli-assistant-archive-gate.log. Release-profile and external-provider behaviour were not newly verified in this increment.

### Details reruns are retained alongside AI generation summaries

Every executed assistant `details` backtest now appends a session evidence record with its tool-call number, candidate index, exact strategy, simulation context, evaluated/invalid status, reported metrics or execution error. Repeated requests remain distinct records; invalid indices rejected before execution do not fabricate trials. CLI output/archives and web responses/archives include detail_evaluations. The assistant view reports their count and explicitly states repeated evaluations are not independent evidence.

Regression coverage verifies repeated successful requests, a failed strategy execution retained with its error, and no extra execution record for an invalid index. The local-provider CLI integration now requests generation then details before a final answer or provider error, and checks restored archive details. Web archive backup/restore also checks the details record. Assistant, CLI and web test suites, workspace build, inline JS syntax and assistant UI handler checks passed. Logs: work/ux-implementation/assistant-details-tests.log and assistant-details-build.log. No new full release or browser visual verification. Records remain summaries written at session return; complete trade/order records and crash-safe checkpoints are still incomplete.

### Release gate after assistant evidence integration

Full local release gate passed: 340 Rust tests, zero failures/ignored, release workspace build, JS syntax and all 15 frontend checks. Debug/release comparison again passed for seeds 42 and 731: each has 60 evaluations and a nonempty ranking with byte-identical result traces (executable identity intentionally differs). Logs: work/ux-implementation/assistant-records-release-gate.log and assistant-records-parity.log. This refreshes local macOS evidence only. The completion audit near the top records source-inspected outstanding requirements, including the successful nonselected walk-forward candidates currently omitted from reports; none of the six requirements is declared complete by this gate.

### Walk-forward retains every in-sample candidate

Each WalkForwardWindow now exposes candidate_evaluations in original input order, with exact strategy, index, metrics (or None on execution failure), execution errors, eligibility, explicit exclusion reason and selected flag. selected_candidate_index disambiguates the original winning occurrence. Existing candidate_failures and selected_strategy remain available. Ranking still uses the original profit-factor/trade-count policy and stable ordering; identical duplicate strategies resolve to the first eligible occurrence. Reports now retain successful nonselected candidates instead of discarding them after ranking. Consumers that serialize the full report, including existing archive/replay paths, retain these fields.

Tests compare all supplied-family candidates' recorded metrics and eligibility against separate backtest computations with carried capital; assert stable winner identity with duplicate strategies; retain failed executions and successful zero-trade exclusions; verify random-search candidate counts and selection flags. Full debug gate passed with 341 Rust tests, zero failures/ignored, workspace build, JS syntax and all 15 frontend checks. Logs: work/ux-implementation/wf-candidates-tests.log and wf-candidates-gate.log. The completion-audit table is updated to reflect this specific closed reporting gap. This is not complete order/fill accounting, crash recovery, independent broker parity or a new release-profile verification.

### Explicit realized cash-settlement evidence

ExecutionEvidence now includes cash_settlements, appended at each normal/stop/time exit and end-of-data liquidation at the same point the engine updates cash. Each record identifies the trade, close timestamp/reason, quote/account currencies, gross quote PnL, applied quote-to-account conversion, gross account PnL, commission, signed financing, net change and balances before/after. This records the existing accounting convention without changing fills or PnL: commission and accumulated financing settle at close, while open-equity valuation already reserves them. It is not a broker daily-posting ledger. Spread/slippage remain embedded in executed prices. Bar timestamps do not become exact intrabar timestamps merely by appearing in a settlement; end-of-data remains a reporting convention.

Tests reconcile the sequential cash chain to final equity and each trade's PnL, including repeated long/short exits, commission, financing charges and credits, a USDJPY quote/account conversion, rejection without cash movement, gap stop-out and exact observed tick-close timestamp. This closes the absence of explicit realized settlement records, not the broader complete order/account-lifecycle requirement. Per-rollover accrual events, order amendments, mark-to-market/margin snapshots and independent broker statement correspondence remain open.

Verification: full debug gate passed with 342 Rust tests, zero failures/ignored, workspace build and all 15 frontend checks. The final expanded account assertions also passed in a targeted rerun. Logs: work/ux-implementation/cash-settlement-tests.log, cash-settlement-gate.log and cash-settlement-account-final.log. No independent broker or new release-profile validation is claimed.

### Financing accruals reconcile to realized settlement

ExecutionEvidence now records nonzero financing_accruals as the existing financing routine applies crossed UTC rollovers, before processing the next available quote. Each event includes trade ID, UTC day number, recognition timestamp, lots, signed per-lot daily rate, multiplier, charge and accrued totals before/after. It distinguishes an economic rollover day from delayed recognition after missing quotes; the engine still settles accumulated financing in cash only at liquidation. Zero-charge days do not create events. No new broker calendar, daily cash-posting convention or rate feed is implied.

Regression fixtures reconcile three crossed rollover days (including a triple Friday charge) to the trade and cash settlement, for both costs and credits. A tick fixture verifies the first quote after midnight charges once, repeated processing of the opening quote and later same-day ticks does not duplicate it, and final settlement uses the actual last tick time. This is accrual provenance for the current model, not complete order/margin state history or broker statement parity.

Verification: full debug gate passed with 344 Rust tests, zero failures/ignored, workspace build, JS syntax and all 15 frontend checks. Logs: work/ux-implementation/financing-events-tests.log and financing-events-gate.log. No new release-profile, visual-browser or independent-broker verification was performed.

### Entry margin admission is inspectable

Each entry attempt now contains an optional margin_check populated at the actual normalized-size admission test. It records cash balance, normalized lots, proposed fill price, quote/account rate, maximum leverage, required margin, spread plus entry-slippage reserve, commission reserve, equity after costs, free margin and accepted flag. The arithmetic and admission decision are unchanged. Earlier gates/invalid lot sizes leave the check null; older records may lack it. Rejected orders retain no actual fill price/trade ID even though the admission record contains a proposed price.

The Entry decisions explanation renders these values without rounding away evidence, distinguishes missing legacy evidence from an unreached calculation, and labels admission reserves as estimates rather than cash postings or ongoing margin history. Hand-computed fixtures verify accepted one-lot and rejected twenty-lot orders with spread/slippage/commission, plus invalid-volume bypass. Frontend regression checks accepted/rejected labels, exact negative free margin, missing/unreached states and the proposed-price qualification. This does not supply intratrade margin snapshots or stop-out decision provenance.

Verification: full debug gate passed with 345 Rust tests, zero failures/ignored, workspace build and all 15 frontend checks. Logs: work/ux-implementation/margin-evidence-tests.log and margin-evidence-gate.log. No new browser visual or release-profile validation was performed.

### Executed stop-outs carry margin valuation evidence

ExecutionEvidence now includes stop_out_decisions for actual StopOut exits. Records contain trade/time identity, pre-slippage liquidation price and final fill, calculated stop-out threshold, conversion rate, cash balance, unrealized account PnL, reserved commission, accrued financing, equity before slippage, required margin, stop-out ratio and minimum equity. Evidence is appended only for the chosen executed exit, not hypothetical OHLC-path alternatives. At a continuous OHLC threshold the valuation is modelled; gap/tick prices can be beyond the threshold. Existing bar timestamp limitations remain, and execution slippage is shown separately rather than changing the trigger valuation.

Hand-calculated tests cover long continuous/gap liquidation with commission/slippage, short USDJPY reciprocal conversion with constant USD notional margin, and financing-induced stop-out at the first observed post-rollover tick. They reconcile threshold equality or adverse gap inequality and link financing recognition, trade execution and settlement time. This explains executed stop-outs; it is not every intermediate margin check, a complete order lifecycle or independently verified broker behaviour.

Verification: full debug gate passed with 346 Rust tests, zero failures/ignored, workspace build, JS syntax and all 15 frontend checks. Logs: work/ux-implementation/stopout-evidence-tests.log and stopout-evidence-gate.log. No new release-profile or independent broker verification was performed.

### Effective protective-stop changes follow the chosen execution path

ExecutionEvidence now retains stop_changes for effective trailing/break-even stop changes while a position is live. Events contain trade ID, reached price, old/new finite stop (or explicit absence), old/new reason, source and ordering information. Tick events use observed timestamps. OHLC events identify high-first/low-first and the path step under the candle's timestamp; no precise intrabar clock is invented. Both alternative OHLC paths are evaluated into separate temporary event lists; only the selected path is appended. Price observation at an exit still updates existing excursion statistics, but does not produce a post-close stop-amendment event.

Regression coverage checks the original adverse-first stop fixture has no leaked high-first amendment, chosen high-first trailing/break-even paths each retain their actual amendment, and long/short tick paths preserve amendment time while ignoring favorable quotes after closure. This records effective protective-stop state transitions, not initial order submission, broker acknowledgements/cancellations or all margin snapshots.

Verification: full debug gate passed with 348 Rust tests, zero failures/ignored, workspace build, JS syntax and all 15 frontend checks. Logs: work/ux-implementation/stop-changes-tests.log and stop-changes-gate.log. No new independent broker, browser visual or release-profile verification.

### Initial protection joins entry evidence to later stop changes

Filled EntryAttempt records now include initial_protection with finite installed stop-loss/profit-target levels, trailing/break-even flags, dynamic distance when applicable, and the last completed candle index/open timestamp used for ATR resolution. Rejected attempts leave the protection record null; a filled position without configured protective levels has a record with absent levels, not fabricated zero prices. The position itself uses the same resolved flags and levels; no fill/exit arithmetic changes.

Tests check long/short point-based levels, dynamic distance, rejected entries with no installed protection, unconfigured exits, and ATR-level provenance/independence from the entry candle's future extremes. This provides the initial state needed to interpret later effective-stop changes. It is model-state evidence, not broker acceptance/cancellation messages or a complete order event stream.

Verification: full debug gate passed with 349 Rust tests, zero failures/ignored, workspace build, JS syntax and all 15 frontend checks. Log: work/ux-implementation/initial-protection-gate.log. No new release-profile or independent broker verification.

### Sampled account valuations expose equity and margin components

ExecutionEvidence now contains account_snapshots after each evaluated candle, with balance, gross unrealized account PnL, commission reserve, accrued financing, equity, required margin and free margin. Open positions also identify trade, liquidation-side price and conversion rate. Candle labels and actual last-tick timestamps are separate. Forced final liquidation appends a distinct terminal_liquidation flat snapshot, retaining the prior open-position mark; the existing equity curve still ends at settled cash. Valuation arithmetic/order is preserved to avoid changing research results.

Hand-calculated fixtures check open equity versus final cash with entry/exit slippage and one commission, reciprocal USDJPY conversion/margin, short ask-side marking with financing, and exact tick valuation timestamps. Every flat terminal snapshot releases margin/reserves and equals final equity. These are sampled valuations, not every intrabar margin calculation or a broker posting ledger.

Verification: full debug gate passed with 350 Rust tests, zero failures/ignored, workspace build and all 15 frontend checks; a subsequently added short-side snapshot case passed with the complete targeted account suite. Logs: work/ux-implementation/account-snapshots-tests.log, account-snapshots-gate.log and account-snapshots-final-tests.log. No new release-profile, visual-browser or independent broker validation.

### Final queued entry signals remain visible without invented fills

ExecutionEvidence now includes unexecuted_entry when an entry remains queued after the final signal candle. It preserves direction, candle index/open label, signal operand evidence and no_following_opening_quote reason, separately from entry_attempts. No sizing, admission or fill is fabricated. The previous early return for fewer than two bars is narrowed to empty data so a single valid candle can produce boundary evidence while never opening a trade without a following quote. Signals suppressed while a position is open remain outside this entry-attempt trace.

The Entry decisions view now shows final queued signals separately even when there were zero executable attempts, and distinguishes absent legacy evidence from an explicit null boundary entry. Tests cover long/short terminal signals on one/two candles, unchanged cash/no trades/no attempts, signal matching, no-signal absence and frontend epoch-zero/missing/null states. This closes a terminal entry visibility gap, not a full broker order-submission lifecycle.

Verification: full debug gate passed with 352 Rust tests, zero failures/ignored, workspace build, JS syntax and all 15 frontend checks. Logs: work/ux-implementation/terminal-entry-tests.log and terminal-entry-gate.log. No new browser visual, release-profile or independent broker verification.


## Verified increment: strict account evidence reconciliation

New engine results declare account-evidence schema 1 and run the shared validator
before returning a valid result. It checks trade/settlement identity, cash chains,
currency conversion arithmetic, cost/net reconciliation, financing rollover
ordering and totals, finite sampled account components, equity-curve association
and terminal cash. The metrics layer also rejects inconsistent schema-1 records.
Schema 0 preserves legacy/aggregate metrics compatibility but fails the explicit
strict evidence check; it is not evidence of reconciled execution. Floating-point
comparisons use relative tolerance 1e-10 with a unit floor.

Verification: the account mutation regression rejects 16 missing, corrupted or
nonfinite evidence variants; the metrics regression rejects missing/inconsistent
schema-1 samples and accepts the corresponding consistent flat account. Full
local debug gate passed 354 Rust tests (zero failed/ignored), workspace build,
JavaScript syntax and all 15 frontend checks. Log:
`work/ux-implementation/account-evidence-validator-gate.log`.

Limits: this validates internal recorded arithmetic, not independently sourced
quotes, every snapshot against original market data, or every initial-protection,
stop-change and stop-out field. No new release/multi-platform run or independent
broker correspondence was established by this increment. All six programme
requirements remain open.

## Verified increment: account observation sequence integrity

The strict account validator now requires ordered, strictly increasing candle
observations, followed by at most one terminal liquidation observation. A terminal
observation must follow an open position and retain the preceding candle and quote
timestamps. This closes a gap where phase filtering hid duplicate or prematurely
inserted liquidation records despite arithmetic reconciliation.

The new regression covers duplicate terminal observations, reordered phases,
altered terminal timestamps and duplicated candle timestamps (including a matching
alteration to the equity curve). It failed against the previous validator (exit
101), then passed with the fix. Backtest and metrics suites passed all 93 tests.
Logs: `work/ux-implementation/account-sequence-before.log` and
`work/ux-implementation/account-sequence-tests.log`. No new full-workspace or
release gate was run for this increment. Independent execution parity and full
order-event reconciliation remain open.


## Verified increment: entry-to-trade reconciliation

Schema-1 validation now requires a one-to-one link between closed trades and
accepted entry attempts. Direction, opening time, filled price and volume must
match the trade; margin admission must be accepted with matching proposed price
and normalized volume, and an initial protection record must exist. Rejected
attempts must carry rejection reasons, no execution/protection fields, and no
accepted margin admission. Signal candle labels must precede attempt timestamps.

A regression corrupts ten entry evidence cases, including missing and duplicate
links, a blocked filled entry, mismatched size/price/time, a signal timestamp at
execution, rejected margin, missing protection and removal of the trade identity.
All are rejected. Full local debug gate passed 356 Rust tests, workspace build,
JavaScript syntax and 15 frontend regressions. Log:
`work/ux-implementation/entry-reconciliation-gate.log`.

This checks correspondence of recorded decisions and fills. It does not yet
independently recompute all admission/protection values or prove broker parity.
The full six-part programme remains incomplete.


## Verified increment: margin admission arithmetic

The strict evidence validator now checks finite positive size, price, conversion
and leverage inputs; finite nonnegative margin and costs; equity after entry
costs and free-margin arithmetic; and agreement between the admission decision
and available equity. Both filled and rejected attempts undergo these checks.
The regression uses a one-lot accepted order and a twenty-lot rejected order,
with seven corruptions each. The contradictory-decision case preserves derived
free margin, so arithmetic consistency alone cannot make it pass.

Backtest and metrics suites passed 95 tests with no failures. Log:
`work/ux-implementation/margin-reconciliation-tests.log`. No new workspace or
release gate was run for this increment. The validator still does not recompute
required margin from independently archived instrument specifications or verify
all admission inputs against original market data; broker parity remains unproven.


## Verified increment: scheduled exit correspondence

Schema-1 validation now links scheduled time/signal exits to their trade, recorded
decision candle and actual closing time/reason. It rejects duplicate decisions,
unsupported reasons or precedence, missing decisions for scheduled trade exits,
and unexecuted requests outside the final dataset candle. A terminal request
remains distinct from its reporting-only EndOfData liquidation.

The regression covers both executed and terminal unexecuted requests, corrupting
decision duplication, candle timestamp, chosen reason and execution time; removal
of the executed decision is also rejected. Backtest and metrics suites passed
96 tests, zero failures. Log:
`work/ux-implementation/exit-reconciliation-tests.log`. No workspace/release gate
was rerun in this increment. Full protection/amendment reconciliation and
independently sourced execution comparison remain outstanding.


## Verified increment: stop-out decision reconciliation

Schema-1 validation now requires exactly one stop-out decision for each StopOut
trade, with matching time, fill, costs and pre-settlement balance. It checks
finite valuation/margin inputs, equity components, required minimum equity and
that the recorded equity reaches or falls below the stop-out threshold (allowing
the existing numeric reconciliation tolerance). Eight corruption cases cover
missing/duplicate decisions, timestamps, prices, balance, threshold arithmetic,
nonfinite conversion and an internally consistent valuation above the threshold.

Backtest and metrics suites passed 97 tests, zero failures; this includes existing
continuous/gap, short/base-currency and tick stop-out scenarios. Log:
`work/ux-implementation/stopout-reconciliation-tests.log`. No new full workspace
or release gate was run. Market-price, instrument-specification and analytic
threshold-price recomputation remain separate outstanding verification work.

## Verified increment: effective stop amendment chain

Schema-1 validation follows each trade's initial stop through recorded effective
amendments. Previous stop and reason must match the preceding state; timestamps
must remain within the trade and nondecreasing; prices must be finite; amendments
must not weaken protection for the position direction. Trailing/break-even changes
require the corresponding initial feature to be enabled, and observation source
and OHLC path-step shape must be recognized.

The new regression corrupts seven amendment properties. Backtest and metrics
suites passed 98 tests, including existing long/short tick and OHLC scenarios.
Log: `work/ux-implementation/stop-chain-tests.log`. This checks recorded chain
consistency, not an independent replay of every observed quote or proof that no
amendment was omitted. Full source-data and protection recomputation remains open.

The broader gate exposed an error in the earlier scheduled-exit validator:
walk-forward source candle indices include warmup, while the equity curve starts
at the trading boundary. Comparing these indices incorrectly invalidated valid
out-of-sample results. Exit validation now matches candle timestamps in the
ordered equity curve and checks terminal requests against its final timestamp.
It does not claim to verify source indices without the source dataset. Existing
walk-forward causal-family/carried-capital tests demonstrated the failure in
`work/ux-implementation/stop-chain-gate.log`; the corrected full gate is recorded
separately below.

Corrected full debug gate: 360 Rust tests, zero failures, workspace build,
JavaScript syntax and all 15 frontend regressions passed. Log:
`work/ux-implementation/stop-chain-gate-fixed.log`. This supersedes the failed
first gate, while preserving its evidence of the warmup-index regression.


## Verified increment: current release gate and execution profile parity

Full local release gate passed 360 Rust tests, workspace release build, JavaScript
syntax and all 15 frontend regressions. This covers the recent schema-1 evidence
validators, including corrected walk-forward timestamp matching. Log:
`work/ux-implementation/account-evidence-release-gate.log`.

The profile parity script now also runs long and short OHLC scenarios with spread,
slippage, commission, stop/target and dynamic protection configured, comparing
the entire serialized execution evidence without rounding or deleting any of its
fields. It requires nonempty cash settlements and account/entry observations.
Both scenarios matched byte-for-byte between debug and release. Existing seeds
42 and 731 also matched all 60 candidate evaluations and nonempty rankings each.
Log: `work/ux-implementation/account-evidence-parity.log`. Source-audit wall-clock
metadata is outside the execution comparison; candidate headers exclude only
the necessarily different executable identity as before.

This is local profile determinism evidence on the current host, not independently
sourced broker parity, universal indicator/execution coverage, or remote matrix
execution. All six completion requirements remain open.


## Verified increment: supplied-tick profile determinism

The debug/release parity gate now covers both long and short execution with
6,000 bid/ask ticks across 1,500 candles, in addition to the OHLC cases. It
requires supplied-tick mode, exactly the provided tick count, no missing tick
intervals, nonempty settlements and timestamped account observations. Every
serialized execution field is compared without rounding or normalization.

All four execution cases passed; the two existing evolution seeds also passed
with 60 evaluations and nonempty rankings each. Log:
`work/ux-implementation/tick-profile-parity.log`. Only the parity gate and its
documentation changed; the previously validated debug/release executables were
used. No new Rust build was required. The existing remote workflow invokes this
same script, but no remote run is claimed here.

Four ticks per candle are a deterministic synthetic sequence, not a claim of
complete real-market tick coverage or independent broker execution parity.


## Verified increment: optional full CLI backtest archives

`backtest --archive-dir` now persists full parsed bar/tick input, instrument,
strategy, source context, settings, annualization, executable identity and the
complete engine result through the immutable archive. Storage precedes execution
error propagation; a receipt appears only after successful archive publication.
Input parsing/source-audit failures still occur before this archive boundary.
Dashboard records are explicitly labelled CLI execution records and replay is
disabled/rejected rather than interpreted as the dashboard's different format.

The CLI source integration suite passed both tests, including loading the saved
archive and checking tick/bar input, strategy and schema-1 result, plus a blocked
storage path returning nonzero without a receipt. Log:
`work/ux-implementation/cli-backtest-archive-tests.log`. Frontend syntax passed.
No browser visual check or full gate was run for this increment. Automatic
recording, crash-time checkpoints, CLI replay and durable coverage of every
research command remain open; this explicit option is not their replacement.


## Verified increment: frozen CLI backtest replay

`replay-backtest --archive-dir --run-id` checks archive integrity, CLI record kind
and exact executable identity, then runs exclusively on the saved strategy,
instrument, candles/ticks and execution configuration. It compares the full engine
result without rounding. Mismatches return nonzero; successful comparison reports
execution validity separately (reproducing a recorded invalid result is not a
valid backtest). Receipts now advertise this CLI replay command; dashboard replay
remains explicitly unsupported for the different record format.

Both CLI source integration tests passed. The archive/replay fixture now includes
an actual closed tick trade, replays after deleting the source tick file, rejects
a content-valid archive with altered final equity, rejects a different engine
identity, and retains the failed-storage test. Log:
`work/ux-implementation/cli-backtest-replay-tests.log`. No full workspace/release
gate was run in this increment. Automatic recording and crash-safe checkpoints
remain open; deterministic replay is not independent execution correctness.


## Verified increment: explicit CLI holdout exposure disclosure

The generic archive scan already counted CLI backtest records as backtests. It
now identifies their overlap source as `archived_cli_backtest` and reports the
CLI subset count explicitly, without treating it as additional independent
trials. The scope text now includes archived CLI records and states that only
the current archive is inspected. Failures remain conservative prior exposure.

All 22 experiment integration tests passed, including two CLI records (successful
and failed), overlap identity and counts, unrelated-market exclusion and a later
nonoverlapping period. Log: `work/ux-implementation/cli-exposure-tests.log`.
Records saved elsewhere, unarchived commands and external data viewing remain
unobservable; no known overlap is still not proof of an untouched holdout.

The full local debug gate also passed: 361 Rust tests, workspace build, JavaScript
syntax and 15 frontend regressions. This covers the recent CLI archive/replay and
exposure changes together. Log: `work/ux-implementation/cli-research-gate.log`.
The earlier release/parity evidence predates these CLI changes.

## Verified increment: preserve MT5 exported price precision

Inspection found the MT5 bar/tick exporter formatted every price to five decimal
places, altering higher-precision source quotes before import. It now writes 17
significant digits, preserving the binary64 price values returned by the client.
An offline client-stub regression verifies exact float round trips for all OHLC
fields and bid/ask using prices beyond five decimals. It failed against the old
formatting (exit 1) and passes after repair. The local/CI gate now runs the Python
exporter tests alongside Rust and frontend checks.

Logs: `work/ux-implementation/mt5-export-precision-before.log` and
`work/ux-implementation/mt5-export-precision-tests.log`. No real MT5 runtime was
contacted. This does not fix the separate second-resolution tick timestamp format
or establish completeness of exported bars/ticks. A bounded repository search
found the exporter but no independent broker execution report; the user has been
asked for the local location of a suitable report and associated strategy/data.
Other programme work remains possible while that input is pending.


## Verified increment: millisecond tick export/import

The importer now accepts the explicit `timestamp_ms,bid,ask` schema without
rescaling its integer timestamps. Existing `timestamp`/headerless CSV remains
seconds. Reordered quote columns and unsupported timestamp units are rejected;
chronology checks operate on retained milliseconds. The MT5 exporter writes the
MT5 tick tuple's time_msc field and requests data using a UTC-aware datetime.
The layout and UTC requirement were checked against official documentation:
https://www.mql5.com/en/docs/python_metatrader5/mt5copyticksfrom_py

Data tests cover two different millisecond timestamps within the same second and
reversed chronology. The offline MT5 fixture verifies millisecond output and
price precision. Both CLI source integration tests passed with millisecond tick
input, checking the exact archived tick and actual trade opening timestamp and
then replaying the saved result. Logs: `work/ux-implementation/tick-ms-tests.log`,
`work/ux-implementation/mt5-ms-tests.log`, and
`work/ux-implementation/cli-tick-ms-tests.log`. No live MT5 transport or full
workspace/release gate was run in this increment. Existing truncated files are
not repaired; they require re-export from the original tick source.

## Verified increment: atomic MT5 CSV publication

The exporter now writes each CSV to a temporary file beside its destination,
flushes and fsyncs it, then atomically replaces the destination only after all
rows have been formatted. Exceptions remove the temporary file and preserve the
previous dataset. The client is shut down in a finally block, including export
failures. Publication is per file, not a transaction spanning several symbols.

Both offline exporter tests passed. The new failure regression writes one valid
tick then encounters a malformed tick missing time_msc, verifies the original
file remains byte-identical, checks temporary-file cleanup and client shutdown,
and separately injects an atomic replacement failure. Existing price/timestamp
precision checks still pass. Log: `work/ux-implementation/mt5-atomic-tests.log`.
No live MT5 transport, power-loss durability experiment, or full workspace gate
was run. This does not establish completeness/quality of the fetched market data.

## Verified increment: completed-candle export and partial failure status

The MT5 bar request now starts at position 1, excluding position 0 (the current
candle according to the official API contract). Requests with no symbols or a
nonpositive count are rejected before connecting. Unselectable symbols and empty
responses now mark the operation as a partial export and return nonzero, even if
another symbol succeeded. Existing successful per-symbol files remain available;
this is explicitly not an all-symbol transaction.

Three offline exporter tests pass. The precision fixture asserts the requested
bar offset; the partial-export fixture covers empty and unselectable symbols for
both bar and tick modes while retaining the successful symbol's file. The atomic
failure and precision checks continue to pass. Log:
`work/ux-implementation/mt5-completion-tests.log`. API source:
https://www.mql5.com/en/docs/python_metatrader5/mt5copyratesfrompos_py

No live terminal validation was performed. Available history may still be shorter
than the requested count; printed counts describe actual received rows and do not
certify complete historical coverage. Independent source/data audits remain needed.

## Verified increment: MT5 row validation before publication

The exporter now validates finite positive prices, bid/ask ordering, OHLC bounds,
nonnegative integral volume, integral signed-64-bit timestamps, increasing unique
bar timestamps and nondecreasing tick timestamps. Tick seconds and milliseconds
must agree. Equal tick timestamps are retained in source order rather than
silently deduplicated. Validation runs while writing the temporary file, so a bad
later row cannot replace the prior complete file.

All four offline exporter tests passed. Eight invalid datasets cover duplicate
candles, inconsistent OHLC, NaN, negative volume, reversed tick chronology,
crossed quotes, infinity and inconsistent time fields; each leaves the original
file unchanged. Log: `work/ux-implementation/mt5-quality-tests.log`. Existing
precision, completed-candle request, partial-failure and atomic publication tests
remain green. These are structural row checks, not calendar-aware gap/completeness
or independent broker validation. No full workspace gate was run this increment.

## Verified increment: integrated data/CLI debug and release gates

Both current full gates passed 361 Rust tests, workspace build, JavaScript syntax,
15 frontend regressions and four offline exporter tests. This covers CLI full
archive/replay, explicit exposure reporting, millisecond tick import and MT5
precision/atomic/quality/completion changes together. Logs:
`work/ux-implementation/data-cli-debug-gate.log` and
`work/ux-implementation/data-cli-release-gate.log`.

The profile parity gate now supplies explicit millisecond tick CSV with a 123ms
component and asserts that component survives in executed entries and account
quote timestamps. Both long/short tick cases and both OHLC cases matched all
serialized execution evidence; seeds 42 and 731 matched 60 candidate evaluations
and nonempty rankings each. Log: `work/ux-implementation/data-cli-parity.log`.
These are current-host results, not remote CI or independent broker verification.

## Verified increment: pre-execution CLI checkpoints and restart

Explicitly archived CLI backtests now publish inputs/settings/engine identity as
`cli_backtest_started` before simulation, flush a checkpoint receipt, and link the
final immutable result to that checkpoint. `replay-backtest --resume` requires the
checkpoint kind and matching executable, computes from frozen inputs, then saves
the complete result. Repeated restarts converge on the same result identity;
there is no intermediate-candle continuation. Dashboard replay remains disabled
for both CLI record kinds. Checkpoints are conservatively included in same-archive
holdout exposure even without a completed result; record counts are not trials.

Both CLI integration tests and all 22 experiment tests passed, plus frontend
syntax. The CLI test restores only the input checkpoint into an empty archive,
removes the source tick file, restarts twice and obtains the original final result
ID with only two archive records. The exposure fixture includes an incomplete
checkpoint alongside successful/failed result records. Logs:
`work/ux-implementation/cli-checkpoint-tests.log` and
`work/ux-implementation/cli-checkpoint-exposure-tests.log`.

No forced process-kill experiment or full workspace/release gate was run in this
increment. Crashes before checkpoint publication leave no resumable record; input
parsing/source audit happen before publication. Automatic archival of unflagged
CLI commands and checkpoint coverage for other research entry points remain open.

## Verified increment: discoverable CLI recovery checkpoints

`list-backtests --archive-dir` enumerates immutable CLI input checkpoints and
links completed records after validating dataset, strategy, engine and full
request identity. It reports result presence, recorded execution validity and
whether this executable matches. It does not run simulations or guess process
liveness: `result_not_recorded` can mean active, interrupted or failed work.
Legacy result records without checkpoints are outside this checkpoint listing.

Both CLI source integration tests passed. The recovery test discovers the pending
checkpoint in a fresh archive, sees the recorded result after each repeated
restart, and rejects a content-valid result that claims the checkpoint while
changing its configuration. Log:
`work/ux-implementation/cli-checkpoint-list-tests.log`. No full workspace/release
gate was run in this increment. Actual forced-kill validation, automatic archival
and equivalent recovery across other research entry points remain open.

## Verified increment: actual process interruption recovery

A subprocess regression starts a 100,000-candle CLI backtest with archival,
waits for the flushed durable checkpoint receipt, verifies the child is still
alive, kills it and observes non-successful termination. It then confirms that
only the checkpoint exists (no final result), deletes both source CSV and strategy
JSON, resumes from the archived inputs and obtains one closed trade and the full
100,000-point equity series without execution errors. A subsequent full replay
matches the saved result. Process and temporary-directory guards clean up failures.

The debug integration test passed in 15.41 seconds. Log:
`work/ux-implementation/cli-crash-recovery-tests.log`. This is actual process-kill
recovery after checkpoint publication, not a power-loss test or intermediate-bar
resume. No release or full workspace gate was run in this increment. The test is
part of the workspace suite for future debug/release and CI runs. Automatic
archival and other research entry points remain outstanding.

## Verified increment: readable per-trade cash settlement

The trade explanation now includes an expandable balance calculation: opening
balance, gross quote-currency profit, conversion rate, gross account-currency
profit, commission, signed financing, net balance change and closing balance.
It uses recorded values at full JS numeric precision, explains financing credits
and warns against charging spread/slippage twice. Legacy absence is distinguished
from missing or duplicate records; ambiguous records are not silently selected.

The existing trade-explanation regression passes alongside new cash checks for
trade-ID filtering, currency conversion precision, zero commission, negative
financing, absent/duplicate evidence, nonfinite values and HTML escaping. JavaScript
syntax passes. No rendered-browser check or full workspace gate was run for this
UI increment. This exposes settlement evidence; it does not certify independent
broker correspondence or complete the account/order evidence viewer.

## Verified increment: rendered cash settlement explanation

Built the current CLI and launched a separate demo workspace at
`work/cash-ui-proof` (loopback port 18095), without using the user's data directory.
Browser interaction ran the default RSI example, opened its seven-trade ledger,
then expanded trade 0 and its cash settlement. The visible ledger showed opening
balance 10000, gross/net change 6.763617012781609, zero commission/financing and
closing balance 10006.76361701278, with the model limitations displayed.

The explanation was inspected at the normal desktop viewport and at 390x844.
Text wrapped and the complete balance calculation was readable on the narrow
screen; the trade table retains its separate horizontal-scroll behavior. A mobile
screenshot was saved to the task outputs as `cash-settlement-mobile.jpg` and the
viewport override was reset. This verifies this rendered example, not all browser,
accessibility or currency scenarios. No source code changed in this increment.

## Verified increment: per-trade financing accrual explanations

Trade details now expose recorded financing accruals: rollover UTC day, recognition
quote time, lots, daily per-lot charge, multiplier, individual signed charge and
running accrued totals. The explanation distinguishes delayed recognition after
data gaps from the rollover day and cash settlement at close. Empty recorded
accruals are distinct from absent legacy evidence; events are filtered by trade ID.

The trade/cash/financing frontend regression and JavaScript syntax pass. New cases
cover epoch day zero, millisecond recognition time, a triple multiplier, credits,
zero opening accrued total, escaped currency and missing evidence. No rendered
browser or full workspace check was run in this increment. This is an evidence
viewer, not independent verification of broker financing schedules or amounts.

## Verified increment: readable stop-out calculations

Trade explanations now expose the recorded stop-out account calculation: cash,
unrealized profit, commission/financing reserves, equity, required margin, stop-out
ratio and minimum equity, together with calculated threshold, pre-slippage
liquidation price, fill price and conversion rate. The UI distinguishes modelled
OHLC threshold execution from gap/tick prices and does not describe candle labels
as exact intrabar times. Missing legacy evidence and contradictory exit/record
combinations are explicit; unrelated trade records are excluded.

JavaScript syntax and all trade-explanation assertions passed, including new
threshold/quote/fill distinctions, epoch timestamp zero, filtering and missing or
contradictory evidence. No browser visual check or workspace gate was run in this
increment. This explains model evidence, not independent broker margin behaviour.

## Verified increment: readable effective stop history

Trade explanations now show the recorded effective stop amendments with prior/new
levels and reasons, reached price, timestamp and source. Assumed high-first and
low-first OHLC paths are explicitly labelled modelled with their path step;
observed ticks remain distinct. Candle timestamps are not presented as exact
intrabar times. Events remain in their recorded order and are filtered by trade.
Missing legacy evidence, an empty event list and an absent finite stop are distinct.

JavaScript syntax and the combined trade-explanation regression passed. New
assertions cover two amendments, OHLC versus tick source, millisecond time,
prior/new levels, unrelated-trade exclusion, absent/empty evidence and escaping.
No browser visual or workspace gate was run in this increment. This is the
selected path's effective changes, not every internal observation or a complete
broker order lifecycle; initial protection and sampled account views remain open.

## Verified increment: initial protection in entry explanations

Entry details now show installed stop loss and profit target, trailing/break-even
flags, active dynamic distance and the completed reference candle. The UI
separates absent legacy evidence, an unfilled entry without protection, a filled
entry missing its record and protection erroneously attached without a trade.
Null levels are not rendered as zero-price orders. Links in the explanatory text
point users conceptually to later effective changes under the trade explanation.

JavaScript syntax and both entry/trade explanation regressions pass. New assertions
cover a finite stop with absent target, enabled/disabled flags, epoch reference
candle and rejected/contradictory records. No rendered browser or workspace gate
was run this increment. Installed model levels do not prove broker acceptance;
sampled account observations still need a dedicated interface.

## Verified increment: paged account observation viewer

Backtest results now include a separate account-observation disclosure with
25-sample pages. Each row exposes candle label, phase/position, balance, equity,
required margin and free margin; expandable calculations show actual tick time
when present, liquidation price, conversion rate, open profit and cost reserves.
The text distinguishes sampled candle observations and final liquidation from a
complete intrabar account history. Pagination has boundary-disabled buttons, a
live range description and focus restoration after page changes. Unknown legacy
records and empty evidence remain distinct; trade zero and flat state are separate.

JavaScript syntax and trade-explanation tests passed. Pure-renderer assertions
cover a 26-record dataset split 25/1, page-index clamping, terminal flat sample,
null valuation fields, millisecond quote time and missing evidence. The event
handler and rendered browser pagination still require browser verification; no
workspace gate was run in this increment. This exposes sampled evidence and does
not establish correctness of every intermediate market/account state.

## Verified increment: rendered account pagination

Built the current app and ran the synthetic RSI example in isolated
`work/account-ui-proof` on loopback port 18096. Browser checks opened 2,000 account
samples, moved from observations 1–25 to 26–50, confirmed keyboard focus remained
on Next, returned to 1–25 and confirmed Previous was disabled. Observation 18
expanded to show the open trade's valuation, rate and cost reserves.

At 390x844, the expanded calculation remained readable; the wide summary table
uses its horizontal-scroll container, so not every summary column is visible at
once. Screenshot saved as task output `account-observations-mobile.jpg`; viewport
reset after capture. This verifies the real event handler and narrow-screen
calculation for this example, not all browsers or full accessibility compliance.
No code changed in this verification increment.

## Verified increment: automatic standalone CLI backtest archiving

Standalone `backtest` now defaults its archive to `data/research` relative to the
working directory, so unflagged runs publish the pre-execution checkpoint and
full result. `--archive-dir` selects a different archive; listing/replay use the
same default. An unwritable checkpoint destination fails before simulation rather
than silently falling back to an unrecorded execution. Source parsing/auditing
still precede checkpoint publication. Other research commands are not implicitly
covered by this change, and separate working directories have separate defaults.

All four selected CLI integration tests passed (crash recovery, source/replay,
analysis-source consistency, malformed CSV). The source test explicitly verifies
that an unflagged run creates both records. Test subprocesses and the profile
parity fixture use isolated working directories to avoid writing repository/user
research data. Log: `work/ux-implementation/automatic-backtest-archive-tests.log`.
No full workspace/release or profile-parity rerun was performed this increment.

## Verified increment: recovery and evidence UI full gates

Both debug and release workspace gates passed: each ran 362 Rust tests,
15 frontend regression scripts, four Python exporter tests and the workspace
build. This includes the process-kill CLI checkpoint recovery test and the
new trade/account explanation regressions. Debug/release comparison passed
OHLC and tick execution in both directions with byte-identical execution
evidence, and evolution seeds 42 and 731 with 60 evaluations each and identical
rankings/log records after excluding executable identity from their headers.

The scripts ignore rule now admits scripts/tests/test_*.py so the exporter
regressions can accompany the repository; Python caches remain ignored.

Logs: work/ux-implementation/recovery-ui-debug-gate.log,
work/ux-implementation/recovery-ui-release-gate.log and
work/ux-implementation/recovery-ui-parity.log. These are local checks, not
independent broker parity or proof of a successful remote CI run. The six-step
completion audit above remains open.

## Verified increment: full assistant trial execution records

Assistant sessions retain a separate trial_executions collection for every
executed generation and details trial, including invalid results. Each record
binds the full BacktestResult (trades, equity, execution evidence and errors) to
its exact strategy, candidate index, tool and session-local tool-call number.
Non-executed invalid requests do not manufacture an execution record. Generation
summaries still retain candidates omitted from the provider-facing top five.
Full results are excluded from provider-facing summaries and normal web responses.
Both CLI and dashboard session-return archives now include these records with
their existing frozen dataset, configuration and executable identity.

Verification: 17 assistant library tests, 34 web library tests and the CLI
assistant archive integration test passed. Generation results are compared to
fresh direct engine results; invalid details results remain recorded. The web
backup/restore fixture includes nine full trial results and excludes credentials.
The CLI fixture covers returned success, provider failure and storage failure.
Logs: work/ux-implementation/assistant-full-trials.log and
work/ux-implementation/assistant-full-trials-cli.log. No full workspace/release
rerun was performed for this increment.

This closes the missing full-results payload for returned sessions, not crash
recovery: results remain in memory until the caller archives after run returns.
Large sessions can retain substantial result data. Durable per-trial checkpoints,
recovery/discovery and deterministic replay remain necessary; no independent
broker validation is established by preserving internally generated evidence.

## Verified increment: assistant per-trial immutable checkpoints

CLI and dashboard assistants now attach the shared research archive before
running provider tools. Every executed generation/details backtest first publishes
an assistant_trial_started record containing exact data, instrument, strategy,
configuration, periods-per-year, executable identity, source context and the
session/tool/candidate references. A unique session identifier prevents separate
sessions with identical strategies from silently merging their trial records.
Immediately after execution, assistant_trial publishes the full engine result and
links its checkpoint. Invalid engine results are retained too. Session-return
summaries include the full results and their checkpoint/result receipts.

A failed checkpoint write prevents engine execution. A failed result write retains
the input checkpoint and latches a terminal storage error; no further tool or
provider work continues through run(), and even direct dispatch calls cannot
resume the session after storage is repaired. Provider-facing summaries exclude
full execution evidence. No credential, prompt or provider conversation is an
archive input. Standalone library callers must opt into with_trial_archive;
production CLI and dashboard paths enable it automatically.

Saved research lists the new record types and explicitly disables unsupported
replay. Holdout exposure includes both checkpoints and results, conservatively
including possibly interrupted work. These are record counts, not independent
trial counts: one trial normally has two records plus its session summary.

Tests cover an actual process kill while the second provider response is pending:
both prior trials already have checkpoints/results, survive process termination
and source-file deletion, reproduce from frozen inputs and restore into another
archive. Additional tests cover invalid-result preservation, pre-execution
storage failure with permanent session stop, full-record privacy, web listing and
unsupported replay, and holdout disclosure. This does not yet prove recovery from
a kill during a particular engine calculation or power loss. There is no
user-facing interrupted-trial restart/replay command or provider-conversation
resume yet. Retaining full results in memory and in the final session summary can
still be expensive for large sessions.

Validation completed for this increment: both debug and release workspace gates
passed 365 Rust tests, 15 frontend regression scripts, four Python exporter tests
and workspace builds. The CLI integration was additionally rerun after adding
checks that all per-trial records exclude credentials/conversation content and
provider requests exclude full settlement evidence; both tests passed. Logs:
work/ux-implementation/assistant-checkpoint-debug-gate.log,
work/ux-implementation/assistant-checkpoint-release-gate.log and
work/ux-implementation/assistant-checkpoint-cli-final.log. No remote CI execution
or new browser visual verification was performed. The earlier debug/release
execution parity evidence was not rerun for this archive-only change.

## Verified increment: frozen AI trial restart and replay

The shared project assistant_trials module supports archived trial discovery,
exact-result comparison and checkpoint restart without an LLM/provider call.
CLI commands list-assistant-trials and replay-assistant-trial (with --resume for
checkpoints) expose it; Saved research offers Verify replay and Restart frozen
trial. Both interfaces require the current executable hash to equal the recorded
engine. Restart recalculates the whole trial, not an intermediate candle and not
the AI conversation. Data is read solely from the immutable archive.

Discovery distinguishes missing results from recorded results without inferring
process liveness. It exposes standalone result bundles too. Linked records must
agree on the complete request, dataset, strategy and engine identities. Restart
compares any existing linked result before publishing: identical results deduplicate;
conflicting evidence fails without creating a replacement. Replay compares the
entire serialized engine result and separately reports execution validity; a
reproduced invalid result is not converted into a valid backtest. CLI restart
returns failure after archiving an invalid engine result. Standalone portable
results can replay without the original checkpoint; an available checkpoint must
still agree with the result's identity.

Regression evidence includes nonempty trades, wrong engine/mode rejection,
altered equity, contradictory linkage, idempotent restart, invalid execution,
standalone backup replay and archive listing. The real process-kill CLI fixture
now restores only input checkpoints to a fresh archive after source deletion,
then exercises CLI discovery, restart and replay and reproduces original result
IDs. Web route tests exercise both actions. The production UI handler test covers
explicit restart mode, invalid-result wording, missing receipts and network errors.
No new rendered-browser verification has been performed in this increment.

Both workspace gates passed 368 Rust tests, 16 frontend regression scripts, four
Python exporter tests and workspace builds. Listing was subsequently checked to
retain only record metadata rather than all bar/result payloads simultaneously;
restart comparisons retain result hashes instead of every prior result payload.
A supplementary parallel test exposed colliding temporary directory names; these
now use an atomic sequence, and all three recovery unit tests passed again in
both debug and release. The full debug gate preceded the final memory refinement;
the release gate and final targeted tests include it. Logs:
work/ux-implementation/assistant-trial-recovery-debug-gate.log,
work/ux-implementation/assistant-trial-recovery-release-gate.log,
work/ux-implementation/assistant-trial-recovery-unit-final.log and
work/ux-implementation/assistant-trial-recovery-release-final.log.
No independent broker parity or remote CI execution is established.

## Publication note: workflow activation pending

GitHub rejected the initial push because the OAuth connection lacks workflow
scope. The workflow is therefore versioned as ci/reliability-workflow.yml, not
an active .github/workflows file. Publish that template to its active location
with an authorized workflow-capable connection, verify the matrix and configure
required checks. Earlier references to the local workflow do not establish active
remote automation. TODO.md tracks this prerequisite.

## MT5 reference margin-policy increment

See MT5_VALIDATION.md for the observed demo configuration, clock limitations,
read-only collection and explicit forex-base margin policy. Local debug/release
workspace gates passed 372 Rust tests, 16 frontend scripts, five exporter tests
and builds. Four execution and two evolution debug/release parity fixtures also
passed. An additional tick-specific margin-rate change test was added after the
full gates; its targeted debug run passed. Logs are under work/mt5-validation:
margin-policy-debug-gate-fixed.log, margin-policy-release-gate.log,
margin-policy-parity.log and margin-tick-final.log. The initial debug gate found
a test fixture missing the new margin evidence field; that fixture was corrected
before the successful rerun. New UI labels have regression/syntax coverage but no
new rendered-browser verification. General directional profit conversion and
independent execution coverage remain incomplete.
The final targeted release run also passed all three margin-model tests, including
the added tick case (work/mt5-validation/margin-tick-release-final.log).

## Directional profit conversion validation

Opt-in directional historical profit rates now feed sizing/admission reserves,
account valuation, cash settlement and stop-out consistently by position side.
Independent margin conversion remains separate. The frozen USDJPY/EUR reference
uses both directions in one account configuration; mixed-strategy and tick-time
causality regressions are included. See MT5_VALIDATION.md for scope and source
limitations. Debug and release workspace gates each passed 376 Rust tests,
16 frontend checks, five exporter tests and builds. Four execution and two seeded
evolution profile comparisons passed. Logs: work/mt5-validation/directional-debug-gate.log,
directional-release-gate.log and directional-parity.log. This does not establish
actual-fill or general broker-conversion-policy parity.

## Broker-clock import boundary

New MT5 exports declare raw broker time in their CSV header. The shared Rust
import functions reject these until normalized. The normalizer emits UTC headers
and rejects ordinary UTC input unless the caller explicitly declares it as an
older broker-clock export. End-to-end Python-to-Rust coverage preserves raw data,
price precision and duplicate-millisecond ticks. The full debug gate passed
377 Rust tests, 16 frontend checks, ten Python tests and workspace build (log:
work/mt5-validation/broker-header-debug-gate.log). No full release or profile-parity
rerun was performed for this increment. Normalization-bundle provenance is not yet
automatically embedded by the research import/archive paths; misdeclared older
or headerless CSV remains possible. See MT5_VALIDATION.md for remaining scope.


## Verified CLI normalization provenance

The latest debug and release workspace gates each passed 378 Rust tests,
16 frontend checks, ten Python tests and workspace builds. Logs:
work/mt5-validation/provenance-debug-gate.log and provenance-release-gate.log.
Explicit CLI candle bundle attachment now validates hashes, clock arithmetic,
unchanged price payloads, exact input candles and timeframe. Archives embed the
original bundle; replay/resume verifies it again after restore without source
files. Rehashed false timestamps and contradictory verification metadata fail.
This extends the preceding import-boundary increment; it does not authenticate
broker data or prove the declared clock policy. Dashboard, manifest imports and
tick provenance attachment remain unfinished. CI activation and independent
execution/papertrading comparison remain open. No additional profile-parity or
rendered-browser run was performed for this provenance-only increment.


## Tick normalization provenance

CLI backtests now accept an explicit tick-normalization bundle, bind it to the
exact input tick sequence and preserve it in the archive. Replay/resume verifies
the embedded bundle and derived metadata. Targeted sqx-data broker-clock and
CLI normalization-archive regressions passed, including reordered/missing
same-millisecond ticks, source-free restored replay and altered metadata.
Logs: work/mt5-validation/tick-provenance-tests.log and tick-archive-tests.log.
The preceding full-workspace gates predate this increment. This proves neither
complete tick capture nor independent execution parity; dashboard and manifest
provenance integration remain open.


Cross-check provenance follow-up: each market manifest can now supply a
normalization_bundle, resolved relative to that manifest. Exact candle/timeframe
verification runs for all markets before any comparison output. Regression tests
prove source retention and whole-request rejection when a later market's bundle
mismatches. Existing cross-check source tests also pass (log:
work/mt5-validation/crosscheck-provenance-tests.log). This does not add durable
cross-check registration, dashboard import provenance or symbol authentication.


Cross-check durable-registration follow-up: the CLI now writes the full market
inputs, configurations, source evidence and strategy before execution, followed
by all execution results and the summary, including failed simulations. Results
reference their starting checkpoint and executable hash. Targeted integration
checks confirm input/result retention, checkpoint linkage, preflight rejection
without run publication and retained errors (crosscheck-archive-final.log).
The provenance integration also passed (crosscheck-archive-tests.log). Dedicated
cross-check replay/resume remains unfinished; no recovery guarantee is claimed.


Cross-check replay/resume follow-up: replay-crosscheck now requires the exact
recorded executable, verifies embedded candle provenance, and compares the full
execution arrays and summary. --resume restarts all frozen markets from a starting
checkpoint and records the result; it does not continue midway through a market.
Restored archives work without original source files. Tests cover unchanged replay,
identical resumed result identity, invalid execution disclosure, changed results,
wrong executable, wrong record type and altered provenance metadata. All 19 CLI
tests passed (crosscheck-replay-cli-gate.log); the extended provenance test passed
separately (crosscheck-replay-provenance-final.log). This increment has not received
a full workspace/release rerun or a process-kill recovery test. Independent broker
execution validation and the remaining programme requirements are still open.


Cross-check interruption evidence: a real CLI process with two markets of 50,000
candles each was killed after its checkpoint was published and while still alive.
The test verified that no final result existed, removed the original CSV, strategy
and manifest, restarted from the checkpoint, and checked both complete executions
and exact replay. The targeted debug test passed (crosscheck-crash.log). This
covers process interruption, not power loss or disk corruption. Resume recomputes
all markets; it is not incremental continuation from a partially executed market.


Post-cross-check full verification: debug and release workspace tests/builds
both passed, each with 379 Rust tests, 16 frontend checks and ten Python tests.
The real-process cross-check interruption regression passed in both profiles.
Logs: work/mt5-validation/crosscheck-full-debug.log and crosscheck-full-release.log.
This supersedes the preceding increments' missing full-gate status, not their
remaining functional limitations. CI remains a local template, not active GitHub
Actions. Broker execution parity, synchronized conversion data, dashboard source
provenance and recoverable live capture/paper comparison remain unfinished.


## Cost-unit disclosure after captured-data comparison

The captured-data reference exposed that CLI/UI "points" could be mistaken for
MT5 broker points. Settings, saved-profile help, research summaries and recorded
run headers now name FX pips; Settings explains EURUSD 0.0001 and USDJPY 0.01,
with an explicit 0.001 broker-point -> 0.1 pip example. Slippage is per fill;
commission is round-trip per lot in account currency. Fractional cost inputs have
step=any, including profile fields. CLI help carries the same distinction. No
stored value is silently rescaled. JavaScript syntax, backtest-error and comparison
UI regressions passed. This increment has not had rendered-browser verification;
custom instrument definitions and broker point-size mapping still need validation.


## Independent fixture checks in the local gate

ci/check.sh now invokes ci/check-reference-fixture.py after its existing checks.
The generated synthetic fixture exercises eight long/short reference cases,
independent Decimal arithmetic, archive replay and future-data invariance without
MT5 access or private source files. It includes nonempty trades and actual stop
quote jumps; a changed terminal result is required in the mutation check.
The standalone added gate passed against both debug and release executables
(independent-fixture-debug.log and independent-fixture-release.log). The whole
combined gate has not been rerun for this wiring-only change. This establishes
portable regression coverage, not real-market completeness or broker fill parity.
The tracked workflow remains a template; remote CI activation is still open.


## Remote CI activation and first verified run

The connected GitHub app published .github/workflows/reliability.yml in commit
e832c2f12ca209f7cabdedde4ce815d9b155cb9e. The earlier Git OAuth scope limitation
was specific to that connection. GitHub run 45357592150 completed successfully:
https://github.com/ronald1711/sqxlabs/actions/runs/45357592150
All four Linux/macOS × debug/release jobs passed; both release jobs also passed
debug/release trace comparisons. This verifies the published baseline only.
New local MT5/provenance/reference changes have not yet been pushed.

The full current local gates passed separately: 379 Rust tests, workspace builds,
16 frontend checks, 12 Python tests, plus the eight-case independent arithmetic,
archive replay and future-mutation gate in each profile. Logs:
activated-ci-local-debug-fixed.log and activated-ci-local-release.log. The initial
debug attempt caught an outdated UI-test expectation for points versus FX pips;
the test now checks both spread and slippage units, and the full gates were rerun.
Required branch-protection checks and remote verification of the expanded local
changes remain open. Green CI is regression evidence, not broker certification.


## Expanded code verified remotely

Commit 156c0d7e620710586dca7cad8dfc5036c061d9bc publishes the MT5 account,
clock-provenance, capture/reference, cross-check recovery and cost-unit changes.
GitHub run https://github.com/ronald1711/sqxlabs/actions/runs/45358671142 completed
successfully on that exact SHA: Linux/macOS debug/release, including independent
synthetic reference checks and both profile comparisons. Local user databanks,
projects, research archives, raw captures and work files were excluded from the
commit. All explicit programme requirements remain subject to their separate
evidence gates; remote CI success does not prove broker fill parity or live
recording/papertrading readiness. Required branch checks are not yet configured.

### Independent segmented walk-forward arithmetic fixture

`crates/sqx-optimizer/tests/walkforward_reference.rs` independently computes a
small declared contract without calling engine, indicator, metrics or ranking
helpers for its expected values: 180 synthetic EURUSD M1 candles, USD account,
two disjoint 90-bar segments (63 training / 27 test), SMA3 long/short candidates,
one-candle holding period with next-open exits, end-boundary liquidation, 2%
current-cash sizing against a 100-pip sizing distance, downward 0.01-lot steps,
$3.50/lot round-trip commission, and no spread/slippage/financing.

Integer-pip prices provide a separate SMA/entry calculation. The fixture compares
both candidates' training trade counts, net results, gross gains/losses and profit
factors, the chosen candidate, OOS trade counts, each carried account balance and
aggregate OOS results. Altering the first OOS segment leaves its training evidence
and selection unchanged while changing its test outcome. The fixture requires
wins, losses and changed carried capital, and checks first-OOS-bar eligibility
using historical warmup. This is segmented research with positions closed at
segment boundaries, not uninterrupted portfolio execution or MT5 fill parity.
Other indicators, multi-timeframe warmup and untouched-holdout routes remain open.

Impossible window counts are now rejected before entering the per-window loop;
previously an enormous count could iterate and accumulate errors unnecessarily.

### macOS provider-fixture correction

GitHub run 45364814462 exposed a WouldBlock error in the assistant archive test's
mock provider after accepting a connection from a nonblocking listener. The
fixture now explicitly selects blocking accepted sockets with bounded read/write
timeouts. A delayed-fragment test exercises this condition on every platform.
This fixes the test transport rather than relaxing archive or engine assertions.
The affected run is failed evidence, not a successful release gate. Replacement
run 45365576256 for commit 68a1319 passed all four Linux/macOS debug/release jobs
and both profile comparisons. This does not certify later commits automatically.

### Crosscheck archives in holdout exposure audits

The exposure registry now reads every market in CLI crosscheck archives instead
of trying to parse a multi-market dataset as a single backtest. Previously such
an archive made experiment status/commitment fail. Both execution checkpoints
and completed or failed reports count conservatively as recorded input exposure;
a checkpoint cannot establish what was inspected before an interruption.

The audit records the matching market index and observation interval. Multiple
matching datasets in one archive remain one record count, while checkpoint and
result remain separate records rather than being advertised as independent
trials. Unrelated markets and intervals outside the holdout do not create overlap.
Malformed empty market inputs fail closed. A final commitment retains known reuse
after its experiment backup is restored without the original crosscheck archive.
The dashboard displays the separate crosscheck-record count.

Regression coverage includes a matching market after an unrelated first market,
multiple matching datasets, an interrupted checkpoint, a failed result, temporal
boundaries, retained exposure after restore, and actual CLI-produced crosscheck
archives read by the experiment registry. This closes this archive-format gap;
it does not establish that external or unarchived research was never performed.

### Candle contents, not only candle-open timestamps, define data exposure

A separate reproduction showed that an H1 candle covering minutes [60,120) was
not detected against an M1 holdout covering [70,100): its opening timestamp alone
was outside the holdout. Observation ranges now include the final candle's full
declared duration, with an inclusive millisecond end immediately before the next
boundary. This applies to the target holdout itself, other experiment development
and final periods, single/multi-market archives, restored paper comparisons and
durable evolution claims.

Fixed intervals use the declared duration/timeframe; UTC calendar months use their
actual calendar boundary, including February in leap years. Contradictory,
nonpositive, overflowing or unsupported declarations fail validation. New
experiments and evolution jobs check their interval metadata before publication.
The normalization bundle format currently supports fixed-width candles only;
the dashboard no longer accepts nominal thirty-day normalization as MN1 evidence.

Missing legacy duration metadata is not inferred from candle gaps. Its known
opening timestamps remain usable, but observation_extent_complete is false and
the unresolved sources are listed. The UI displays an incomplete exposure audit,
including for legacy commitment reports, and saved commitments preserve that
uncertainty after restoration without the original source archives. A
no_known_registry_overlap classification alone is never an independence claim.

Tests cover hourly/minute overlap, the target's final candle, exact exclusive
boundaries, other experiment training inputs, restored comparison references,
interrupted evolution claims, true calendar-month ends, contradictory metadata,
legacy uncertainty and restoration. The originally failing reproduction is retained
locally as holdout-interval-probe.log; the corrected cases are in the regression
suite. This addresses observation extents, not independent correctness of all
higher-timeframe trading signals or proof of isolation from external research.

### Exact executable retention (local procedure)

`scripts/retain_engine.py` stores executable bytes under their SHA-256 identity,
matching the engine IDs recorded in research archives. Publication uses a synced
pending file and a no-replacement hard link. Existing objects are rehashed;
corruption, identity mismatches and symlinks fail instead of silently replacing
retained evidence. The copy is not executable and is never launched by this tool.

Three regression tests cover concurrent retention, replacement of the original,
wrong expected identity, existing corruption and symlink rejection. The Python
suite passed all 37 tests. A retained copy of the current debug engine was copied
to an isolated executable path and passed the independent eight-case reference,
archive replay and future-mutation gate. This is local evidence of usable byte
retention, not independent certification of all engine behavior.

The manual operation and restoration boundaries are documented in ci/README.md.
Automatic retention at every research entry point, permanent release artifacts,
OS/runtime dependency retention and representative power-loss validation remain
open. Research backups alone still do not include the executable.

### Automatic engine retention on new archive routes

New CLI backtest, assistant and crosscheck runs retain the current executable
before their first checkpoint. New dashboard backtest, pipeline, robustness and
assistant archives, experiment creation and evolution creation also retain exact
engine bytes under the archive's `engines/<engine-id>.bin`. The dashboard compares
these bytes with its startup identity and refuses changed bytes. Stop processes
before replacing executables; this is not an attestation of the loaded OS image.

Retention uses immutable, synced archive publication. Existing corrupt copies
prevent new archive publication; the CLI assistant fails before contacting its
provider when the engine cannot be retained. JSON research backup/restore does
not embed or execute binaries. Tests cover concurrent publication, corruption,
symlink rejection, non-executable permissions, CLI byte equality, dashboard
publication refusal and absence of provider connections on storage failure.

Legacy job resume and other CLI/MCP paths still need integration. This does not
retain historical engines that are already missing, package runtime dependencies,
prove source provenance or establish permanent remote release retention.

Validation for this increment: both full local debug and release gates passed,
including workspace tests/builds, frontend checks, 37 Python tests and the eight
independent reference cases with archive replay and future-price mutation. The
initial assistant fixture failure reflected the intentional earlier storage
failure; its replacement now asserts zero provider connections in that case.
Remote verification is required separately for the published commit.

### Normalization evidence retained by frozen experiments

New dashboard experiments now preserve the complete optional clock-normalization
bundle in their frozen dataset provenance. Experiment creation and backup restore
reverify the bundle against the exact candles and declared duration. A backup
with altered candles is rejected even if its outer dataset, plan and backup
identities have been recomputed. Missing legacy bundles remain missing evidence;
this change does not manufacture provenance for them.

A synthetic two-hour-offset fixture verifies source retention, source-free restore,
rehashed candle-edit rejection, invalid policy rejection and duration mismatch.
The dashboard experiment/search/backup/restore test now includes a complete bundle
and checks that its exact strings survive. This verifies transformation integrity,
not the truth of the clock policy, broker identity or tick completeness. Evolution
and other research routes still need equivalent source-bundle propagation.

Both full local debug and release gates passed for the experiment normalization
increment: workspace tests/builds, frontend checks, 37 Python tests and independent
reference/replay/future-mutation checks. Publication and remote checks are separate.

### Normalization sources in durable genetic searches

New evolution jobs have an optional source-bundle field separate from their
1 MiB metadata context. Dashboard creation passes the imported bundle; CLI-style
contexts containing a bundle are lifted into this field before the context limit
is checked. Source bundles are bounded separately at 20 MiB. Missing legacy
sources are not manufactured. Existing embedded legacy bundles remain readable.

Creation, stored-job reads and backup verification recheck the source against
initial candles and duration. Recorded normalization verification metadata must
agree with the recomputed result. Duplicate sources, orphan verification claims,
and rehashed candle alterations are rejected. The source remains part of the
content-addressed job and raw backup JSON; it is not an external mutable link.

Targeted tests pass for a source larger than 1 MiB, preserved metadata bounds,
source-free restore, legacy bundles, rehashed candle edits and altered verification
metadata. A dashboard test checks exact source retention after removing live data.
The full workspace/profile gates are checked separately before publication.

Full local debug and release gates passed for this source-bundle increment,
including workspace tests/builds, frontend checks, 37 Python tests and independent
reference/replay/future-mutation checks. Frozen-input consistency across later
checkpoint states is a separate review item; source validation of the initial
job must not be represented as proof of every subsequent state transition.

### Frozen inputs across evolution checkpoints

A new regression reproduced acceptance of a generation checkpoint with changed
source candles after recomputing its outer hash. Backup verification previously
checked chain structure and coverage without binding every later state's immutable
research inputs to the initial job. The correction compares schema, generator,
genetic settings, fitness, candles, instrument, execution/account configuration
and annualization across checkpoints. Population, RNG position and progress are
allowed to evolve; this check does not independently prove their transition.

Both backup verification/restore and stored-history reads now reject changed
frozen inputs. Restore compares against its already-verified incoming job before
publishing the new job header, preserving header-last publication. All eight
evolution lifecycle tests pass, including valid restore, concurrent publication,
replay, and rejection of rehashed candle, initial-deposit and annualization changes
on restore, status and resume. Full profile gates are checked separately.

The frozen-input correction passed both full local debug and release gates:
workspace tests/builds, frontend checks, 37 Python tests, and independent reference,
archive replay and future-price-mutation checks. This extends structural binding
of checkpoints; it does not replace full transition replay or broker evidence.

### Durable browser imports and explicit reload after restart

CSV import now saves an immutable, content-addressed dataset version before
replacing loaded data. Versions include symbol/timeframe, candles, calendar and
optional exact normalization bundle. The Data Manager lists saved versions and
loads a selected version after rechecking its hash, structural data quality and
normalization/candle agreement. Listing retains summaries rather than all source
payloads in memory. Loading replaces the active symbol/timeframe only when the
saved version passes validation; older saved versions remain available.

Storage failure and corrupted snapshot tests preserve previously loaded data.
Other tests cover deduplication, separate versions, reopening, path traversal
rejection and exact normalization retention. A browser smoke test imported a
synthetic two-bar dataset, stopped and restarted the isolated server, then loaded
the same saved version and observed the selected dataset and success message.
Screenshot: local saved-import-restart.jpg (not broker evidence).

Saved imports live under the research archive's `imports` directory. Copy the
complete research directory for a filesystem backup. Existing per-run JSON
backups do not automatically include the saved-import catalog. Startup does not
silently choose a version over CLI/QuestDB input: users explicitly load a version
in the Data Manager. QuestDB loads and CLI sources are not automatically captured
by this CSV-import mechanism. This proves process-restart recovery for the tested
fixture, not arbitrary power-loss, storage-device or large-catalog performance.

Both complete local profile gates passed for durable imports: workspace tests and
builds, frontend checks, 37 Python tests and the independent eight-case reference
fixture with archive replay and future-price invariance. GitHub run 45380020385
for the preceding frozen-input commit 12c4185 passed all four Linux/macOS profile
jobs and both debug/release trace comparisons; this is not CI evidence for the
later durable-import change.

### Isolated MT5 reference compiler feasibility

A fresh Wine prefix and a separate copy of the installed MetaEditor executable
compiled a minimal tester-only initialization probe. The UTF-16 compiler log
reported zero errors and warnings and produced an EX5 artifact. The Wine process
returned 1 despite this successful compiler result; exit status alone is not a
usable compilation success criterion for this installation. Local inventory
records executable, source and output hashes under
`work/mt5-validation/isolated-tester/inventory.json`.

Only the editor executable was copied: no account configuration, credentials,
terminal history or trading program was imported. No terminal or expert was
executed. This establishes isolated compilation feasibility only, not tester
execution or strategy parity. Next evidence must include frozen custom-symbol
data, explicit instrument/account/tester configuration, guarded reference source,
the actual tester report and a per-event comparison against SqxLabs. Local agents
must be explicit and remote/cloud agents disabled in that separate environment.
The shared SiliconMT5 terminal remains read-only.

Official compiler and portable-mode documentation:
https://www.metatrader5.com/en/metaeditor/help/beginning/integration_ide
https://www.metatrader5.com/en/metaeditor/help/beginning/open

The subsequent isolated bootstrap probe copied terminal/tester executables only,
compiled a no-order custom-symbol import script (120 synthetic M1 bars and 480
bid/ask ticks), and launched the separate terminal with a startup script config.
The script checks its isolated installation path and absence of an account login
before any custom-symbol writes, and checks tick readback before publishing its
ready marker. Compilation reported zero errors/warnings. The terminal log proved
startup in the isolated directory but contained no script execution evidence, and
no ready marker appeared. Thus custom-data import has **not** been established.
The isolated terminal process was identified by its exact executable path and
stopped (exit 143); the shared terminal was not signalled. The local MCP endpoint
returned HTTP 401 to an unauthenticated discovery request; no authenticated tools
or account operations were attempted. Diagnose startup-script prerequisites before
claiming a usable tester reference. Probe files remain local under `work/`.

Adding an explicit EURUSD startup chart and enabling program execution (with live
trading and DLL imports still disabled) produced a script-loaded log entry. An
entry marker placed before the import was absent, as was the import-ready marker.
A separately compiled minimal no-order expert also loaded but produced no
initialization marker. This narrows the failure to program startup/execution
rather than demonstrating an import-function failure; its cause remains unknown.
The local `bootstrap-diagnosis.json` records these observations and the log hash.
Script probe terminals were stopped using their verified isolated executable
paths. The minimal-expert terminal exited on its own with code 0; its final log
reported expert removal and MQL5.community authorization failure. Neither a clean
process exit nor the loaded entry establishes execution of the probe handler.
No account login, remote agent or live order was used. The fixture remains
unvalidated and must not be consumed as independent execution evidence.

### Symmetric tick stop-gap arithmetic reference

`sqx-backtest/tests/stop_gap_reference.rs` specifies EURUSD bid/ask quotes in
integer 1e-5 price units, a USD 10,000 account, 0.1 lot, a ten-pip stop,
one-pip adverse slippage at both ends and USD 7 round-trip commission per lot.
Long and short gaps each lose USD 24.70 (240 price units at USD 0.10 per unit,
plus USD 0.70 commission). Assertions cover actual quote timestamps, entry and
gap-exit prices, stop reason, trade PnL, single cash settlement and USD 9,975.30
final equity. An intentionally different configured spread checks that supplied
tick bid/ask is not charged a second synthetic spread. Expected values are fixed
independent arithmetic, not copied from engine output. This is an execution-model
contract test, not MT5/broker execution evidence or proof of all stop behaviours.
The targeted test passed in both debug and release profiles. No engine behaviour
was changed by this addition.

Four additional fixed-price cases cover long/short trailing and break-even
activation on supplied ticks. An adverse excursion precedes the favourable tick;
the only stop-change event must occur at the favourable tick, never retroactively
at the earlier excursion. A later quote crosses the new stop and liquidates at
the executable bid/ask. Independent arithmetic gives USD 5.30 net for the trailing
cases and USD -2.70 for break-even gaps, including USD 0.70 commission. Assertions
cover stop-change time/level, exit reason/time/price, PnL and final equity. All six
fixed-price cases passed in debug and release. These cover the declared ten-pip
distance and fixed-size USD-account contract only; ATR distances, combined stop
types, financing, conversion and broker parity need separate evidence.

The dashboard Overview now distinguishes execution-model reference examples,
per-run data/replay records and the still-required independent strategy/broker
comparison. Its disclosure explains holdout discipline and explicitly states that
this is validation guidance, not live CI status or approval of a selected strategy.
The embedded page built successfully, inline JavaScript passed syntax validation,
and the rendered overview/disclosure were checked in the browser, including the
current narrow viewport. No new runtime validation status is inferred from this
informational UI change.

### Isolated tester math-mode diagnostic

A subsequent no-order probe in MT5's mathematical-calculation mode reached both
OnInit and OnTester on its local test agent. The agent log records the expected
OnTester result 42 and mathematical-test completion; an HTML report was created.
The terminal nevertheless reported "some error after pass finished" before
exiting with code 0, so this is not an unqualified successful tester run. Source,
compiled program, config, logs and report hashes are recorded locally in
`work/mt5-validation/isolated-tester/math-probe-evidence.json`.

This narrows the earlier startup problem: compiled MQL can execute in the
isolated test agent even though chart-attached startup probes did not produce
handler evidence. Math mode has no market history, symbol information or order
simulation, and is **not** independent signal/fill/account validation. Local
agents were explicitly enabled and remote/cloud agents disabled. No shared
terminal settings or broker account were changed.
Reference: https://www.metatrader5.com/en/terminal/help/algotrading/tick_generation

### Retain the engine before creating CLI evolution jobs

`start-evolution` now retains the exact current executable in the selected store
before publishing the frozen job. Previously it saved only its hash, leaving
resume dependent on a manually preserved executable after an update. A corrupt
existing engine object aborts creation and is never replaced. The CLI integration
regression verifies byte equality and matching job identity, refusal without a
published job on corruption, and subsequent generation-boundary resume in
separate processes after the original input files are removed or changed.

This covers new CLI evolution jobs only. Existing jobs cannot recover an engine
that was never retained. JSON backups still omit executable bytes, and retaining
a binary neither packages its runtime dependencies nor certifies strategy results.

Validation: the CLI evolution integration test passed in debug and release;
`cargo build --workspace` passed. This increment does not claim a new full
workspace test run or a completed remote CI run.

### Inspect recovery-engine availability separately from compatibility

Explicit CLI evolution status and the dashboard's saved-search inspection now
check the retained engine copy: `verified` means the stored regular file hashes
to the job's engine identity, `missing` means no copy is present, and `unverified`
means it could not be validated (including corrupt bytes, symlinks and legacy
non-hash identifiers). Inspection never executes or replaces stored bytes. Large
executables are hashed as a stream. This check runs on explicit inspection, not
once per job when listing searches, and does not change compatibility gates for
the currently running engine.

The report explains that JSON backups omit engine copies and a verified copy is
not proof of operating-system/runtime compatibility or trading correctness.
Archive regressions cover missing, retained, corrupted, symlink and restored
backup cases; CLI status checks the verified result and report tests cover all
statuses, including an unknown-status fallback.

Validation for engine inspection: `cargo test --workspace` and
`cargo build --workspace` passed locally. Evolution report, recovery and restore
JavaScript checks passed. This is automated rendering coverage; no new browser
interaction or cross-platform CI pass is claimed for this increment yet.

### Preserve MCP generation execution failures in the response

The MCP `generate` tool previously used the optimizer's metrics-only compatibility
view, losing candidate execution diagnostics before ranking. It now consumes
detailed evaluations, excludes every failed execution from ranking, returns all
failed candidate identities/rules/errors in research context, and sets the MCP
error flag when any candidate failed. Valid partial rankings are explicitly
labelled incomplete. Counts distinguish attempted, invalid, eligible and valid
but below ranking criteria; seed, objective and minimum trades are explicit.

A valid no-trade or undefined-ratio evaluation is not itself an execution failure.
The regression includes a failed candidate with apparently excellent metrics, a
valid eligible candidate, too few trades, a nonfinite ratio and an all-failed
batch. This is response evidence only: successful non-selected candidates and
interrupted work are not durably archived by the tool. Full MCP trial accounting
and portfolio-route failure accounting remain open.

Validation: all five MCP tests passed in debug and release; the workspace build
passed. Earlier publication 5a911a8 completed all four Linux/macOS CI jobs and
release-profile comparisons successfully (run 45387699788). That run predates
the subsequent engine-retention/inspection and MCP changes.

### Fail closed on incomplete MCP portfolio candidate evaluation

The MCP portfolio route now checks all component execution errors and validates
account observations before any shortlist/subset selection. If any candidate
failed, or result coverage differs from the candidate count, it returns a
protocol error with the failed candidate rules, indices and diagnostics; it does
not proceed with a silently reduced candidate pool. Insufficient valid trade
evidence is separately reported with source/account context instead of losing
that context in a generic error.

For valid searches, context retains attempted/eligible/below-criteria counts,
seed, objective, minimum trades, shortlist/subset limits and original candidate
indices for shortlist and selection. Combination errors also preserve context.
Regression cases distinguish an execution failure, a valid no-trade evaluation
and a nonfinite final account balance. This improves returned evidence only;
durable complete candidate history, interruption recovery and independent
portfolio/account validation remain open.

Validation: all six MCP tests passed in debug and release;
`cargo build --workspace` passed. No independent portfolio parity is claimed.

### Durable MCP generation checkpoints before selection

MCP generation now retains exact engine bytes and an immutable search checkpoint
with the full frozen candidate family, bars, instrument, timeframe/calendar,
configuration, seed and research context. Every candidate gets a checkpoint before
execution and a complete backtest/metrics/error/eligibility record immediately
afterwards. Only after all results are durable is the evaluation-completion record
published and ranking returned. Execution is sequential at this entry point so a
write failure stops subsequent work; other optimizer callers retain their existing
parallel behavior. Repeated requests have distinct random attempt identities; the
strategy generation seed and backtest calculations are unchanged.

The server-controlled SQX_MCP_RESEARCH_DIR selects storage (default data/research);
tool callers cannot override it. The generation_history tool reads content-verified
records and reports planned candidates, trial checkpoints, results, completion
records and orphaned records. It does not execute or resume anything, certify
relationships/replay, authenticate origin, or infer worker liveness. Research
context points to the saved search/completion identities. Full retained candidate
results now include failures and non-selected candidates for this route.

Tests reopen a completed mixed-validity search, distinguish repeated requests,
refuse execution on checkpoint storage failure and simulate a mid-candidate
interruption. A separate process test kills a real MCP generation after initial
trial publication, deletes the source CSV and inspects the incomplete search in
a fresh server process: prior results survive without a completion claim.
Single-trial replay/restart, completed selection reconstruction, all other MCP
routes, broader conversion/recording provenance and whole-programme trial coverage
remain open. This does not establish market execution correctness.

A result-write failure regression also proves the next candidate is not executed;
its existing checkpoint survives when the simulated storage outage is removed.
Shared-dashboard listing labels these MCP records explicitly, disables unsupported
replay/restart, and labels a download as one record rather than a whole-search
backup. The backend independently rejects unsupported replay.

Validation: workspace tests and build passed with the lockfile; final MCP tests
(10 unit cases plus the separate-process kill/inspection test) passed in debug
and release. The added dashboard refusal test, report JavaScript syntax and
recovery UI checks passed. No new browser interaction or independent broker
comparison is claimed for this increment. Whole-search portable backup and
restore of linked trial records remain open.

### Replay and explicitly restart one MCP generation trial

The generation_trial tool compares a saved trial result against recomputation,
or restarts a trial checkpoint when restart=true is explicit. It checks engine
identity, search/checkpoint kinds and state, candidate index and full rule tree,
dataset identity, frozen configuration/annualization and the result/checkpoint
relationship before execution. Input overrides and ambiguous restart values are
rejected. It uses the currently running engine, never an archived executable.

Comparison uses the serialized outcome bytes, including the full backtest,
metrics, errors and eligibility flag. Nonfinite values retain the existing JSON
null encoding; this is equality of the retained representation, not recovered
nonfinite payload bits or independent correctness. Restart retains the engine
and refuses a conflicting prior outcome instead of replacing it. A deterministic
repeat returns the same outcome identity. It neither executes other candidates
nor creates a completed-search record; repeated restarts are not independent
strategy trials or a complete count of execution invocations.

Regressions reject wrong engine/strategy/index/settings and mode, detect modified
results (including a signed-zero serialization difference), preserve conflicting
outcomes, and restart an interrupted candidate without advancing the remaining
family. The separate-process kill test also replays/restarts a saved trial through
MCP after removing its source CSV. Whole-search resume, completion reconstruction,
portable linked-search backup and the other MCP routes remain open.

Validation: 12 MCP unit tests and the separate-process interruption/replay/restart
test passed in both debug and release; the locked workspace build passed. The
preceding published commit b0bd715 passed all four Linux/macOS CI jobs and profile
comparisons in run 45390250514; that run predates the new archive/replay work.

### Whole-search MCP recovery with replayed prefix and process exclusion

`generation_resume` accepts only a saved search ID. It validates the frozen search
kind, engine, bounds, inputs and candidate/result relationships, replays prior
outcomes byte-for-byte, then saves missing candidate outcomes and a completion
record. The original generation path uses the same executor. Existing results
are never replaced; mismatching outcomes, conflicting completion records and
completed searches with missing results fail closed. Repeated successful recovery
replays all outcomes and returns the same completion without new search/result
records. Previously completed computations are deliberately replayed, not skipped.

Generation, full recovery and single-candidate restart share a nonblocking OS
file lock for that search. A competing writer is refused; a dead process releases
the lock even though its lock file remains. Read-only replay remains available.
A completed record means evaluation coverage, not that all candidates were valid.
Responses include verified/new result counts, failures and a valid-only ranking;
failures retain the MCP error status. No stored executable is launched.

Unit coverage includes interrupted-prefix recovery, conflicting completion
evidence, duplicate recovery without extra records and writer exclusion. A
separate-process fixture kills a 64-candidate generation, removes its source CSV,
restarts/replays a candidate, recovers the full search and compares every outcome
with a fresh uninterrupted search under the same engine and seed. This is
repeatability/recovery evidence, not an independent trading-model reference.
Portable linked-search backup, all other MCP research routes and full programme
provenance/independent execution validation remain open.

Validation: all 13 MCP unit tests and the expanded separate-process recovery test
passed in debug and release; the locked workspace build passed. The 64-candidate
process fixture compared every serialized candidate outcome and checked a
repeated full recovery creates no extra result records. It took approximately
94 seconds in debug and 9 seconds in release on this machine. No new independent
broker-execution evidence is claimed.

### Keep MCP generation exposure visible through holdout commitment and restore

Holdout audits now classify all four MCP generation record kinds separately from
ordinary backtest records. A search checkpoint, trial checkpoint, failed trial
or evaluation-completion record still discloses the declared market-data range;
none is treated as an independent trial count. The dashboard summary includes
MCP and assistant record counts explicitly. The regression verifies complete
candle extents, non-overlapping periods/other markets, and carried commitment
evidence after restoring an experiment without the original MCP archive.

The current completion audit above was refreshed to remove resolved pending
items (persistent browser imports and selected engine retention/recovery routes)
while preserving all six original completion requirements and unresolved gates.

Validation for the MCP holdout classification increment: all 32 experiment tests
passed in debug and release, the workspace build passed, and JavaScript syntax
and holdout-label checks passed. The new boundary assertion was corrected to
the audit API’s documented inclusive final millisecond (5,999,999), not the
exclusive candle-end boundary; the engine interval convention was unchanged.

### Preserve missing scenario coverage instead of forcing a passing live fixture

A newly predeclared five-minute read-only MT5 window passed account binding and
quote/OHLC availability checks, but contained no executable SMA(2)-short signal.
The reference checker now distinguishes verified no-trade behavior from exercised
execution coverage, persists per-case progress, checks all remaining cases and
keeps a nonzero incomplete-coverage result rather than declaring all eight passed.
Seven execution cases matched independent calculations on this fixed dataset.
The original failed attempt remains evidence; no date-range or freshness-limit
selection was made after seeing prices. See MT5_VALIDATION.md for hashes, exact
scope and the still-missing market-mode MT5/broker comparison.

Validation for reference coverage reporting: all 46 Python tests passed. The
synthetic eight-case reference fixture still passed with archive replay and
future-price invariance. The prospective real-data run deliberately returned
nonzero coverage_incomplete (seven exercised cases, one verified no-signal case).

## MCP backup protocol verification (2026-11-24)

At commit `c82703a`, a stable local `cargo test --workspace` run completed with
418 passed, zero failed and zero ignored tests across 91 summaries. The subsequent
`cargo build --workspace` exited successfully (one existing deprecation warning
in sqx-web). The separate-process backup test passed in debug and release: it
exports two candidates, deletes source archive/CSV, imports into a fresh archive,
checks missing retained engine bytes, reimports idempotently and replays both
results with the original MCP executable.

An earlier workspace attempt failed parsing a generation-resume error response
while another debug build was performed during execution. The test now reports
that protocol error explicitly. The same crash/recovery test passed when the
workspace executable remained unchanged. Concurrent rebuilding is a plausible
cause, not a proven diagnosis: the original response text was not retained.
The failed log remains evidence and is not replaced by the passing run.

Local log SHA-256 values (raw logs remain outside Git):

- `mcp-import-workspace-tests.log`: `34c3533585e5d0c13575c992d03539ed4d4215df7a48ac4ecafc79a204808f7d`

- `mcp-import-workspace-tests-stable.log`: `a2afc447cfaf32d18f34ceafb1d60ccd3d72c17fda2845214c70d89618011edb`

- `mcp-import-workspace-build-stable.log`: `cc22d8c41b483fb1dec30612211d4cd759039e0bf1578fbee991c6dcc1f3b607`


This verifies narrow backup/recovery fixtures, not representative-scale transport,
power-loss durability, UI recovery, independent MT5 execution or trading validity.
GitHub run 45399712550 for `9a1656f` was still running when this evidence was recorded.

## MCP backup scale probe (2026-11-24)

A frozen local debug executable (SHA-256
`2fb77f5cf641490b626dff72b7769fd8dbd539b0863d1145fcf13d082202707b`)
generated 64 candidates over 2,000 synthetic candles. Export retained all 130
records; after deleting that disposable source archive and CSV, import into a
new store, equal re-export, and replay of all 64 outcomes passed with no invalid
candidates or newly computed missing results.

The JSON backup was 74,611,326 bytes; the MCP response was 82,450,000 bytes.
Observed export/import/replay times were 16.545s / 58.498s / 22.805s. This reveals
a material transport scaling limitation: shared datasets are repeated per record.
It is a single local measurement, not a performance guarantee or peak-memory test.
Deduplicated transport remains required before large-search usability is claimed.
The full local report is `work/mt5-validation/backup-scale-64x2000/report.json`;
backup SHA-256 is
`681b87e25b7f308576ffe97c60195df8dd4d531ecdc8bb74a0bf9f6c569e4f15`.
This synthetic test does not establish MT5 parity, power-loss safety or untouched
holdout evidence.

## Compact MCP backup comparison (2026-11-24)

Schema 2 stores each shared dataset/strategy once under its existing content hash;
schema 1 remains importable. The 64-candidate/2,000-candle search above retained
all 130 run IDs after compact export/import/re-export, then replayed all 64
results using the original frozen executable, without creating missing results.
Backup size fell from 74,611,326 to 52,777,342 bytes (29.26%); MCP response size
fell from 82,450,000 to 57,493,244 bytes. Export/import/replay measured
18.111s / 59.108s / 22.718s on this run. Import speed did not improve materially.
Current validation still expands shared objects internally, so peak memory and
large full-result transport remain open work.

All 16 MCP unit tests and both separate-process integration tests passed in
release, including missing/altered/unreferenced shared-object rejection and
legacy import compatibility. This is not a new full-workspace or broker-parity
claim. Local report: `work/mt5-validation/backup-compact-scale-64x2000/report.json`.
Compact backup SHA-256:
`e89405f596dd3ae0128af74e751613bdb66ccb56a0854cec7629c35bf3f6e397`.
Exporter executable SHA-256:
`feb8f356e23d8a69b908787827b93c6661299a96ba89aa227a881bbe77f4ca20`.
