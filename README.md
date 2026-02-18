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
| Insert    | Empty book  | 10.8 ns | 56.3 ns | 67 ns (15 M/s)  |
| Insert    | Depth 100   | 16.5 ns | 35.2 ns | 52 ns (19 M/s)  |
| Insert    | Depth 1000  | 32.4 ns | 48.0 ns | 80 ns (13 M/s)  |
| Cancel    | Single      | 4.9 ns  | 27.2 ns | 32 ns (31 M/s)  |
| Cancel    | Depth 100   | 9.0 ns  | 48.9 ns | 58 ns (17 M/s)  |
| Cancel    | Depth 1000  | 24.6 ns | 54.0 ns | 79 ns (13 M/s)  |
| Match     | 1 level     | 39.3 ns | 83.3 ns | 123 ns (8.1 M/s)  |
| Match     | 5 levels    | 103 ns  | 177 ns  | 280 ns (3.6 M/s) |
| Match     | 10 levels   | 189 ns  | 318 ns  | 507 ns (2.0 M/s) |

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

