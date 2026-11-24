# MT5 independent reference harness (not yet validated)

`constant_signal_exit.mq5` is a separately written, minimal reference program for
an explicit constant-entry/constant-exit scenario. It is not an SqxLabs exporter
and must not be used as a deployable trading robot.

The program refuses initialization outside the Strategy Tester, on non-custom
symbols, on names without the `SQX_` prefix, or outside M1. Each order path repeats
the environment check. It uses 0.1 lot, one position and FOK market execution. A
constant entry signal on `SignalBar` fills at the first tick of the following
minute. A constant exit signal fills at the first tick at or after the next minute
boundary. Missing the entry interval fails the case. `ShortSide` selects the side.
No stop, target, indicator, conversion policy or financing assumption is implicit
in this reference source.

Structured `SQX_REFERENCE_*` journal lines expose account/instrument properties,
requested quote/time, actual execution response, every final history deal and
its profit, commission, swap and fee. Final history is read directly rather than
assuming transaction callbacks arrive in request order. Completion additionally
requires exactly two market deals of the intended sides, volume, symbol and
magic, one entry and exit for the same position, no remaining position, and
balance change reconciled against their net amounts within 0.000001 account
currency units. Other cash adjustments cannot silently pass that reconciliation.
This internal consistency check does not assert parity with SqxLabs. Preserve
the full MT5 report and history, not merely the completion line.

Current evidence: the installed MetaEditor build 6328 compiled this source with
zero errors and warnings in a separate Wine prefix. **No market-mode execution or
signal/fill/account comparison has passed yet.** Vendor binaries, credentials,
compiled EX5 files and private market/account records are not included here.

Before comparison, freeze custom-symbol bars/ticks, symbol specification, account
currency/deposit/leverage, spread, commission, financing and tester execution
settings. Use local agents only, disable remote/cloud agents and preserve exact
source/configuration hashes. Confirm the tester used those data without missing
tick intervals or generated replacements. Compare each signal and executed order,
then reconcile costs and balances under declared tolerances. Expand scenarios
only after discrepancies are explained; this harness alone does not validate
the wider engine or the user's strategy.

## Frozen baseline inputs

Run `python3 examples/mt5-reference/prepare_fixture.py <new-directory>` to create
120 M1 candles and 480 synthetic bid/ask ticks, two SqxLabs strategy files, an
account configuration and corresponding MT5 `.set`/`.ini` files. Existing
directories are refused. The manifest hashes the generated inputs and records
`comparison_status: not_executed`; generation is never validation.

The baseline uses USD 10,000, leverage 30, 0.1 EURUSD lot, observed two-pip spread
and zero commission, financing and extra slippage. Hand arithmetic predicts
USD -1 for long and USD -3 for short. It begins at 2026-01-05 00:00 UTC, enters
at 00:01 and exits at 00:02. SqxLabs uses a one-trade daily limit to reproduce the
reference program's single-position case.

MT5 inputs require a separately prepared `SQX_EURUSD` custom symbol with EUR base
and margin currency, USD profit currency, Forex calculation, 100,000 contract
size, five decimal places, 0.00001 tick size, 0.01 volume step and FOK execution.
Enable quote/trade sessions covering the fixture. Set commission and financing
to zero explicitly; the generated INI does not configure all symbol/tester fees.
Place the `.set` files in that isolated terminal's `MQL5/Profiles/Tester` folder
and the compiled reference in `MQL5/Experts`. Import and verify both bars and ticks
before any market-mode run. Prefer an isolated test terminal. For the explicitly
user-started workflow inside the existing SiliconMetaTrader5 terminal, follow
`START_HIER_NL.md`; the agent does not install or start it automatically.

The generated two-sided fixture was run locally through SqxLabs and its archived
trade prices, timestamps, exit reasons and balances matched the hand calculation.
**The corresponding MT5 market-mode comparison remains unexecuted.** The baseline
does not cover third-currency conversion, nonzero costs or dynamic exits.

## Checking a captured tester journal

After an actual run, extract exactly that run's journal into a separate file and
run `python3 scripts/mt5_tester_log_check.py <log>` (add `--short` for the short
case). UTF-8 and BOM-marked UTF-16 are supported. The checker rejects missing,
duplicate, reordered or mixed reference records; it checks the fixed baseline's
account currency, deposit, leverage, contract and currency specification, entry
and exit times, bid/ask requests, successful full fills, deal identities,
directions, volume, zero costs, profit and final balance/equity. It uses Decimal
arithmetic and fails on non-finite numeric values. This deliberately narrow
zero-cost baseline expects exact logged values, not configurable tolerances.

Success is named `reported_baseline_values_match`. It explicitly leaves
`tick_provenance_verified` and `sqxlabs_parity_verified` false. A journal can be
edited, and the printed records do not prove tester model, complete tick input,
all symbol settings or the source/binary that produced them. Retain the original
full log, report, exact input/configuration/source/binary hashes and independent
SqxLabs results before assessing parity. The checker prints the input log hash.
Its unit tests use synthetic journals and do not count as an executed MT5 test.

## User-started setup inside SiliconMetaTrader5

`prepare_custom_symbol.mq5` is a terminal script requiring explicit
`CreateSyntheticTestMarket=true`. It refuses tester execution and existing symbol
names, contains no order calls, and creates/selects only `SQX_EURUSD`. It imports
240 M1 bars / 960 ticks, including 120 warmup bars before the January 5 baseline,
and checks both tick and candle readback before printing `SQX_FIXTURE_READY`.
A setup error leaves the new partial symbol for inspection; no automatic deletion
or overwrite occurs. The new setup source has not yet been compiled/executed in
the user's terminal. Compilation and readback are required gates in the guide.

The extra prehistory avoids the documented tester requirement for at least 100
preceding bars moving the requested start date. The original 120 test candles
and hand-calculated long/short expectations are unchanged. Setup is not execution
parity evidence; inspect the actual tester start, ticks, settings and full logs.

### Setup revision V2

The first user-run setup reported 5308, followed by 5304 on retry. Read-only
inspection found volume_min still at 0.00000001 while volume_max=100 and
volume_step=0.01 had applied. V2 sets maximum, step and minimum in that order,
checks volume readback and reports each failing property/session immediately.
It creates a fresh `SQX_EURUSD_V2`, leaving the partial original untouched.
Use that symbol in the tester and `--symbol SQX_EURUSD_V2` in the log checker.
The user's subsequent setup screenshot reports 240 bars, 960 ticks and matched
tick/bar readback for V2. This supports successful setup execution, but does not
establish source/binary provenance or execution parity.

### HTML account-setting gate (2026-11-24)

The first two user-supplied single-test HTML reports match the expected long and
short entry/exit prices and times. Both explicitly report **Currency: profit in
pips**, however. They are rejected for USD account validation: matching -1/-3
numbers in that mode do not establish dollar PnL, commission or swap handling.
Repeat the two tests with profit-in-pips disabled and USD selected; retain the
original reports separately. Raw reports remain private in `work/`.

Supply the English HTML report alongside each single-run journal:

```sh
python3 scripts/mt5_tester_log_check.py long.log --symbol SQX_EURUSD_V2 --report long.html
python3 scripts/mt5_tester_log_check.py short.log --symbol SQX_EURUSD_V2 --short --report short.html
```

The added gate checks explicit report account/input settings and rejects pips
mode, missing/duplicate settings and unsupported layouts. It does not inspect
HTML deals or prove that the report and journal came from the same execution.
Those limitations remain explicit; `report_settings_match` is false when no
report is supplied, and SqxLabs parity and tick provenance remain unverified.
