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
| Insert    | Empty book  | 10.6 ns | 43.4 ns | 54 ns (19 M/s)  |
| Insert    | Depth 100   | 17.1 ns | 30.6 ns | 48 ns (21 M/s)  |
| Insert    | Depth 1000  | 36.7 ns | 46.9 ns | 84 ns (12 M/s)  |
| Cancel    | Single      | 4.8 ns  | 26.4 ns | 31 ns (32 M/s)  |
| Cancel    | Depth 100   | 9.0 ns  | 48.3 ns | 57 ns (18 M/s)  |
| Cancel    | Depth 1000  | 24.5 ns | 44.6 ns | 69 ns (14 M/s)  |
| Match     | 1 level     | 39.7 ns | 50.8 ns | 90 ns (11 M/s)  |
| Match     | 5 levels    | 102 ns  | 354 ns  | 456 ns (2.2 M/s) |
| Match     | 10 levels   | 189 ns  | 740 ns  | 929 ns (1.1 M/s) |

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

