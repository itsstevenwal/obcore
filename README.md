# obcore

A single threaded, zero dependency price-time priority orderbook implementation in Rust.

## design

The orderbook separates state evaluation from state mutation:

- **`eval`**: evaluates operations against current state, returns matches and instructions without modifying the book
- **`apply`**: takes instructions from eval and commits them to the book

## benchmark

Run with: `cargo bench --features bench`

| Operation | Benchmark   | Eval   | Apply  | Total*   |
|-----------|-------------|--------|--------|----------|
| Insert    | Empty book  | 9.4 ns  | 54.6 ns | 64 ns (16 M/s)  |
| Insert    | Depth 100   | 23.9 ns | 36.1 ns | 60 ns (17 M/s)  |
| Insert    | Depth 1000  | 47.2 ns | 56.6 ns | 104 ns (9.6 M/s) |
| Cancel    | Single      | 5.1 ns  | 23.6 ns | 29 ns (34 M/s)  |
| Cancel    | Depth 100   | 8.2 ns  | 40.3 ns | 48 ns (21 M/s)  |
| Cancel    | Depth 1000  | 24.1 ns | 53.3 ns | 77 ns (13 M/s)  |
| Match     | 1 level     | 48.9 ns | 58.3 ns | 107 ns (9.3 M/s) |
| Match     | 5 levels    | 114 ns  | 339 ns  | 453 ns (2.2 M/s) |
| Match     | 10 levels   | 202 ns  | 719 ns  | 921 ns (1.1 M/s) |

*Total = Eval + Apply. Throughput in millions of ops/sec. Match apply is per fill.*

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

