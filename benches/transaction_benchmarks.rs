use criterion::{criterion_group, criterion_main, BenchmarkId, Criterion, Throughput};
use std::hint::black_box;
use std::io::Cursor;
use std::time::Duration;

use transactions_engine::engine::{process_csv, Ledger};
use transactions_engine::transaction::{TransactionRow, TransactionType};

fn deposit(client: u16, tx: u32, amount: &str) -> TransactionRow {
    TransactionRow {
        tx_type: TransactionType::Deposit,
        client,
        tx,
        amount: Some(amount.parse().unwrap()),
    }
}

fn withdrawal(client: u16, tx: u32, amount: &str) -> TransactionRow {
    TransactionRow {
        tx_type: TransactionType::Withdrawal,
        client,
        tx,
        amount: Some(amount.parse().unwrap()),
    }
}

fn dispute(client: u16, tx: u32) -> TransactionRow {
    TransactionRow { tx_type: TransactionType::Dispute, client, tx, amount: None }
}

fn resolve(client: u16, tx: u32) -> TransactionRow {
    TransactionRow { tx_type: TransactionType::Resolve, client, tx, amount: None }
}

fn generate_csv(num_clients: u16, num_transactions: u32) -> String {
    let mut csv = String::from("type,client,tx,amount\n");
    let mut tx_id = 1u32;

    for client in 1..=num_clients {
        for i in 0..num_transactions / num_clients as u32 {
            csv.push_str(&format!("deposit,{},{},{}.0000\n", client, tx_id, (i + 1) * 10));
            tx_id += 1;

            if i % 2 == 1 {
                csv.push_str(&format!("withdrawal,{},{},{}.0000\n", client, tx_id, i * 5));
                tx_id += 1;
            }

            if i % 10 == 9 {
                let dispute_tx = tx_id - 3;
                csv.push_str(&format!("dispute,{},{},\n", client, dispute_tx));
                csv.push_str(&format!("resolve,{},{},\n", client, dispute_tx));
                tx_id += 2;
            }
        }
    }

    csv
}

fn ledger_with_deposits(num_clients: u16, deposits_per_client: u32) -> (Ledger, u32) {
    let mut ledger = Ledger::new();
    let mut tx_id = 1u32;
    for client in 1..=num_clients {
        for _ in 0..deposits_per_client {
            ledger.apply(deposit(client, tx_id, "100.0000")).unwrap();
            tx_id += 1;
        }
    }
    (ledger, tx_id)
}

fn bench_end_to_end(c: &mut Criterion) {
    let mut group = c.benchmark_group("end_to_end");

    for size in [100u32, 1_000, 5_000, 10_000, 50_000] {
        let csv = generate_csv(100, size);
        group.throughput(Throughput::Bytes(csv.len() as u64));
        group.bench_with_input(BenchmarkId::new("run", size), &csv, |b, data| {
            b.iter(|| {
                let cursor = Cursor::new(data.as_bytes());
                black_box(process_csv(cursor, Vec::new()).unwrap());
            });
        });
    }

    group.finish();
}

fn bench_deposit(c: &mut Criterion) {
    c.bench_function("deposit_1k", |b| {
        b.iter(|| {
            let mut ledger = Ledger::new();
            for tx_id in 1u32..=1_000 {
                let client = (tx_id % 100 + 1) as u16;
                ledger.apply(deposit(client, tx_id, "10.0000")).unwrap();
            }
            black_box(ledger);
        });
    });
}

fn bench_withdrawal(c: &mut Criterion) {
    let (base, next_tx) = ledger_with_deposits(100, 100);

    c.bench_function("withdrawal_1k", |b| {
        b.iter_batched(
            || (base.clone(), next_tx),
            |(mut ledger, mut tx_id)| {
                for client in 1u16..=100 {
                    ledger.apply(withdrawal(client, tx_id, "1.0000")).unwrap();
                    tx_id += 1;
                }
                black_box(ledger);
            },
            criterion::BatchSize::SmallInput,
        );
    });
}

fn bench_dispute_resolve(c: &mut Criterion) {
    let mut group = c.benchmark_group("dispute_resolve");

    for deposits in [100u32, 500, 1_000] {
        let (base, _next_tx) = ledger_with_deposits(1, deposits);

        group.bench_with_input(
            BenchmarkId::new("cycle", deposits),
            &deposits,
            |b, &deposits| {
                b.iter_batched(
                    || base.clone(),
                    |mut ledger| {
                        for tx_id in 1..=deposits {
                            ledger.apply(dispute(1, tx_id)).unwrap();
                            ledger.apply(resolve(1, tx_id)).unwrap();
                        }
                        black_box(ledger);
                    },
                    criterion::BatchSize::SmallInput,
                );
            },
        );
    }

    group.finish();
}

fn bench_high_contention(c: &mut Criterion) {
    c.bench_function("high_contention_10_accounts", |b| {
        b.iter(|| {
            let mut ledger = Ledger::new();
            let mut tx_id = 1u32;

            for client in 1u16..=10 {
                ledger.apply(deposit(client, tx_id, "10000.0000")).unwrap();
                tx_id += 1;
            }

            for _ in 0..10_000 {
                let client = (tx_id % 10 + 1) as u16;
                let _ = ledger.apply(withdrawal(client, tx_id, "1.0000"));
                tx_id += 1;
            }

            black_box(ledger);
        });
    });
}

fn bench_large_account_set(c: &mut Criterion) {
    let mut group = c.benchmark_group("account_set_size");

    for count in [100u16, 1_000, 10_000] {
        group.bench_with_input(BenchmarkId::new("deposits", count), &count, |b, &count| {
            b.iter(|| {
                let mut ledger = Ledger::new();
                for client in 1..=count {
                    ledger.apply(deposit(client, client as u32, "100.0000")).unwrap();
                }
                black_box(ledger);
            });
        });
    }

    group.finish();
}

fn bench_cpu_intensive(c: &mut Criterion) {
    let mut group = c.benchmark_group("cpu_intensive");
    group.measurement_time(Duration::from_secs(15));

    let csv = generate_csv(1_000, 50_000);

    group.bench_function("complex_50k_transactions", |b| {
        b.iter(|| {
            let cursor = Cursor::new(csv.as_bytes());
            black_box(process_csv(cursor, Vec::new()).unwrap());
        });
    });

    group.finish();
}

criterion_group!(
    benches,
    bench_end_to_end,
    bench_deposit,
    bench_withdrawal,
    bench_dispute_resolve,
    bench_high_contention,
    bench_large_account_set,
    bench_cpu_intensive,
);
criterion_main!(benches);
