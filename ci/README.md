# Reliability checks

Install the pinned compiler with `rustup toolchain install 1.100.0 --profile minimal`.
Run `bash ci/check.sh debug` and `bash ci/check.sh release` from a checkout.
The script stops at the first failure, uses the checked-in Cargo.lock, runs all
workspace tests and builds, compiles both dashboard JavaScript sources, and tests
backup transport without converting hash-sensitive JSON numbers, and exercises
search-start recovery after a lost response and page reload. It does not
publish anything or launch the app against the user's data directory.

The workflow template [reliability-workflow.yml](reliability-workflow.yml) is not
active yet. The current GitHub OAuth connection cannot create workflow files
without the `workflow` scope. Publish this template as
`.github/workflows/reliability.yml` using a connection with that permission, then
verify its first run. The configured workflow runs both profiles on Linux and macOS, with Rust 1.100.0,
Node 22.23.4 and commit-pinned official checkout/setup-node actions. Runner images
and external curl/system libraries are not pinned; this is cross-platform
regression evidence, not a claim of identical binaries across machines. Archive
replay still requires the recorded executable identity.

`cargo test` includes analytical account/execution fixtures, future perturbation,
indicator-prefix and warmup comparisons, snapshot/backup integrity, experiment
commitment/concurrency tests and local HTTP transport tests. It does not certify
all indicator formulas or broker/platform parity. See RELIABILITY.md for scope.

After this workflow has run successfully on the repository, its four matrix checks
must be configured as required checks in branch protection/rulesets before it can
prevent merges. Merely adding this file does not enable that server-side rule.
The local worktree and local test results do not prove a remote CI run succeeded.

After both builds, run `node ci/check-profile-parity.cjs target/debug/sqx-cli
target/release/sqx-cli` (on one command line). It compares all serialized execution
evidence for nonempty long/short cost-bearing OHLC and supplied-tick runs, then every candidate
evaluation and ranking for two fixed evolution seeds. It does not normalize
execution numbers. Source-audit wall-clock metadata is outside that comparison;
only executable identity is excluded from the candidate input-header comparison.
This establishes profile determinism for these fixtures, not broker correctness.

Tick fixtures provide four bid/ask observations per candle (6,000 total). The
check requires supplied-tick mode, no empty tick intervals, and timestamped
account observations. This proves agreement for the supplied sequence, not that
four observations capture every possible real intrabar price path.

The gate also requires Python 3 and runs `scripts/tests` offline. Its MT5 client
stub verifies CSV price precision without requiring SiliconMT5 or a broker login.
It is exporter regression coverage, not independent broker evidence.
The supplied tick fixture uses explicit millisecond CSV and checks that its
123ms component survives in both executed entry and account quote timestamps.
