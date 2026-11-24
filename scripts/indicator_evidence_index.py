"""Render a conservative, source-linked reference index; never infer validation."""
import argparse
from pathlib import Path
import re

ROOT = Path(__file__).resolve().parents[1]
TESTS = ROOT / "crates/sqx-indicators/tests"
# Explicitly reviewed references only. Unknown catalog entries remain unindexed.
# This indexes mathematical functions, not strategy-dispatch parameter wiring.
GROUPS = [
    ("SMA EMA", "talib_crosscheck", "sma_and_ema_match", "Rust ta-lib; periods 3/10/20; SMA seeds and warmup"),
    ("RSI", "talib_crosscheck", "rsi_matches_wilder", "Rust ta-lib; periods 2/14; non-flat Wilder case"),
    ("ATR", "talib_crosscheck", "atr_matches_wilder", "Rust ta-lib; periods 14/20; Wilder smoothing"),
    ("ADX DIPlus DIMinus", "talib_crosscheck", "adx_and_di_converge_to_ta_lib_in_the_tail", "Converged tail only, period 14; seed/warmup NOT established"),
    ("DPO", "talib_crosscheck", "dpo_matches", "Rust ta-lib; periods 4/20"),
    ("CCI", "talib_crosscheck", "cci_matches", "Rust ta-lib; periods 14/20"),
    ("MFI", "reference", "mfi_hand_calculation_requires_n_classified_flows", "Fixed arithmetic; periods 1/2; classified-flow warmup"),
    ("StochK StochD", "talib_crosscheck", "stoch_matches", "Rust ta-lib; K14/D3; comparison aligns to later D warmup"),
    ("AroonUp AroonDown", "talib_crosscheck", "aroon_matches", "Rust ta-lib; periods 14/25"),
    ("MACD MACDSignal MACDHist", "reference", "macd_sma_seeded_reference_includes_first_signal_and_histogram", "Fixed rational values; 2/3/2 SMA seeds; first signal and future-prefix invariance"),
    ("BBUpper BBMiddle BBLower", "talib_crosscheck", "bollinger_matches", "Rust ta-lib; period 20, multiplier 2"),
    ("TrueRange ROC StdDev OBV WilliamsR PPO", "talib_crosscheck", "trange_roc_stddev_obv_willr_ppo_match", "Rust ta-lib; selected periods; first-bar TR is a local convention"),
    ("Highest Lowest", "ta_crosscheck", "maximum_minimum_match", "Rust ta; period 20, defined tail only"),
    ("KaufmanER", "ta_crosscheck", "efficiency_ratio_matches_after_warmup", "Rust ta; period 14, defined tail only"),
    ("Momentum", "reference", "momentum_reference", "Fixed arithmetic; period 2"),
    ("VortexPlus VortexMinus", "reference", "vortex_hand_calculation_requires_n_actual_movements", "Fixed arithmetic; periods 1/2/3; movement warmup"),
    ("CMO", "reference", "cmo_matches_hand_calculated_rolling_changes_not_wilder_smoothing", "Fixed arithmetic; rolling period 3; not Wilder variant"),
    ("TII", "reference", "tii_current_baseline_and_two_to_one_periods_have_independent_values", "Fixed arithmetic; periods 2/30; current baseline and 2:1 mapping"),
]


def render():
    catalog = (ROOT / "crates/sqx-strategy/src/catalog.rs").read_text()
    names = re.findall(r'IndicatorSpec\s*\{\s*name:\s*"([^"]+)"', catalog)
    if not names or len(names) != len(set(names)):
        raise ValueError("Empty or duplicate catalog; inspect source format")
    evidence = {}
    for group, suite, test, scope in GROUPS:
        source = (TESTS / f"{suite}.rs").read_text()
        match = re.search(r"#\[test\]\s*fn " + re.escape(test) + r"\(", source)
        if not match:
            raise ValueError(f"Missing test: {suite}::{test}")
        line = source[:match.start()].count("\n") + 1
        link = f"[test](crates/sqx-indicators/tests/{suite}.rs#L{line})"
        for name in group.split():
            if name not in names or name in evidence:
                raise ValueError(f"Unknown or duplicate evidence entry: {name}")
            evidence[name] = f"Partial reference indexed | {scope}; {link}"
    lines = [
        "# Indicator reference evidence index", "",
        "Generated with `python3 scripts/indicator_evidence_index.py`; verify freshness with `--check`.", "",
        f"All {len(names)} registered indicator names are listed. {len(evidence)} have a reviewed reference link; "
        f"{len(names)-len(evidence)} remain unindexed. These are inventory counts, not validation percentages.", "",
        "A link identifies an existing test, not a successful test run. Rust `ta` and `ta-lib` "
        "are the packages pinned in Cargo.lock; this index does not imply execution of TA-Lib C or MT5.", "",
        "Unindexed means evidence has not been reviewed here, not that no tests exist. "
        "Every row remains incomplete: parameter extremes, missing/nonfinite input, seed and warmup "
        "conventions, causal prefixes, and strategy-dispatch/default-parameter wiring need their own evidence. "
        "A function tested with period 2 does not validate its catalog default of 14. "
        "Local algebraic consistency tests are not promoted to independent comparisons.", "",
        "| Registered indicator | Evidence status | Exercised scope / source |",
        "| --- | --- | --- |",
    ]
    for name in names:
        lines.append(f"| {name} | " + evidence.get(name, "Not indexed | Review formula, conventions and independent reference") + " |")
    return "\n".join(lines) + "\n"


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--check", action="store_true")
    args = parser.parse_args()
    output = ROOT / "INDICATOR_EVIDENCE.md"
    content = render()
    if args.check:
        if not output.exists() or output.read_text() != content:
            raise SystemExit("Indicator index is stale; regenerate and review it")
        print("Indicator reference index matches the catalog and referenced test locations")
    else:
        output.write_text(content)
