# Pipeline evidence and recovery

CLI pipelines default to `data/research`; use `--archive-dir` to isolate studies.
The printed **Pipeline checkpoint** is the start ID needed below. Fresh pipeline
invocations have different attempt IDs even when all research inputs match.
Repeated recovery of one start ID is not a new independent trial.

```sh
sqx-cli pipeline-history --archive-dir /path/to/archive --run-id START_ID
sqx-cli replay-pipeline --archive-dir /path/to/archive --run-id START_ID --resume
sqx-cli replay-pipeline --archive-dir /path/to/archive --run-id START_ID
```

Inspection checks content identities and ordered event links without executing
calculations. It distinguishes an incomplete prefix from recorded completion;
recorded completion can still contain simulation errors. Replay requires the
exact original executable, and never launches a binary found inside an archive.
Engine bytes are retained at creation but runtime dependencies are not packaged.
Older executable builds without these commands cannot acquire recovery support
by running a newer, different engine against their records.

Resume recomputes the saved prefix, comparing every emitted event before adding
missing work. It restarts deterministic calculation from the beginning, not a
serialized in-memory simulator state. It preserves failed/rejected outcomes and
stops on mismatches, missing/contradictory links or storage failures. A process
lock prevents simultaneous writers/replayers for one start ID. Lock-file
existence alone does not mean that a process is alive.

The completion record references all event IDs. A backup must preserve the start,
all events and completion (when present); exporting only one record is not a
whole-search backup. Restore/replay tests cover source deletion and process kill,
not arbitrary disk corruption or power loss. Optimization records each walk-forward family, candidate start/full execution
outcome, and out-of-sample start/full execution outcome before proceeding. Resume
still recomputes and verifies the prefix; it does not restore simulator memory.
Individual Monte Carlo samples are not resumable units. Older engines retain
their original family-level checkpoint coverage.

Current tests compare a killed/restored/resumed search with a separate
uninterrupted search, reject a changed event and wrong engine, verify lock
exclusion, and ensure repeated replay adds no records. This is internal
reproducibility evidence, not independent execution validation or untouched
holdout certification. Data from the entire pipeline is prior exposure.

## Portable whole-search backup

```sh
sqx-cli export-pipeline --archive-dir /path/to/archive --run-id START_ID > pipeline-backup.json
sqx-cli import-pipeline --archive-dir /path/to/restored --input pipeline-backup.json
```

Export includes the frozen inputs once, all ordered events, and completion when
present. Import checks the checksum, content identities and the combined incoming
and existing event graph before publishing records. Repeat import is idempotent;
completion is published last. A storage failure may leave a recoverable prefix.
Neither operation installs or executes an engine. Keep the original executable
and its runtime separately if replay is required. Checksums detect corruption,
not an untrusted author's fabricated results. Exported JSON can contain private
market data and strategy definitions; store it accordingly. Interrupted exports
must be repeated; partial JSON is not a backup. Large-archive performance and
power-loss recovery remain outside the tested coverage.
