#!/usr/bin/env bash
# Run the same reliability checks locally and in CI. Never publish or modify data/.
set -euo pipefail
cd "$(dirname "$0")/.."
profile="${1:-debug}"
cargo_args=(--workspace --locked)
case "$profile" in
  debug) ;;
  release) cargo_args+=(--release) ;;
  *) printf 'Usage: %s [debug|release]\n' "$0" >&2; exit 64 ;;
esac
rustup run 1.100.0 rustc --version
rustup run 1.100.0 cargo --version
node --version
curl --version
rustup run 1.100.0 cargo test "${cargo_args[@]}"
rustup run 1.100.0 cargo build "${cargo_args[@]}"
node --check crates/sqx-web/assets/workspace.js
node ci/check-inline-js.cjs
node crates/sqx-web/tests/normalization_import.cjs
node crates/sqx-web/tests/backup_wire.cjs
node crates/sqx-web/tests/search_recovery.cjs
node crates/sqx-web/tests/generator_inputs.cjs
node crates/sqx-web/tests/confirmation_notice.cjs

node crates/sqx-web/tests/backtest_errors.cjs

node crates/sqx-web/tests/robustness_errors.cjs

node crates/sqx-web/tests/comparison_ui.cjs
node crates/sqx-web/tests/evolution_recovery.cjs
node crates/sqx-web/tests/evolution_report.cjs
node crates/sqx-web/tests/evolution_creation.cjs
node crates/sqx-web/tests/evolution_restore.cjs
node crates/sqx-web/tests/trade_explanations.cjs
node crates/sqx-web/tests/entry_explanations.cjs
node crates/sqx-web/tests/signal_explanations.cjs
node crates/sqx-web/tests/assistant_evidence.cjs
node crates/sqx-web/tests/assistant_trial_recovery.cjs

python3 -m unittest discover -s scripts/tests -v

# Independent arithmetic and future-data mutation checks need no MT5 connection.
python3 ci/check-reference-fixture.py --executable "target/$profile/sqx-cli"
