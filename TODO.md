# Reliability TODO

Status: research tool under validation, not independently certified for trading.
This checklist tracks remaining work; it does not replace the six completion
requirements and evidence audit in [RELIABILITY.md](RELIABILITY.md).

Local MT5 integration evidence and completion gates: [MT5_VALIDATION.md](MT5_VALIDATION.md).

User priority (2026-11-24): temporarily defer further MT5 work. Continue research
reproducibility, recovery, walk-forward/holdout and evidence visibility. Deferred
MT5 checks remain incomplete; this changes work order, not validation claims.

## P0 — Independently validate execution

- [ ] Resolve the observed USDJPY/EUR margin conversion-route discrepancy against
  MT5 references; distinguish margin from profit conversion and bid/ask policies.
  The opt-in forex-base margin policy now matches the frozen calculation samples;
  directional profit conversion is also implemented. Broader coverage and
  historical conversion-series validation remain open.
  See MT5_VALIDATION.md. Do not relax tolerances to conceal the difference.

- [ ] Obtain a reproducible independent fixture: market data, instrument/account
  settings, exact strategy and MT5/broker execution report with declared provenance.
- [ ] Compare orders, fills, trades and balances for long/short strategies,
  spread/commission/slippage, account-currency conversion, gap fills, SL/TP,
  trailing/break-even, margin rejection, stop-out and financing.
- [ ] Explain every discrepancy and preserve reference results as regression tests.
- [ ] Document which instrument/account/data/execution combinations have evidence.
  Start with a concrete end-to-end scenario, then expand coverage; this does not
  reduce the full programme's scope.
- [ ] Rerun historical databanks created by the earlier defective execution model
  after that model has been independently validated. Preserve provenance rather
  than presenting old rankings as current evidence.

Acceptance: independent results agree within explicit, justified tolerances;
remaining execution assumptions and unsupported cases are visible.

## P1 — Complete the simulation contract and evidence coverage

- [ ] Reconcile the full intrabar order/account lifecycle, not only sampled
  account values and final settlement arithmetic.
- [ ] Complete higher-timeframe confirmation and market-calendar boundary cases.
- [ ] State quote basis, event ordering and information availability explicitly.
- [ ] Distinguish local OHLC path scenarios from global adverse/favourable bounds;
  do not imply that candle data determines the true intrabar sequence.
- [ ] Maintain a coverage matrix for every advertised indicator, warmup/seed
  convention and supported execution feature, backed by independent references.
- [ ] Extend causal/future-perturbation and deterministic replay fixtures to the
  remaining supported combinations.

## P1 — Make research provenance complete

- [x] Preserve and reverify optional clock-normalization bundles for browser CSV imports and archived web backtests. Display missing evidence and the limited verification scope.
- [x] Persist browser CSV imports as immutable versions and provide explicit reload after dashboard restart, including optional calendar and normalization evidence.
- [ ] Carry full recording and conversion-source provenance through every research route.

- [ ] Provide durable trial accounting on every research entry point, including
  CLI, MCP, pipelines, optimization and robustness operations.
  CLI pipelines retain exact engine bytes, frozen inputs/configuration/seed,
  a pre-execution start record, ordered stage/family events, per-build/retest
  starts and outcomes, and the full returned report before reporting errors.
  Storage failures stop further evaluation. A process-kill/source-deletion
  restore test preserves the frozen family and completed build outcomes without
  fabricating completion. Restored event/start/result records disclose holdout
  exposure; counts are records, not independent trials. Optimization checkpoints
  now include every walk-forward candidate and OOS start/full execution outcome. CLI `pipeline-history` checks
  prefix links; `replay-pipeline --resume` replays the prefix with the exact
  engine before adding missing events, under an exclusive process lock. A
  separate-process kill/restore/resume fixture matches an uninterrupted search;
  repeat recovery deduplicates and conflicting/tampered prefixes are rejected.
  Whole-search JSON export/import now validates checksums, event links and the
  merged destination graph before publication; repeat imports deduplicate.
  Tests reject corruption, missing completion dependencies and conflicting
  destination events. A separate process-kill test inside walk-forward restores through the public
  backup commands and matches all 48 candidate outcomes and full event identities
  against an uninterrupted run. Engine/runtime transport, bounded-scale
  validation and power-loss validation remain open. Dashboard pipeline inspection,
  whole-backup transfer and exact-build recovery now use the shared CLI archive
  implementation; broader recovery UX and large archives remain open.
  Standalone CLI `optimize` now uses the same archive protocol, retaining each
  window's separately generated candidate family without changing its seeded
  search semantics. Its process-kill/backup/source-deletion recovery test matches
  all 32 candidate outcomes and the completion identity of an uninterrupted run.
  This increment is on the optimize-recovery branch, not yet released on main.
- [x] Preserve MCP generation execution failures in responses, exclude failed candidates from ranking and mark partial searches as errors. Durable generation accounting is implemented below; other MCP routes remain open.
- [x] Stop MCP portfolio selection on failed or missing candidate evaluations and preserve error/selection context.
- [x] Persist MCP generation inputs, per-candidate checkpoints and full results before ranking; inspect interrupted searches after restart without original CSV files. Replay and resume are tracked below; other MCP routes remain open.
- [x] Replay or explicitly restart one archived MCP generation candidate with engine/search/input relationship checks, preserving conflicting outcomes and incomplete search status.
- [x] Resume a frozen MCP generation search under an exclusive process lock, replaying stored outcomes before completing missing candidates; preserve conflicts and deduplicate repeated recovery.
- [ ] Retain successful, rejected, failed and interrupted trials before selection;
  distinguish archive record counts from independent trial counts.
- [ ] Independently review walk-forward boundaries, warmup, capital continuity,
  minimum evidence thresholds and untouched-holdout handling across all routes.
  A separate integer-price SMA3 fixture now checks segmented selection, warmup,
  costs and carried capital. A hand-calculated EURUSD/USD holdout fixture checks
  SMA3 warmup, first-boundary fills, long/short spread and commission, fresh
  holdout capital, future-data isolation and persisted commitment after restart.
  Broader configurations, independent platform agreement and untouched-data
  provenance beyond the local protocol remain open.
- [x] Classify all MCP generation phases in holdout exposure audits, keep record counts separate from trial counts, and preserve committed exposure after restore.
- [x] Include all markets from archived CLI crosschecks, including checkpoints and failures, in holdout exposure audits and restored commitment evidence.
- [x] Audit complete declared candle intervals across experiment, crosscheck, backtest, comparison and evolution evidence; surface missing legacy durations and retain uncertainty through restore.
- [ ] Make a selected strategy traceable to its full search/selection history.
- [ ] Export and restore a complete MCP generation search, including the frozen
  candidate family, every checkpoint/result (including rejected and failed
  candidates), and completion links. A single-record download is not this backup.
  MCP `generation_export` now exports under the existing search lock and checks
  linked identities; `generation_import` validates the entire merged graph before
  publication. Source-deletion restore, original-engine replay, conflicts and repeat
  imports are tested. Scale, disk/power-loss and UI validation remain open.
  Validate content identities and
  cross-record relationships before publishing restored runs. Reject missing or
  conflicting linked records, retain incomplete-search status, and test recovery
  after deleting the original store and CSV. Keep executable transport explicit;
  a JSON backup must not silently install or execute stored engine bytes.

## P1 — Establish remote release gates

- [x] Activate `.github/workflows/reliability.yml`. Published through the connected
  GitHub app in e832c2f; first run 45357592150 passed all four jobs. The older Git
  OAuth workflow-scope limitation did not apply to this app connection.

- [x] Verify baseline GitHub Actions run 45357592150 for Linux/macOS and
  debug/release, including profile parity. Commit: e832c2f.
- [x] Push and independently verify the expanded MT5/reference changes. Commit
  156c0d7 passed all four jobs and both profile comparisons in run 45358671142.
- [x] Verify recorder commit ed76833 in GitHub run 45362906500: all four jobs and profile comparisons passed.
- [x] Resolve the macOS mock-provider nonblocking read failure exposed by run
  45364814462. Fixed in fdee568; replacement run 45365576256 for 68a1319 passed
  all four jobs and both profile comparisons.
- [x] Verify history-availability retry commit 44992ab in run 45369266431; all four platform/profile jobs and both profile comparisons passed.
- [x] Require Linux/macOS debug/release checks on `main`, bound to GitHub Actions
  (app 15368), with strict up-to-date enforcement, administrator enforcement and
  force pushes disabled. Repository protection API reverified on 2026-11-24.
  Individual revisions still need successful checks before merging.
- [x] Retain exact engine bytes before creating new CLI evolution jobs; refuse creation on corrupt retained bytes and test separate-process resume.
- [ ] Establish a versioned release/artifact retention procedure: archive replay
  requires the exact recorded executable, not merely compatible source code.
  `scripts/retain_engine.py` now provides hash-checked local byte retention;
  new CLI backtest/assistant/crosscheck and dashboard archive/experiment/evolution
  creation routes now retain engine bytes automatically. Remaining routes, legacy
  resume, runtime/environment packaging and durable release artifacts remain open. See ci/README.md for the manual procedure.

## P2 — Validate exports and paper trading

- [ ] Check exported strategy semantics and execution against the source strategy
  for each advertised target and supported feature combination.
- [ ] Reconcile a real paper-trading dataset against archived strategy results.
- [ ] Explain fill, timing, costs and financing differences using explicit
  tolerances; matching results are not a profitability guarantee.
- [ ] Keep unsupported exports and insufficient evidence visibly unresolved.

## P2 — Finish operational reliability and usability

- [ ] Bound retained full-result memory during large AI research sessions.
- [ ] Implement provider-conversation/session recovery; current recovery restarts
  individual frozen backtests, not the AI conversation or unsaved future work.
- [ ] Test interruption during an AI engine calculation, not only while awaiting
  the next provider response; define the scope of power-loss guarantees.
- [ ] Complete the evidence viewer and test rendered recovery flows, keyboard
  navigation and narrow-screen layouts.
- [ ] Validate long-running research and backup/restore at representative scale.
  A 64-candidate/2,000-candle synthetic MCP export/import/replay passed, but
  produced 74,611,326 backup bytes (82,450,000 protocol bytes) and took 58.498s
  to import on the local Mac. Version 2 now deduplicates shared objects: the same
  backup is 52,777,342 bytes (29.26% smaller), but import still took 59.108s.
  Bound internal memory and improve large-result transport/validation before
  claiming large-search usability; retain all outcomes and exact hashes.
- [ ] If remote hosting is introduced, implement and test authentication and access
  control first. The supported server currently restricts access to loopback.

## Current verified baseline

- Local debug and release workspace gates: 379 Rust tests, 16 frontend scripts,
  twelve Python tests, independent eight-case arithmetic/future-mutation checks
  and successful workspace builds. Remote Linux/macOS gates passed for 156c0d7.
- Additional recovery tests passed after the final memory/test-isolation changes.
- Earlier debug/release parity fixtures cover four execution scenarios and two
  seeded evolution runs; this is internal determinism evidence, not broker parity.
- Frozen CLI and AI-trial inputs/results support archives, backup/restore and
  exact-engine replay; AI checkpoints can restart one trial without provider calls.
- Internal execution/account checks and per-trade explanations are implemented.

Do not mark this checklist complete solely because existing tests are green.
See [README.md](README.md) for operation and [ci/README.md](ci/README.md) for checks.
