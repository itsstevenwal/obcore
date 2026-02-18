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
| Insert    | Empty book  | 7.5 ns  | 43.6 ns | 51 ns (20 M/s)  |
| Insert    | Depth 100   | 11.6 ns | 31.0 ns | 43 ns (23 M/s)  |
| Insert    | Depth 1000  | 32.7 ns | 50.2 ns | 83 ns (12 M/s)  |
| Cancel    | Single      | 4.9 ns  | 21.4 ns | 26 ns (38 M/s)  |
| Cancel    | Depth 100   | 8.0 ns  | 38.1 ns | 46 ns (22 M/s)  |
| Cancel    | Depth 1000  | 24.1 ns | 66.1 ns | 90 ns (11 M/s)  |
| Match     | 1 level     | 20.9 ns | 77.8 ns | 99 ns (10 M/s)  |
| Match     | 5 levels    | 45.6 ns | 170 ns  | 216 ns (4.6 M/s) |
| Match     | 10 levels   | 88.7 ns | 304 ns  | 393 ns (2.5 M/s) |

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

