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
before any market-mode run. Do not install this workflow into the shared terminal.

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
