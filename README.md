# obcore

A single threaded, zero dependency price-time priority orderbook implementation in Rust.

## design

The orderbook separates state evaluation from state mutation:

- **`eval`**: evaluates operations against current state, returns matches and instructions without modifying the book
- **`apply`**: takes instructions from eval and commits them to the book

## benchmark

Run with: `cargo bench --features bench`

| Operation | Benchmark   | Eval   | Apply  | Total   |
|-----------|-------------|--------|--------|---------|
| Insert    | Empty book  | 7.6 ns  | 39.7 ns | 47 ns (21 M/s)  |
| Insert    | Depth 100   | 11.9 ns | 30.1 ns | 42 ns (24 M/s)  |
| Insert    | Depth 1000  | 25.2 ns | 52.1 ns | 77 ns (13 M/s)  |
| Cancel    | Single      | 4.9 ns  | 15.3 ns | 20 ns (50 M/s)  |
| Cancel    | Depth 100   | 7.4 ns  | 26.6 ns | 34 ns (29 M/s)  |
| Cancel    | Depth 1000  | 22.4 ns | 33.1 ns | 55 ns (18 M/s)  |
| Match     | 1 level     | 23.8 ns | 71.9 ns | 96 ns (10 M/s)  |
| Match     | 5 levels    | 50.3 ns | 142 ns  | 192 ns (5.2 M/s) |
| Match     | 10 levels   | 99.0 ns | 266 ns  | 365 ns (2.7 M/s) |

*Total = Eval + Apply. Throughput in millions of ops/sec.*

## usage

Implement the `OrderInterface` trait for your order type:

```rust
use obcore::{Evaluator, OrderInterface, OrderBook, Op};

#[derive(Clone, Debug, PartialEq, Eq)]
struct MyOrder {
    id: u64,
    is_buy: bool,
    price: u64,
    quantity: u64,
    remaining: u64,
}

impl OrderInterface for MyOrder {
    type I = u64;   // Order ID type
    type N = u64;   // Numeric type
    type O = u64;   // Owner type for self-trade prevention

    fn id(&self) -> &u64 { &self.id }
    fn price(&self) -> u64 { self.price }
    fn is_buy(&self) -> bool { self.is_buy }
    fn quantity(&self) -> u64 { self.quantity }
    fn remaining(&self) -> u64 { self.remaining }
    fn fill(&mut self, quantity: u64) { self.remaining -= quantity; }
    fn owner(&self) -> &u64 { &self.id }
}

let mut ob = OrderBook::<MyOrder>::default();
let mut eval = Evaluator::default();

// Evaluate without mutating the book
let instructions = eval.eval(&ob, vec![Op::Insert(order)]);

// Apply instructions to commit state changes
ob.apply(instructions);
```

## license

Apache-2.0 or MIT

