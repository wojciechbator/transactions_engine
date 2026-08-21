# Simple Transactions Engine

A small Rust CLI exercise that streams transaction records from CSV, applies account/dispute state transitions in order, and writes final account balances as CSV.

Supported operations:

- deposit
- withdrawal
- dispute
- resolve
- chargeback

A sample `transactions.csv` is included.

## Running

```bash
cargo run -- transactions.csv > account.csv
cargo test
cargo bench --bench transaction_benchmarks
```

## Processing model

```text
CSV reader
  -> deserialize one transaction
  -> validate/apply ordered ledger transition
  -> update account + transaction history
  -> continue streaming
  -> emit final account state
```

The implementation assumes input transactions arrive in chronological order. It does not load the entire CSV into memory before processing; the ledger retains only the account state and transaction history required for later dispute/resolve/chargeback operations.

## Design choices

- The executable is a thin CLI over library code so the state machine can be tested independently.
- Transaction and dispute state are represented explicitly with enums rather than stringly-typed branches.
- Monetary values use a purpose-built fixed-point `Amount` for the required four decimal places, with checked arithmetic and an explicit `NumericOverflow` error.
- `FxHashMap` is used for in-process lookup-heavy ledger state. This is a local CLI with non-adversarial keys; a network-facing service would need a different threat/performance analysis.
- Withdrawals are constrained by currently available funds; disputed money moves out of the available balance until resolve/chargeback changes the state.
- Benchmarks are included to make performance changes measurable rather than assumed.

## Why synchronous?

The current workload is a single ordered file feeding one stateful ledger. A synchronous streaming loop keeps the execution model small and preserves transaction order naturally. I have not measured a benefit from adding async or worker concurrency to this CLI, so the implementation does not add either merely for architectural style.

That choice is specific to this workload. A service accepting many independent uploads over the network would have different requirements: asynchronous socket I/O, bounded admission, per-request isolation and controlled CPU concurrency could all be appropriate while each individual ledger still preserves its own ordering guarantees.

## Parsing and allocation

The parser processes records incrementally instead of eagerly materializing the whole input file. The implementation also avoids unnecessary intermediate representations where practical, but this README deliberately does **not** claim allocation-free parsing without allocator-level measurement proving it.

If allocation behaviour becomes important, the benchmark suite should be extended with allocator/profiling evidence before making a stronger claim.

## Turning it into a service

A production network service would need more than wrapping `main` in Tokio. I would separate concerns roughly as follows:

```text
accept/read request
  -> bounded admission / request size limits
  -> parse one independent ledger stream
  -> ordered ledger execution
  -> bounded response write
```

The outer transport can be asynchronous while the ledger transition logic stays deterministic and synchronous. Concurrency should be capped explicitly rather than tied to the number of incoming sockets, with backpressure/load shedding when the system reaches that limit.

## AI usage

AI assisted with the benchmark harness, additional test cases and documentation after the core transaction logic was written. The benchmark results and tests are still executable repository artifacts rather than claims that depend on generated prose.
