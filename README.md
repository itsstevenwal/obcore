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
| Insert    | Empty book  | 8.3 ns  | 39.8 ns | 48 ns (21 M/s)  |
| Insert    | Depth 100   | 11.3 ns | 29.7 ns | 41 ns (24 M/s)  |
| Insert    | Depth 1000  | 37.0 ns | 48.3 ns | 85 ns (12 M/s)  |
| Cancel    | Single      | 5.0 ns  | 15.4 ns | 20 ns (49 M/s)  |
| Cancel    | Depth 100   | 8.1 ns  | 30.1 ns | 38 ns (26 M/s)  |
| Cancel    | Depth 1000  | 23.4 ns | 32.8 ns | 56 ns (18 M/s)  |
| Match     | 1 level     | 20.1 ns | 70.8 ns | 91 ns (11 M/s)  |
| Match     | 5 levels    | 45.2 ns | 130 ns  | 175 ns (5.7 M/s) |
| Match     | 10 levels   | 90.8 ns | 221 ns  | 312 ns (3.2 M/s) |

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
    type T = u64;   // Order ID type
    type N = u64;   // Quantity type
    type Owner = u64;  // For self-trade prevention

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

