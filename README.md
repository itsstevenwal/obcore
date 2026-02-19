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
| Insert    | Empty book  | 6.0 ns  | 38.9 ns | 45 ns (22 M/s)  |
| Insert    | Depth 100   | 9.8 ns  | 30.9 ns | 41 ns (24 M/s)  |
| Insert    | Depth 1000  | 15.9 ns | 41.6 ns | 58 ns (17 M/s)  |
| Cancel    | Single      | 5.9 ns  | 15.0 ns | 21 ns (48 M/s)  |
| Cancel    | Depth 100   | 9.4 ns  | 32.3 ns | 42 ns (24 M/s)  |
| Cancel    | Depth 1000  | 17.3 ns | 25.2 ns | 43 ns (23 M/s)  |
| Match     | 1 level     | 16.9 ns | 15.8 ns | 33 ns (30 M/s)  |
| Match     | 5 levels    | 54.2 ns | 116 ns  | 170 ns (5.9 M/s) |
| Match     | 10 levels   | 107 ns  | 257 ns  | 364 ns (2.7 M/s) |

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

