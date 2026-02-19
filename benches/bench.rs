use criterion::{BatchSize, Criterion, Throughput, criterion_group, criterion_main};
use obcore::{Evaluator, Instruction, Op, OrderBook, OrderInterface};
use std::hint::black_box;

/// Order type for benchmarks
#[derive(Default, Debug, PartialEq, Eq, Clone)]
pub struct BenchOrder {
    id: u64,
    is_buy: bool,
    price: u64,
    quantity: u64,
    remaining: u64,
}

impl BenchOrder {
    pub fn new(id: u64, is_buy: bool, price: u64, quantity: u64) -> Self {
        Self {
            id,
            is_buy,
            price,
            quantity,
            remaining: quantity,
        }
    }
}

impl OrderInterface for BenchOrder {
    type I = u64;
    type N = u64;
    type O = u64;

    fn id(&self) -> &u64 {
        &self.id
    }

    fn price(&self) -> u64 {
        self.price
    }

    fn is_buy(&self) -> bool {
        self.is_buy
    }

    fn quantity(&self) -> u64 {
        self.quantity
    }

    fn remaining(&self) -> u64 {
        self.remaining
    }

    fn fill(&mut self, quantity: u64) {
        self.remaining -= quantity;
    }

    fn owner(&self) -> &u64 {
        &self.id
    }
}

/// Helper to pre-populate an order book with N orders on each side
fn populate_book(ob: &mut OrderBook<BenchOrder>, count: usize) -> u64 {
    let mut eval = Evaluator::default();
    for i in 0..count {
        let buy = BenchOrder::new(i as u64, true, 900 + (i % 50) as u64, 100);
        for instr in eval.eval(ob, Op::Insert(buy)) {
            ob.apply(instr);
        }

        let sell = BenchOrder::new((i + count) as u64, false, 1100 + (i % 50) as u64, 100);
        for instr in eval.eval(ob, Op::Insert(sell)) {
            ob.apply(instr);
        }
    }
    (count * 2) as u64
}

/// Create a populated order book with given depth
fn make_book(depth: usize) -> (OrderBook<BenchOrder>, u64) {
    let mut ob = OrderBook::<BenchOrder>::default();
    let next_id = populate_book(&mut ob, depth / 2);
    (ob, next_id)
}

/// Benchmark eval operations with varying book depth
fn bench_eval_insert(c: &mut Criterion) {
    let mut group = c.benchmark_group("eval_insert");
    group.throughput(Throughput::Elements(1));

    let mut eval = Evaluator::default();
    group.bench_function("empty", |b| {
        b.iter_batched_ref(
            || {
                let ob = OrderBook::<BenchOrder>::default();
                let order = BenchOrder::new(0, true, 900, 100);
                (ob, Some(order))
            },
            |(ob, order)| {
                eval.reset();
                black_box(eval.eval_insert(ob, black_box(order.take().unwrap())).count());
            },
            BatchSize::LargeInput,
        );
    });

    let mut eval = Evaluator::default();
    group.bench_function("depth_100", |b| {
        b.iter_batched_ref(
            || {
                let (ob, next_id) = make_book(100);
                let order = BenchOrder::new(next_id, true, 895, 100);
                (ob, Some(order))
            },
            |(ob, order)| {
                eval.reset();
                black_box(eval.eval_insert(ob, black_box(order.take().unwrap())).count());
            },
            BatchSize::LargeInput,
        );
    });

    let mut eval = Evaluator::default();
    group.bench_function("depth_1000", |b| {
        b.iter_batched_ref(
            || {
                let (ob, next_id) = make_book(1000);
                let order = BenchOrder::new(next_id, true, 895, 100);
                (ob, Some(order))
            },
            |(ob, order)| {
                eval.reset();
                black_box(eval.eval_insert(ob, black_box(order.take().unwrap())).count());
            },
            BatchSize::LargeInput,
        );
    });

    group.finish();
}

/// Benchmark apply after insert
fn bench_apply_insert(c: &mut Criterion) {
    let mut group = c.benchmark_group("apply_insert");
    group.throughput(Throughput::Elements(1));

    group.bench_function("empty", |b| {
        b.iter_batched_ref(
            || {
                let ob = OrderBook::<BenchOrder>::default();
                let order = BenchOrder::new(0, true, 900, 100);
                (ob, Some(order), 100u64)
            },
            |(ob, order, remaining)| {
                black_box(ob.apply_insert(black_box(order.take().unwrap()), *remaining))
            },
            BatchSize::LargeInput,
        );
    });

    group.bench_function("depth_100", |b| {
        b.iter_batched_ref(
            || {
                let (ob, next_id) = make_book(100);
                let order = BenchOrder::new(next_id, true, 895, 100);
                (ob, Some(order), 100u64)
            },
            |(ob, order, remaining)| {
                black_box(ob.apply_insert(black_box(order.take().unwrap()), *remaining))
            },
            BatchSize::LargeInput,
        );
    });

    group.bench_function("depth_1000", |b| {
        b.iter_batched_ref(
            || {
                let (ob, next_id) = make_book(1000);
                let order = BenchOrder::new(next_id, true, 895, 100);
                (ob, Some(order), 100u64)
            },
            |(ob, order, remaining)| {
                black_box(ob.apply_insert(black_box(order.take().unwrap()), *remaining))
            },
            BatchSize::LargeInput,
        );
    });

    group.finish();
}

/// Benchmark eval cancel operations with varying book depth
fn bench_eval_cancel(c: &mut Criterion) {
    let mut group = c.benchmark_group("eval_cancel");
    group.throughput(Throughput::Elements(1));

    let mut eval = Evaluator::default();
    group.bench_function("single", |b| {
        b.iter_batched_ref(
            || {
                let mut ob = OrderBook::<BenchOrder>::default();
                let mut setup_eval = Evaluator::default();
                let order = BenchOrder::new(0, true, 900, 100);
                for instr in setup_eval.eval(&ob, Op::Insert(order)) {
                    ob.apply(instr);
                }
                (ob, 0u64)
            },
            |(ob, order_id)| {
                eval.reset();
                black_box(eval.eval_cancel(ob, black_box(*order_id)).count());
            },
            BatchSize::LargeInput,
        );
    });

    let mut eval = Evaluator::default();
    group.bench_function("depth_100", |b| {
        b.iter_batched_ref(
            || {
                let (ob, _) = make_book(100);
                (ob, 50u64)
            },
            |(ob, order_id)| {
                eval.reset();
                black_box(eval.eval_cancel(ob, black_box(*order_id)).count());
            },
            BatchSize::LargeInput,
        );
    });

    let mut eval = Evaluator::default();
    group.bench_function("depth_1000", |b| {
        b.iter_batched_ref(
            || {
                let (ob, _) = make_book(1000);
                (ob, 500u64)
            },
            |(ob, order_id)| {
                eval.reset();
                black_box(eval.eval_cancel(ob, black_box(*order_id)).count());
            },
            BatchSize::LargeInput,
        );
    });

    group.finish();
}

/// Benchmark apply after cancel
fn bench_apply_cancel(c: &mut Criterion) {
    let mut group = c.benchmark_group("apply_cancel");
    group.throughput(Throughput::Elements(1));

    group.bench_function("single", |b| {
        b.iter_batched_ref(
            || {
                let mut ob = OrderBook::<BenchOrder>::default();
                let mut eval = Evaluator::default();
                let order = BenchOrder::new(0, true, 900, 100);
                for instr in eval.eval(&ob, Op::Insert(order)) {
                    ob.apply(instr);
                }
                (ob, 0u64)
            },
            |(ob, order_id)| black_box(ob.apply_delete(black_box(*order_id))),
            BatchSize::LargeInput,
        );
    });

    group.bench_function("depth_100", |b| {
        b.iter_batched_ref(
            || {
                let (ob, _) = make_book(100);
                (ob, 50u64)
            },
            |(ob, order_id)| black_box(ob.apply_delete(black_box(*order_id))),
            BatchSize::LargeInput,
        );
    });

    group.bench_function("depth_1000", |b| {
        b.iter_batched_ref(
            || {
                let (ob, _) = make_book(1000);
                (ob, 500u64)
            },
            |(ob, order_id)| black_box(ob.apply_delete(black_box(*order_id))),
            BatchSize::LargeInput,
        );
    });

    group.finish();
}

/// Benchmark eval matching across varying number of price levels
fn bench_eval_match(c: &mut Criterion) {
    let mut group = c.benchmark_group("eval_match");
    group.throughput(Throughput::Elements(1));

    let mut eval = Evaluator::default();
    group.bench_function("1_level", |b| {
        b.iter_batched_ref(
            || {
                let mut ob = OrderBook::<BenchOrder>::default();
                let mut setup_eval = Evaluator::default();
                let sell = BenchOrder::new(0, false, 1000, 100);
                for instr in setup_eval.eval(&ob, Op::Insert(sell)) {
                    ob.apply(instr);
                }
                let buy = BenchOrder::new(1, true, 1000, 100);
                (ob, Some(buy))
            },
            |(ob, buy)| {
                eval.reset();
                eval.eval_insert(ob, black_box(buy.take().unwrap()))
                    .for_each(|i| { black_box(i); });
            },
            BatchSize::LargeInput,
        );
    });

    let mut eval = Evaluator::default();
    group.bench_function("5_levels", |b| {
        b.iter_batched_ref(
            || {
                let mut ob = OrderBook::<BenchOrder>::default();
                let mut setup_eval = Evaluator::default();
                for i in 0..5u64 {
                    let sell = BenchOrder::new(i, false, 1000 + i, 10);
                    for instr in setup_eval.eval(&ob, Op::Insert(sell)) {
                        ob.apply(instr);
                    }
                }
                let buy = BenchOrder::new(5, true, 1005, 50);
                (ob, Some(buy))
            },
            |(ob, buy)| {
                eval.reset();
                eval.eval_insert(ob, black_box(buy.take().unwrap()))
                    .for_each(|i| { black_box(i); });
            },
            BatchSize::LargeInput,
        );
    });

    let mut eval = Evaluator::default();
    group.bench_function("10_levels", |b| {
        b.iter_batched_ref(
            || {
                let mut ob = OrderBook::<BenchOrder>::default();
                let mut setup_eval = Evaluator::default();
                for i in 0..10u64 {
                    let sell = BenchOrder::new(i, false, 1000 + i, 10);
                    for instr in setup_eval.eval(&ob, Op::Insert(sell)) {
                        ob.apply(instr);
                    }
                }
                let buy = BenchOrder::new(10, true, 1010, 100);
                (ob, Some(buy))
            },
            |(ob, buy)| {
                eval.reset();
                eval.eval_insert(ob, black_box(buy.take().unwrap()))
                    .for_each(|i| { black_box(i); });
            },
            BatchSize::LargeInput,
        );
    });

    group.finish();
}

/// Benchmark apply matching across varying number of makers.
fn bench_apply_match(c: &mut Criterion) {
    let mut group = c.benchmark_group("apply_match");
    group.throughput(Throughput::Elements(1));

    group.bench_function("1_level", |b| {
        b.iter_batched_ref(
            || {
                let mut ob = OrderBook::<BenchOrder>::default();
                let mut eval = Evaluator::default();
                let sell = BenchOrder::new(0, false, 1000, 100);
                for instr in eval.eval(&ob, Op::Insert(sell)) {
                    ob.apply(instr);
                }
                ob
            },
            |ob| black_box(ob.apply(Instruction::Fill(0, 0, 1000, 100, false))),
            BatchSize::LargeInput,
        );
    });

    group.bench_function("5_levels", |b| {
        b.iter_batched_ref(
            || {
                let mut ob = OrderBook::<BenchOrder>::default();
                let mut eval = Evaluator::default();
                for i in 0..5u64 {
                    let sell = BenchOrder::new(i, false, 1000 + i, 10);
                    for instr in eval.eval(&ob, Op::Insert(sell)) {
                        ob.apply(instr);
                    }
                }
                ob
            },
            |ob| {
                let fills: Vec<_> = (0..5)
                    .map(|i| Instruction::Fill(i, i, 1000 + i, 10, false))
                    .collect();
                for instr in fills {
                    black_box(ob.apply(instr));
                }
            },
            BatchSize::LargeInput,
        );
    });

    group.bench_function("10_levels", |b| {
        b.iter_batched_ref(
            || {
                let mut ob = OrderBook::<BenchOrder>::default();
                let mut eval = Evaluator::default();
                for i in 0..10u64 {
                    let sell = BenchOrder::new(i, false, 1000 + i, 10);
                    for instr in eval.eval(&ob, Op::Insert(sell)) {
                        ob.apply(instr);
                    }
                }
                ob
            },
            |ob| {
                let fills: Vec<_> = (0..10)
                    .map(|i| Instruction::Fill(i, i, 1000 + i, 10, false))
                    .collect();
                for instr in fills {
                    black_box(ob.apply(instr));
                }
            },
            BatchSize::LargeInput,
        );
    });

    group.finish();
}

criterion_group!(
    benches,
    bench_eval_insert,
    bench_apply_insert,
    bench_eval_cancel,
    bench_apply_cancel,
    bench_eval_match,
    bench_apply_match
);
criterion_main!(benches);
