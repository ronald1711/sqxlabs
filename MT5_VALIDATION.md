# MT5 validation workstream

Goal: a reliable private strategy-development/testing workflow, beginning with
read-only access to the local SiliconMetaTrader5 installation. The full six-step
requirements in RELIABILITY.md remain applicable. Green internal tests and one
validated scenario are not completion of this goal.

## Initial observed configuration

- Runtime reports version 1.3.0; installed Python package metadata reports 1.2.3.
- Local bridge: 127.0.0.1:8001; terminal build 6328.
- Broker server: TickmillEU-Demo; demo account, account currency EUR, leverage 30.
- Initial markets: EURUSD and USDJPY; both already visible in Market Watch.
- Read-only API calls used: terminal status, account/symbol information, latest
  tick, closed M1 candles and a bounded historical tick range. No order placement,
  account login changes, symbol selection or runtime restart was performed.

## Evidence collected

Local private evidence is under work/mt5-validation (not committed): raw captures,
probe-report.json and validation-report.json. Each market supplied 120 closed M1
candles; the one-minute tick samples contained 47 EURUSD ticks and 82 USDJPY ticks.
All sampled candles passed OHLC bounds, ordering and contiguous-minute checks.
Sampled ticks passed bid/ask and seconds/milliseconds consistency checks.
EURUSD contained a pair of ticks sharing one millisecond: time alone cannot be
used as a unique tick identifier.

Both latest tick timestamps were about 7,199 seconds ahead of the host clock,
consistent with a current broker clock offset of +02:00. This is an observation,
not proof of a permanent offset or a historical DST conversion rule. Raw source
timestamps are retained. Do not ingest these values as proven UTC or subtract a
constant two hours across history without independent clock evidence.

The exporter now closes only its own client with close(), including initialization
failure, instead of shutting down the shared terminal connection. Five offline
exporter tests pass. A live two-client check confirmed that the second initialized
client could still read ticks after the first closed, without reinitialization.

## Completion gates

- [ ] Data integration: versioned read-only collector, explicit source-clock policy,
  historical offset/DST evidence, instrument metadata, raw and normalized records,
  duplicate/gap checks and clearly declared coverage. Current small samples do
  not prove full tick history or long-period data quality.
- [ ] Independent execution: define supported market/account/execution settings;
  compare simple strategy signals, fills, costs, FX, margin and financing against
  independent reference calculations and execution records. MT5 calculation APIs
  may provide read-only references; they do not replace actual fill evidence.
- [ ] Reproducible research: immutable inputs/configuration/results, all-candidate
  accounting, independently reviewed walk-forward and holdout boundaries.
- [ ] Live collection and paper comparison: durable progress, reconnect/backfill,
  bounded storage, stale-data visibility, explainable trade correspondence.
- [ ] Operations: activate CI (workflow permission still pending), bound resource
  use, display evidence state and limitations and preserve replayable versions.
- [ ] Expand verified coverage across the advertised scope. Do not extrapolate
  the first broker or market scenario to every instrument or execution feature.

Next: establish a trustworthy timestamp mapping and collect bounded reference
snapshots for profit/margin calculation on the observed EUR demo configuration.
No live or demo orders are authorized by the read-only connection phase.

## Read-only MT5 profit/margin references

Eight calculation calls were captured for EURUSD and USDJPY, one lot, both
buy/sell and positive/negative price movement, on the same EUR/1:30 demo account.
Local full evidence: work/mt5-validation/calculation-reference.json. No orders
were submitted. Quotes before/after the calls and host timestamps are retained.

The EURUSD subset is versioned under
crates/sqx-backtest/tests/fixtures/mt5_eurusd_eur_calculation.json. Its regression
runs synthetic flat-bar scenarios at the supplied calculation prices and checks
profit, final equity and entry margin against MT5's returned EUR-cent amounts
with a half-cent rounding tolerance. All four cases pass. This is independent
calculation evidence, not observed fills, spreads, commissions, financing, signal
timing or general broker execution parity. Targeted test log:
work/mt5-validation/mt5-reference-tests.log. No full workspace gate was rerun.

### Open discrepancy: USDJPY margin with an EUR account

Observed MT5 margins: EUR 3057.40 (buy) and EUR 3056.95 (sell) for one lot.
These agree to cents with 100000 / 30 / EURUSD bid (buy) or ask (sell) in the
captured quotes. Our current margin formula uses the traded price multiplied by
the same quote-to-account conversion rate used for profit. With EURJPY bid as
that rate, the frozen snapshot gives about 3057.54/3057.21 instead. Using a single
conversion path for both purposes therefore has not established broker parity.

The matching direct-base route is an inference from these snapshots. Before
implementing a broader broker-compatible policy, capture more directions, sizes,
calculation modes and currencies. Margin conversion and profit conversion need
separate explicit semantics; do not hide this discrepancy by enlarging tolerance.
USDJPY profit snapshots also distinguish the sampled EURJPY bid/ask paths by
trade direction. Existing historical-rate configuration has only a single rate,
so directional conversion spread needs explicit investigation too.

Next work: define and test explicit conversion/margin policies using frozen
independent references, while retaining the existing model as a clearly labelled
assumption where parity is unproven. Historical source-clock/DST mapping remains
open; no constant offset was applied to history.

## Explicit forex-base margin policy

The engine now supports opt-in `margin_model: "forex_base"`. For third-currency
accounts it uses separately supplied historical base-to-account conversion
observations, with long/short rates and the existing maximum observation age.
The old `quote_notional` convention remains the default and is explicitly not a
claim of broker parity. No broker snapshot is silently wired into production
settings or treated as a historical rate series.

The new margin policy is used consistently for entry admission, sampled account
margin and the stop-out threshold/settlement decision. Evidence records the
margin base-to-account rate separately from the profit quote-to-account rate,
and the UI includes the new field in admission, account and stop-out details.
Missing/future/stale rates fail; duplicate observations and invalid rates fail
configuration validation. The constant-base-margin stop-out equation has distinct
long and short regression cases.

Four frozen USDJPY/EUR MT5 calculations now reproduce profit and margin within
half a EUR cent with explicit fixture rates derived from the captured EURJPY and
EURUSD quotes. Tests also check that the sampled margin stays constant as the
USDJPY price changes with unchanged base conversion. The reference fixture is
crates/sqx-backtest/tests/fixtures/mt5_usdjpy_eur_calculation.json. These isolated
single-direction tests configure the appropriate profit rate explicitly: general
directional profit conversion for a mixed long/short strategy remains open.

This addresses the observed margin-route discrepancy under an explicit opt-in
model; it does not establish all broker margin modes, account-level rounding,
actual fills, commissions, swaps or historical clock correctness. The broker
references remain independent calculation samples, not a trading certification.

Reference API scope was checked against MetaQuotes documentation:
https://www.mql5.com/en/docs/python_metatrader5/mt5ordercalcmargin_py
and https://www.mql5.com/en/docs/python_metatrader5/mt5ordercalcprofit_py .
Margin estimation excludes existing pending orders/open positions; neither API
proves an actual execution. No hedge/portfolio margin parity is inferred here.

An additional supplied-tick stress test changes only the historical margin
conversion rate at 120500ms. It verifies admission used the earlier rate and
stop-out occurs at the first tick with the new rate, at the actual quote, with
no artificial cash charge or prior knowledge of the later observation.

Verification: both full workspace gates passed 372 Rust tests, 16 frontend checks,
five Python exporter tests and builds. Four execution/two evolution profile-parity
fixtures passed. The subsequently added tick case passed in targeted debug and
release runs (three margin-model tests each). No real orders were placed and no
new rendered UI verification was performed.

## Directional profit conversion

AccountConfig now has an explicit `profit_conversion_model` (`single_rate` by
default, or opt-in `directional`) and separate historical
`profit_conversion_rates`. A directional observation declares account units per
quote-currency unit for long and short positions. It is selected by position
direction, not by the sign of profit. Direct quote-account and base-account
conversion retain their established one/reciprocal-price conventions. No broker
bid/ask routing is silently inferred by this API.

Engine sizing/admission reserves, open-position valuation, exit settlement,
stop-out threshold and stop-out evidence now consistently use the position's
profit conversion. Margin has its own policy/rates; default quote-notional margin
uses the selected directional profit rate, while forex-base margin remains
separate. Directional mode rejects missing/future/stale observations even when
legacy single-rate observations are present. Invalid/duplicate observations fail
validation. The directionless quote_rate API rejects an external directional
conversion rather than silently picking one side.

The frozen USDJPY/EUR reference tests now supply BOTH directions in one account
configuration, instead of manually choosing a single rate per direction. A mixed
long/short strategy verifies nonzero trades and cash/account evidence for both
sides. Separate tests cover causal rate lookup, independent long/short stop-out
thresholds and a tick-time profit-conversion change that triggers liquidation
only at its first available tick. The latter keeps margin conversion unchanged,
so the two economic effects are explicitly separated.

This resolves the earlier implementation gap for mixed-direction supplied rates.
It does not establish a broker-independent rule for selecting conversion quote
sides, validate historical conversion data/DST, or certify actual fill behaviour.
The earlier snapshot and model limitations remain applicable.

## Broker clock evidence for the next data-normalization step

Tickmill's dated 2026 announcements specify a move to GMT+3 on March 8 and back
to GMT+2 on November 1, following New York DST. This is stronger evidence than
assuming the European transition dates from a generic timezone label. Sources:
https://www.tickmill.com/eu/news/daylight-saving-time-in-the-us-trading-schedule-changes-2026
https://www.tickmill.com/eu/news/end-of-daylight-saving-time-in-new-york-trading-schedule-changes-2026

The observed current +02:00 offset is consistent with the November announcement.
The notices state dates, not precise clock-transition instants for every server.
A historical conversion policy must preserve its sources/version and reject
unresolved transition intervals instead of guessing an exact switch hour. No
historical data was normalized in this increment, and no European-DST rule or
constant two-hour correction was silently applied.

Directional-conversion verification completed: both full workspace gates passed
376 Rust tests, 16 frontend checks, five exporter tests and builds; four execution
and two evolution debug/release comparisons passed. Logs are
work/mt5-validation/directional-debug-gate.log, directional-release-gate.log and
directional-parity.log. No orders were submitted in this increment.

## Explicit offline timestamp normalization

scripts/mt5_normalize.py converts exporter-compatible raw candles/ticks using a
server-bound interval policy. The bounded example examples/tickmill-clock-2026.json
uses the dated broker notices for supported 2026 intervals and rejects entire
transition dates whose exact switch hours were not established. It is not a
permanent timezone rule or a policy for other brokers/servers.

Every published bundle contains raw.csv (original bytes), policy.json (original
bytes), utc.csv and a hash-linked manifest. A content-derived directory is
published after all four files are written; repeats verify existing contents.
Conversion refuses raw or normalized interval overlaps, unsupported timestamps,
non-increasing candles/reversed ticks and candles crossing policy boundaries.
Equal-millisecond ticks remain separate and retain source order and price text.
Source CSV is unchanged. No live connection or account mutation is involved.
This validates timestamp conversion only, not prices, full tick coverage or
cryptographic authenticity of the declared broker identity. Directory publication
has not been power-loss tested.

The previously captured EURUSD/USDJPY samples were converted: 120 candles and
47/82 ticks respectively, with unchanged row counts. Converted candles are closed
and tick timestamps are not future-dated relative to their original host capture
window. Local report: work/mt5-validation/normalization-report.json. These samples
were collected in different bar/tick windows; do not treat them as a complete
aligned tick backtest dataset. Nine Python tests (exporter plus normalization)
pass, including failed-write cleanup and existing-bundle integrity checks.
No Rust source changed in this increment; full workspace gates were not rerun.

Next: integrate bundle provenance into the research import/archive path, prevent
accidental use of unnormalized broker CSV, and collect aligned historical windows.
The current offline command does not yet make raw CSV import automatically safe.

## Raw clock headers fail closed at import

The exporter now writes timestamp_broker for candle seconds and
timestamp_broker_ms for ticks. Rust CSV ingestion rejects either header with an
explicit normalize-first error, covering the common data import functions rather
than relying on the user to notice future-dated bars. The normalizer accepts
these raw clock headers and emits the existing UTC timestamp/timestamp_ms schema.
It refuses ordinary UTC headers by default, preventing accidental double
normalization. For older exports only, --legacy-broker-clock explicitly declares
that the otherwise ambiguous header actually contains raw broker time; the
manifest records that declaration. It is not appropriate for already-normalized
files. Policy and raw bytes remain preserved.

An end-to-end regression runs the real Python converter then reads its output
with the Rust bar/tick parsers. It proves raw inputs fail, UTC timestamps convert
by the declared offset, millisecond ties retain both ticks, prices retain their
precision and source bytes are unchanged. Ten Python tests and all sqx-data tests
pass. This is not full provenance-aware import: generic old/headerless CSV can
still carry misdeclared time, and automatic bundle discovery remains unfinished. Explicit CLI bundle attachment
is now supported as described below.

## Verified bundles in CLI source context

sqx-data now independently verifies normalization bundle hashes and semantics:
server/schema binding, raw/normalized interval order and overflow, original CSV
payload preservation, exact row-by-row timestamp transformation, row counts,
chronology, candle-boundary containment and parsed bar/tick structure. Rehashing
an altered UTC file cannot conceal an incorrect conversion. The bundle retains
all original UTF-8 texts, including the policy and manifest bytes used for hashes.

CLI SourceOptions exposes --normalization-bundle for candle research. Its audit
binds verified normalized candles and duration to the actual input/timeframe, then
embeds the bundle and verification record in source context. Existing archival
routes preserve that source context. Backtest replay/resume independently checks
the embedded bundle, its relation to archived candles, and verification metadata.
The integration test restores an archived run after deleting source files and
proves replay works; wrong candles/duration and contradictory archived metadata
are rejected. Tick bundles can now be attached with --tick-normalization-bundle together with
--ticks. Exact sequence checks reject reordered or missing same-millisecond ticks.
Replay verifies the embedded tick bundle and metadata after source removal.
Cross-check manifests also support per-market normalization_bundle paths and
retain the verified source context. Dashboard ingestion/data manifests remain open.
No default verification is claimed for ordinary CSV without an attached bundle.


## Common-window read-only capture

A reusable local collector now stores EURUSD/USDJPY/EURJPY history for identical
explicit broker-clock bounds, without symbol selection or order/account changes.
Actual capture 8adc8fa417cca415dccd2417c36d146116632dcfc4d9ada0bf40157fefb33e25
covers [1795515180,1795515480) broker seconds: five M1 candles per symbol and
340/360/817 in-window ticks respectively. Full tick responses retain observations
at the requested inclusive endpoint separately. Data is under
work/mt5-validation/aligned-captures. Conversion freshness and historical tick
completeness have not yet been certified; this is not an execution reference.
The real API exposed an exporter bug: NumPy structured rows reject slices.
Bar serialization now uses scalar field indexing, exercised by the live capture
and a slice-rejecting regression fixture. All ten Python tests passed in
aligned-capture-python.log. Collector error-path/atomic-publication tests and
integration with normalized archived backtests remain follow-up work.


## Causal conversion coverage audit

The original aligned capture's bid ticks reproduce all 15 reported M1 OHLC
candles exactly. This is consistency evidence, not proof of complete ticks.
Two USDJPY events lacked any preceding conversion quote in that bounded window.
The collector now requests 60 seconds of preceding ticks by default (configurable
0–3600), preserving them separately from the half-open trading window and the
complete inclusive API response. No future rate is substituted.

A fresh read of the same bounds produced capture
09726cd6dd29b546c6d153d7efeae9d2b046cdf479cda3ecd21c321ed81ae9e2.
For all 365 USDJPY candle-open/tick events, the latest EURUSD/EURJPY observation
at or before the event exists. Maximum ages are 5847/5853 ms respectively;
53 EURUSD and 150 EURJPY warmup ticks were retained. Equal timestamps mean
causal ordering across different symbols remains unresolved at sub-millisecond
precision; this audit uses an explicit <= timestamp convention. A maximum
permitted age remains an execution-policy decision, not a proven broker rule.
Twelve Python tests passed (capture-warmup-tests.log), including response endpoint
partitioning, same-time tick order, missing candles, changed account and immutable
capture integrity. Applying these rates in archived strategy runs remains next.


## Captured-data independent arithmetic comparison

scripts/mt5_reference_check.py reproduced long and short USDJPY trials on capture
09726cd6dd29b546c6d153d7efeae9d2b046cdf479cda3ecd21c321ed81ae9e2.
Separate Python Decimal arithmetic matched entry/exit price and time, commission,
profit, ending equity and entry margin within 1e-8. Both archived runs replayed.
Expected net results: long +1.0302384361887 EUR; short -1.9602596543681 EUR.
Evidence: aligned-reference-final.log and the capture's reference/comparison.json.
The first comparison detected a point-unit mismatch (engine pip 0.01 versus MT5
broker point 0.001). The requested slippage was corrected to 0.1 engine point;
the independent expected price and tolerance were not relaxed. The initial
mismatching run remains in the local archive.
These are constant-entry, fixed-volume, terminal-liquidation trials on five
candles. Costs and conversion side policies are declared assumptions; no actual
broker fills, stop paths, indicator signals or overnight financing are certified.
Conversion source bundles remain beside the run; automatic full conversion-source
provenance attachment to archives is unfinished. Broader regression fixtures and
remaining reliability requirements remain open.


## SMA signal timing on captured ticks

The independent Decimal checker now also evaluates close > SMA(2) for long and
close < SMA(2) for short. SMA is calculated directly from the two closing prices;
the first candle is ineligible. The first qualifying zero-based signal indices
are 1 and 2 respectively; fills match the first tick of the next candle, not the
signal candle. Recorded condition operands match the independent close and mean.
Margin, costs, conversion and terminal equity also match: SMA long net
+0.6851788271412 EUR; SMA short -1.7302225608874 EUR. All four constant/SMA cases
and exact archive replays passed (aligned-sma-reference.log).
This is one indicator period on five candles, not validation of the indicator
catalog or general proof of no look-ahead. Stop/trailing behavior, signal exits,
additional market conditions and actual broker execution remain unverified by
this captured-data reference suite.


## Captured-tick stop reference

The Decimal suite now includes 0.4-pip long/short stop cases, capped at one entry
per day. The independent reference scans observed ticks in order and applies
adverse slippage to the first executable stop quote. A 0.4-pip distance was chosen
to exercise an observed short-side quote jump, not to optimize strategy returns.
Short entry 159.364 gives stop 159.368; the first crossing ask is 159.369 and the
fill after one broker-point slippage is 159.370. Long entry 159.366 gives stop
159.362 and fill 159.361. Net changes are approximately -0.6950893781 EUR (short)
and -0.6375910226 EUR (long). All six cases and archive replays passed in
aligned-stop-reference.log. Commission, margin and conversion checks remain active.

The initial independent comparison exposed binary float serialization residue at
an exact stop boundary. Executable USDJPY quotes in the reference are now rounded
to the declared 0.001 broker price grid before threshold comparison. Tolerances
were not relaxed. This checks observed tick stops in this sample, not general
bar-path ambiguity, trailing/break-even, latency or actual broker stop fills.


## Captured-data signal exits

The reference suite now includes constant exit signals for long and short, with
one entry per day. Entry follows candle 0; the first exit decision while holding
is at candle 1 close; the independently expected fill is the first tick of candle
2. Both recorded exit reasons are ExitSignal, and fill prices/times, commission,
conversion, margin and final equity agree with the Decimal reference. All eight
cases and archive replays passed (aligned-signal-exit-reference.log).
The checker records executable/checker hashes, marks new attempts incomplete
before execution and refuses optimized Python, which would disable assertions.
This tests constant exit signals on this sample, not all exit conditions or
sub-millisecond event ordering. General no-look-ahead and broker parity remain
broader requirements.


## Future-data mutation comparison

scripts/mt5_prefix_check.py independently creates a synthetic derivative of the
frozen capture by shifting the last USDJPY candle/ticks +0.050. The original
capture is unchanged and synthetic provenance is explicit. All eight cases are
rerun and independently checked on both inputs. Earlier entry attempts (including
signal evidence), cash settlements and tick-timed account snapshots match exactly
before the mutation cutoff; nonempty account evidence is required. At least one
terminal result must differ so the mutation cannot be vacuous. The run passed:
work/mt5-validation/prefix-reference.log and prefix-derived/prefix-comparison.json.
This covers one late price mutation and the eight declared strategies, not the
whole indicator catalog, higher-timeframe logic or every look-ahead pathway.

## Restartable recording journal

A local append-only journal now binds recording configuration, window continuity,
capture hashes and account context. Capture files are published before commit
records; the cursor is derived from committed records rather than a separately
mutable offset. OS locking allows one writer and releases after process exit.
Tests cover corruption, conflicting configuration/account, missing middle windows,
write failure and actual child-process exit after blob publication before commit.
All 15 Python tests passed (capture-journal-tests.log).

The read-only recording CLI captured two one-minute windows from the existing
historical sample for EURUSD/USDJPY/EURJPY. A separate invocation resumed at the
third window: [1795515300,1795515360), without repeating the first two. Logs:
recording-first.log and recording-resume.log; journal: recording-journal/.
--follow supports bounded waiting for closed windows, but this real-MT5 evidence
is historical catch-up/restart, not a long-running live-stream validation.
Automatic reconnect, calendar-aware gaps, final-record deletion detection, power
loss, live interruption and downstream papertrading comparison remain open.


## Bounded reconnect and first live closed-window capture

The recorder reconnects only for ConnectionError/TimeoutError/EOFError raised by
bridge reads/initialization. Default three retries with capped backoff; success
resets the retry counter. Data validation and journal/storage errors escape the
retry loop. Tests prove same-window retry, one commit, preserved cursor on failure,
closed own clients, retry exhaustion and waiting until a window closes. All 18
Python tests pass (recorder-reconnect-tests.log). These transport failures are
injected tests, not deliberate disruption of the shared user's MT5 bridge.

A real --follow invocation observed an open minute, reported waiting without
advancing, then captured [1795517760,1795517820) broker seconds for EURUSD/USDJPY/
EURJPY after closure. Committed capture:
2767f12d05306d02eaccd437e257d774f353368e3ed7c7b2bc02d3bda2adac85.
Evidence: live-recording-first.log and live-recording-journal/.
This establishes one live window, not long-duration completeness, actual network
outage recovery, power-loss durability or papertrading reconciliation.

## Account context across reconnects

The recorder now reads server, account currency, leverage and trade mode before
reading each window (including after reconnect), comparing them with the committed
journal context or the initial context of the current invocation. A changed
context stops recording before fetching history; the acquired payload must also
match that context before publication. This compares economic context, not account
login identity. Nineteen Python tests pass, including a transport failure followed
by a changed account currency: no second history request and no cursor advance.
The real local journal resumed successfully at [1795515360,1795515420) with the
unchanged context (recording-account-resume.log). The user's account was not changed
to exercise the negative case; that transition is covered by an injected test.

## Frozen recording configuration

The journal now snapshots configuration bytes at construction and returns copies
of its configuration. Caller mutations, including nested symbol-list changes,
cannot silently alter an active writer. Every verification also checks the stored
configuration bytes; a changed file stops publication before a new record is
committed. The regression mutates both caller data and the on-disk configuration
and confirms the original window contract and record count remain intact.
All 20 Python tests pass (recorder-config-tests.log). This detects configuration
changes; it does not add authentication or detect deletion of a final record.

## Missing recording configuration

Opening a nonempty recording directory without config.json now fails instead of
recreating configuration from current command-line arguments. This prevents a
missing context file from silently becoming a new recording, including when the
caller proposes a different start time. The test confirms no replacement file is
created and that restoring the exact original configuration permits continuation.
All 21 Python tests pass (recorder-missing-config-tests.log). A genuinely new empty
directory, or only a stale temporary initialization file, can still initialize.

## Real live-recorder interruption and restart

A controlled probe started the recorder against the local MT5 bridge and observed
waiting_for_closed_window. It verified that the recorder process was still alive,
terminated only that process, checked that no window record existed, and launched
a new recorder with the identical configuration. After the minute closed, exactly
one record for the original pending window was committed. The shared MT5 bridge
still answered another client's quote request without reinitialization afterward.
Evidence: work/mt5-validation/live-restart-probe.log and
live-restart-probe/result.json. This verifies one real recorder-process interruption
while awaiting live data, complementing the injected between-write failure test.
It does not establish broker outage recovery, long-duration tick completeness,
power-loss durability, or papertrading parity. No orders or terminal shutdown were
used by the probe.

## Instrument-context continuity

The journal now requires captured instrument specifications and compares them
across all committed windows and each new append. A changed contract/tick/volume
or currency specification rejects the next window, including after restart.
The collector additionally freezes each symbol's specification before history
reads and compares it after reading, rejecting changes during acquisition.
Tests cover both within-read drift and a changed contract size after reopening.
All 23 Python tests pass (recorder-spec-tests.log). The real existing journal
also resumed with unchanged specifications (recording-spec-resume.log).
This compares the captured metadata fields; it does not certify historical
instrument settings for dates before those metadata observations.

## Recording-to-research export

The offline exporter verifies the locked journal and exports a specified range of
whole committed windows (maximum one hour). It embeds original captures and
commit records, concatenates main ticks without timestamp deduplication and keeps
only the initial selected warmup. Corrupt input fails before output publication.
Tests cover subset boundaries, missing/uncommitted ranges, same-millisecond tick
order, original endpoint responses, idempotency and corrupt source rejection.
All 25 Python tests pass (recording-export-tests.log).

The five recorded historical windows [1795515180,1795515480) exported as capture
f98bcd58bb601a265af2471f5028dc7dec8ba3bb63a4a84ca22bc4cb7ebfbc24 under
work/mt5-validation/recording-exports. All eight independent reference cases and
archive replays passed on that export (recording-reference.log). This connects
recorded chunks to reproducible research, but the full conversion/source recording
provenance is not yet automatically embedded in every engine archive or dashboard
route. Export currently requires stopping the writer; online snapshots remain open.


## Export snapshots while recording continues

The exporter now reads a finite snapshot of atomic commit-record filenames,
verifies their capture hashes/continuity/account/specifications, and retains the
verified payloads rather than rereading them for assembly. Configuration is
checked before and after scanning. It does not take the writer lock or publish
configuration. The single-writer rule remains unchanged. Newly published records
outside the snapshot are considered only by a subsequent export; orphan capture
blobs never advance the snapshot horizon. A concurrent or external inconsistency
fails validation rather than producing a partial dataset.
The active-writer regression publishes an orphan blob, rejects its uncommitted
range, exports the committed prefix, commits the next window and confirms the
first immutable export remains unchanged. All 26 Python tests pass
(recording-snapshot-tests.log). This supersedes the earlier stop-writer restriction;
long-duration/concurrent load and power-loss validation remain open.

## Separate-process recording/export validation

A regression now holds the writer lock in a separate live child process. The
reader exports the first committed window while that writer remains alive,
rejects the still-uncommitted second window, then requests its publication and
exports both without stopping the writer. The earlier export remains a one-window
snapshot. Handshake reads have explicit timeouts and the test cleans up its child.
All 27 Python tests pass (recording-process-snapshot-tests.log). This tests
cross-process publication boundaries, not sustained throughput or power failure.

## Recorder failure reporting

Expected data, transport and filesystem failures now produce a structured
recording_failed diagnostic and nonzero CLI exit instead of a raw traceback.
An end-to-end subprocess test provides an initialized read-only bridge stub that
returns no history. It verifies explicit failure, no reconnect loop, no success
receipt, no committed window and an unchanged journal cursor after reopening.
All 28 Python tests pass (recorder-empty-history-tests.log). A failed response is
not interpreted as an empty-but-complete market interval or silently skipped.

## Instrument metadata observation times

New captures explicitly timestamp both symbol-info reads around history acquisition
in host UTC milliseconds. The metadata scope states that these are current terminal
observations, not proof of historical specification applicability. Recording exports
retain one observation record per selected window; older captures without these
fields produce null entries instead of inferred dates. Observation timestamps are
outside the economic specification object, so successive observations do not trigger
false instrument-change rejections. All 28 Python regressions pass
(spec-observation-tests.log). Historical contract-specification validation remains open.

## Read-only recording status

The status command uses the verified committed snapshot path without opening a
writer lock or MT5 connection. It reports committed coverage in broker time and
next cursor, with explicit non-certification of liveness/tick completeness. A
subprocess regression checks the report while a writer holds its lock, compares
all file contents before/after, then corrupts a capture and requires nonzero exit,
no success output and integrity=not_verified. All 29 Python tests pass
(recording-status-tests.log). The actual historical journal reports five committed
windows [1795515180,1795515480) (recording-status.json). Dashboard display and
active-process monitoring remain separate unfinished work.

## Export-range validation

Snapshot range validation now lives in the journal API, so callers cannot bypass
whole-window, integer, start-boundary and one-hour limits. The exporter validates
stored configuration before range arithmetic; a zero window width now fails with
a configuration error rather than division by zero. Tests confirm rejection
without output publication or cursor changes. All 31 Python tests pass
(recording-range-tests.log). This is validation coverage, not a scale guarantee.

## Dashboard clock-evidence import and archive binding

The CSV importer accepts the four exact files from one normalization bundle
(manifest.json, policy.json, raw.csv, utc.csv), totaling at most 20 MB in the
browser. The submitted UTC candles must exactly match the bundle and the selected
candle duration. Validation happens before replacing an existing dataset. A plain
CSV replacement clears earlier normalization evidence rather than carrying it
forward to unrelated data.

Data Manager distinguishes missing evidence, invalid evidence and a verified clock
transformation. This is not authentication of the declared broker or symbol, an
independent check of the clock policy, or proof that market history is complete.
The source bundle has no authenticated symbol identity.

Web backtest archives retain the full bundle with the frozen candles; replay
rechecks the transformation and candle binding even if archive hashes were
recomputed. Backtest reports retain the verification identifier and scope.
Other web archives using ArchivedDataset also retain this optional bundle, but
all research routes, conversion series, restartable Data Manager persistence and
recorder-dashboard integration are not yet covered. CLI server dataset manifests
currently supply no normalization evidence; use the browser bundle import or the
existing CLI backtest normalization options.

Validation includes failed import atomicity (wrong candles, timeframe and policy),
source-free backtest replay, rejection of a newly hashed archive containing
changed candles, and clearing evidence on plain replacement. Browser checks on
an isolated localhost server imported a synthetic two-candle bundle and visibly
confirmed its limited verification status. The file-reader checks preserve exact
bytes, reject missing/duplicate/oversized selections and propagate read failures.

## Closed-window history can arrive late

A five-minute live observation encountered an empty/failed EURJPY history response
immediately after the first minute closed. No record was committed. A later
read-only request for that identical window returned one M1 candle and ticks.
The preserved journal resumed from the same cursor; the first failed attempt was
retained locally. This is evidence of delayed history availability, not permission
to skip a market or fabricate a candle.

Capture now distinguishes `HistoryUnavailable` (empty/None rates or ticks) from
malformed data. The recorder retries only this availability condition, by default
three times at the polling interval; `--history-attempts 0` preserves immediate
failure, and 0–10 is accepted. Each attempt rechecks account identity and rereads
the same complete window. Counters reset only after a durable commit. Exhaustion
still exits with recording_failed and leaves the cursor unchanged. Malformed
candles, changed metadata/account context and storage failures remain fatal.

The 34-test Python suite covers bounded availability retries, reset after commit,
unchanged progress while waiting, exhaustion, account changes before rereading,
and the zero-retry CLI diagnostic. No real broker-outage, multi-day or power-loss
guarantee follows from this change.

The resumed observation committed five windows [1795520640,1795520940) and
exported the first two while its writer was still alive. The final immutable
capture was ce2207c0415b09a1228d989aa00f3fcb28f678cb3fdde5ecc5899a6d3ed96807.
All 15 recorded M1 bid candles matched their observed tick OHLC. This does not
prove every broker tick was received. At USDJPY event times, EURJPY conversion
quotes were at most 2402 ms old; EURUSD reached 11769 ms at one event, exceeding
the current 10000 ms reference contract. That contract was not relaxed to declare
the new sample fully covered. A later invocation of the updated recorder appended
one more window successfully; automatic late-history recovery itself is covered
by the controlled tests, not claimed as observed in that successful invocation.

## Reusable offline candle/tick consistency check

`python3 scripts/mt5_capture_quality.py --capture <hash-named-capture-directory>`
checks the capture hash, contiguous M1 candle timestamps, tick bounds/order and
bid/ask validity, then compares each candle's OHLC exactly with its observed bid
ticks. Equal-millisecond ticks retain response order. Missing ticks and differing
OHLC produce a discrepancy report and nonzero exit; invalid input is rejected.
Tick-volume counts are reported but are not used to certify tick completeness.
This check neither normalizes broker time nor proves authentic/complete history,
conversion freshness or executable fills.

During the fifteen-window read-only recording probe beginning at broker timestamp
1795527300, the first EURJPY history response contained ticks but no candle. A
bounded retry in the same invocation recovered the window before committing it.
A concurrent two-window export (hash a5137be1e54124c140dec232317227d79811f0623877952ae397e87502e67bf0)
passed all six candle comparisons while recording continued. This is interim
evidence, not completion of the fifteen-window probe. The 40-test Python suite
passed, including mismatched OHLC, missing ticks and invalid-input regressions.

The same checker optionally accepts `--event-symbol USDJPY
--conversion-symbols EURUSD,EURJPY --max-conversion-age-ms 10000` (supply all three
arguments together). It checks the last conversion quote at or before every
observed event, using captured warmup quotes and rejecting future-quote borrowing.
The age limit is inclusive. This checks quote availability across all event ticks,
not actual strategy exposure or correctness of the chosen currency routes.

The ongoing probe's first five-window snapshot
8963c8390d6e6f0db380f278fa153b4456ae4e594fb87b994ba463aae50e825e had 15 matching
bid candles. Of 458 USDJPY event ticks, 13 exceeded the 10,000 ms EURUSD quote-age
limit (maximum 12,896 ms); EURJPY had no stale events (maximum 4,200 ms). No prior
quotes were missing. The tool correctly returned a discrepancy/nonzero result;
the limit was not relaxed. This does not establish a faulty broker quote: it
establishes that these events do not satisfy the declared freshness contract.
All 41 Python tests passed, including no-future-quote and exact-age-boundary cases.

Running the existing independent eight-case checker against that five-window
snapshot, with the dated Tickmill normalization policy and unchanged 10-second
conversion contract, failed at the first backtest with `missing or stale USD->EUR
margin conversion at 1795520153332` (normalized UTC milliseconds). This is the
first reported EURUSD age failure after removing the declared two-hour broker
offset. It precedes the first planned entry: the engine's whole-input conversion
preflight rejects the dataset, rather than proving a conversion failure during
an actual open position. No successful comparison was published; comparison.json
remained `verification_not_completed`. This observed rejection is useful negative
evidence and must not be reported as successful strategy or broker parity.

## Account identity across capture and resume

New captures now include an `account_fingerprint` derived from the broker server
and login, in addition to server/currency/leverage/demo mode. Previously, an
account switch with identical settings could evade the context check. The new
fingerprint is compared before/after capture and across recorder reconnects and
resume. It avoids printing/storing the raw login, but is pseudonymous rather than
anonymous or proof of broker authenticity; keep private captures private.

Legacy journals remain verifiable and exportable. A new recorder invocation
refuses to append to a nonempty journal without the fingerprint: preserve that
journal and start a new one rather than inferring its historical account identity.
The already-running fifteen-window probe loaded the previous code before this
change and retains the explicitly weaker legacy binding. Its evidence must not
be upgraded to fingerprint-bound after the fact.

All 44 Python tests passed, including same-settings login switches during capture
and resume, and legacy rejection before connecting. Two read-only observations
of the installed bridge produced the same new fingerprint; no account switch or
terminal setting was performed during this real-environment check.

A separate new recording at `work/mt5-validation/live-account-bound-1795527300`
then captured [1795527300,1795527360) and resumed in another process to capture
[1795527360,1795527420). Both committed exactly once and the journal verified two
contiguous windows with the same fingerprint. This is observed process-resume
evidence on the real read-only connection; it is not a simulated account-switch
test on the user's terminal. Account-switch rejection is covered by regressions.

Recording status now reports `account_binding` as `not_observed`,
`legacy_settings_only` or `server_login_fingerprint`; malformed fingerprint fields
are rejected. Actual status checks distinguished the two-window new journal from
the concurrently running legacy fifteen-window probe. All 44 Python tests passed.

## Completed fifteen-window read-only probe

The recorder completed successfully with 15 contiguous, integrity-verified windows
covering broker seconds [1795527300,1795528200), for EURUSD, USDJPY and EURJPY.
The final exported capture hash is
`eb1994a4bf2d8694f38e4ad81487288e6b3c1c6bf3c12001fe9443edd1bf00d3`.
All 45 bid OHLC candles matched the observed ticks exactly. Tick counts and bar
tick volumes differ in some windows; OHLC agreement does not certify completeness.

Across 1,330 USDJPY event ticks, EURUSD conversion quotes exceeded the unchanged
10,000 ms age limit at 13 events (maximum 12,896 ms). EURJPY had no stale events
(maximum 4,200 ms); neither route lacked a prior quote. The final quality result
remains `discrepancies`, not a passing strategy comparison. The first five-window
engine rejection above remains the separately observed execution evidence.

An initially unavailable EURJPY bar recovered through the recorder's bounded
retry and the window committed once. This is a short live recovery observation,
not proof of prolonged-outage or power-loss recovery. Account binding remains
`legacy_settings_only` because this process started before fingerprint support.
Private status and quality reports remain under `work/mt5-validation/`; no raw
account history or market capture is published. No broker orders were sent.

## Prospective fingerprint-bound five-window observation

Before recording, a private plan fixed broker interval [1795530840,1795531140),
the next complete future minute plus four more minutes, EURUSD/USDJPY/EURJPY,
60-second warmup, and the unchanged 10,000 ms conversion-age limit. There was no
post-capture range selection. The recorder exited successfully with five verified
contiguous windows and `server_login_fingerprint` binding. Final capture hash:
`f406d795b3680173386e5a7d8bee700107cada01a91ed6dfbc7252726d9aae9f`.

All 15 bid OHLC candles matched observed ticks. Across 495 USDJPY event ticks,
no prior conversion quotes were missing or stale: maximum EURUSD age was 9,600 ms
and EURJPY age 2,079 ms. This is availability/observed OHLC agreement, not proof
of complete broker history, UTC authenticity or actual fill correspondence.
The earlier 15-window stale-quote rejection remains valid negative evidence.

The original eight-case checker stopped on SMA(2)-short because it assumed every
case would execute one trade. Independent close/SMA comparison found eligible
long signals at indices 1, 2 and 3, and no eligible short signal with a following
fill bar. The engine correctly recorded no short entry attempts, no trades and
unchanged initial/final EUR 10,000 equity. The initial failed attempt/log and its
checker identity are retained privately; the window and freshness rule were not
changed to obtain coverage.

The checker now records per-case progress, verifies the no-trade result and replay
when a SMA signal is absent, continues the remaining cases, and returns nonzero
`coverage_incomplete` unless all eight distinct execution cases are exercised.
On the same entire capture, seven execution cases passed independent Decimal
calculations and archive replay: constant long/short, SMA(2) long, stop long/short
and signal-exit long/short. SMA(2) short is `not_exercised`, with its no-trade
behavior separately verified. This must not be presented as eight successful
execution cases.

Simulation assumptions remain EUR 10,000, 0.1 lot USDJPY, EUR 3.5/lot round-trip
commission, one broker point adverse slippage per fill, forex-base margin at
1:30 and directional account conversion from the observed EURUSD/EURJPY quotes.
Engine hash: `38e0661c4e18ce7c8ee41c146c034eac626a039f05561d0bf22086b83bd4e269`.
Checker hash: `a714cfa2ec49f936a2bf2cccf17430dd506fceaf5df12ec4dcb585cfa235bffa`.
These are independent calculations under the declared simulation contract, not
MT5 Strategy Tester trades, actual broker fills or certified broker fees. Raw
captures, fingerprints, plans and comparison reports remain private under work/.
No orders or shared-terminal setting changes were performed.

## Isolated MT5 tester bootstrap limitation (2026-11-24)

A separate portable MT5 build 6328 installation, with its own Wine prefix and no
copied broker credentials, ran a minimal mathematical-mode EA. Its `OnInit`
called `CustomSymbolCreate("SQX_PROBE_ONLY", "SqxReference", "")` and captured
`GetLastError()` immediately. The observed result was
`SQX_CUSTOM_PROBE created=0 error=4014`. The tester reached `OnTester` (43) and
the terminal exited with code 0. Compilation reported zero errors and warnings.

This rules out creating the fixture market from this mathematical tester
bootstrap. It does not demonstrate a market-mode backtest, any fills, or account
parity, and does not prove that broker login is required for every alternative
terminal bootstrap. The custom market still needs preparation in the isolated
terminal before the reference strategy can be exercised. The shared broker
terminal was not modified and this probe contains no order calls.

Local evidence is retained under
`work/mt5-validation/isolated-tester/custom-probe-evidence.json`; source SHA-256
`a2250dd44528cc1583e3415b846a95f6c9b9c52ef6fd715a6e2b7a2932594039`,
compiled EA SHA-256
`6f3c4d99ac3f55713479f063fab76c74f323cafbc4235513b86a47dfe9b4258c`,
and tester log snapshot SHA-256
`14f0018824d85039cd667846b286ca087da529ef83b612b0ed00a7dd54a5981b`.
The log is append-only during subsequent runs; its recorded hash identifies this
observation, not a guarantee that the live log remains unchanged.

## Read-only USDJPY volume calculation coverage (2026-11-24)

The existing SiliconMetaTrader5 bridge remained connected to an EUR demo account
with leverage 1:30. No separate terminal was started for this check. Read-only
`order_calc_profit` and `order_calc_margin` calls supplied 16 new reference cases:
0.01, 0.1, 1 and 2 lots, buy/sell, opening price 150 and closing price 149 or 151.
The declared USDJPY specification was Forex calculation, JPY profit currency and
USD margin currency. Account identity/settings were checked before and after
the capture without retaining the login in the published fixture. EURUSD/EURJPY
quote snapshots were unchanged before/after every case; this brackets the calls
but does not prove atomic capture of MT5's internal conversion quote.

The SqxLabs reference tests compare trade P&L, final account equity, entry margin
and all in-position margin observations against these MT5 calculations, using
explicit EUR-account directional profit conversion and Forex base-currency
margin conversion. All 16 cases passed with an absolute EUR 0.005000001 tolerance
for MT5's cent-rounded outputs. The existing eight calculation cases still pass.
Synthetic three-candle engine inputs reproduce the requested open/close prices;
this is calculation agreement, not actual MT5 signal/fill/timing, fee, financing,
margin-rejection or stop-out evidence. Broker timestamps remain unnormalized in
this fixture. No orders, account changes or Market Watch changes were made.

Fixture SHA-256: `f8af22e295163fe72f9e29ffdc9991dc70aa477d6e7afc1ed52f612c22145de1`.
