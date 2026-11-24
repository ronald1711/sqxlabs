# Reliability checks

## Retaining the exact research engine

Before upgrading or rebuilding an executable used for research, retain its bytes:

```sh
python3 scripts/retain_engine.py target/release/sqx-cli --out data/engines
```

For an existing research record, add `--expected-engine-id` followed by its
`run.engine_id`. A mismatch fails before publication. The filename is the SHA-256
engine identity used by research archives. Keep this directory with research
backups, outside Git, and copy it to an independent backup location. The command
does not delete older versions, invoke the engine or upload files. Concurrent
publication is atomic on local POSIX filesystems; an existing corrupted object
is reported and never silently replaced. Interrupted pending files are not
published evidence. This is not a power-loss or network-filesystem guarantee.

Retained files have no execute permission. To replay, verify the selected file's
SHA-256 against the research record, copy it to a separate local executable path,
grant that copy execute permission, and use its normal research replay command.
Only execute a locally trusted build. Do not automatically execute binaries from
downloaded research backups. The normal replay check still rejects a different
engine identity. The original operating system, architecture and required runtime
libraries must remain available: retaining bytes does not package those
dependencies or authenticate the build's source. Record that environment with
your research. This manual procedure does not yet provide automatic retention
on every research route or durable GitHub release artifacts.

## Running the gates

Install the pinned compiler with `rustup toolchain install 1.100.0 --profile minimal`.
Run `bash ci/check.sh debug` and `bash ci/check.sh release` from a checkout.
The script stops at the first failure, uses the checked-in Cargo.lock, runs all
workspace tests and builds, compiles both dashboard JavaScript sources, and tests
backup transport without converting hash-sensitive JSON numbers, and exercises
search-start recovery after a lost response and page reload. It does not
publish anything or launch the app against the user's data directory.

The reliability workflow is active at `.github/workflows/reliability.yml` as of
commit e832c2f12ca209f7cabdedde4ce815d9b155cb9e. The connected GitHub app could
publish it although the earlier Git OAuth connection lacked workflow scope.
The first run is https://github.com/ronald1711/sqxlabs/actions/runs/45357592150;
all four jobs passed, including profile parity on both platforms. This run covers
the published baseline, not newer uncommitted local changes. It runs both profiles on Linux and macOS, with Rust 1.100.0,
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

The Rust data-import integration test also invokes Python 3 to exercise real
broker-clock normalization and UTC CSV ingestion end to end. Python 3 must be
available when running the workspace Rust tests, not only the exporter suite.

The local gate also runs `check-reference-fixture.py` against the selected profile's
CLI. It generates a small synthetic EUR-account dataset with USDJPY and conversion
quotes, runs eight independent Decimal comparisons (constant/SMA entry, stops and
signal exits, long and short), replays the archives, then mutates future prices
and requires unchanged earlier evidence plus changed terminal results. No MT5
installation, network connection or private market files are needed. These are
bounded regression checks, not broker-execution certification. The GitHub workflow is active; unpublished local checks are not part of its
remote coverage until their source changes are pushed and the resulting run passes.
