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
| Insert    | Empty book  | 11.3 ns | 39.2 ns | 50 ns (20 M/s)  |
| Insert    | Depth 100   | 15.1 ns | 29.1 ns | 44 ns (23 M/s)  |
| Insert    | Depth 1000  | 29.7 ns | 39.6 ns | 69 ns (14 M/s)  |
| Cancel    | Single      | 13.3 ns | 16.9 ns | 30 ns (33 M/s)  |
| Cancel    | Depth 100   | 15.4 ns | 31.6 ns | 47 ns (21 M/s)  |
| Cancel    | Depth 1000  | 24.2 ns | 27.3 ns | 51 ns (20 M/s)  |
| Match     | 1 level     | 22.7 ns | 16.9 ns | 40 ns (25 M/s)  |
| Match     | 5 levels    | 48.7 ns | 115 ns  | 164 ns (6.1 M/s) |
| Match     | 10 levels   | 93.7 ns | 241 ns  | 335 ns (3.0 M/s) |

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
let instructions = eval.eval(&ob, Op::Insert(order));

// Apply instructions to commit state changes
for instr in instructions {
    ob.apply(instr);
}
```

## license

Apache-2.0 or MIT

