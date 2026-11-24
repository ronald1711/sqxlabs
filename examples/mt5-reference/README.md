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
