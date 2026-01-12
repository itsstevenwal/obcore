# obcore

A single threaded, zero dependency price-time priority orderbook implementation in Rust.

## design

The orderbook separates state evaluation from state mutation:

- **`eval`**: evaluates operations against current state, returns matches and instructions without modifying the book
- **`apply`**: takes instructions from eval and commits them to the book

## benchmark

| Operation | Benchmark | Eval | Apply | Total* |
|-----------|-----------|------|-------|--------|
| Insert | Empty book | 22.0 ns | 95.5 ns | 117.5 ns (8.5 M/s) |
| Insert | Depth 100 | 28.2 ns | 82.6 ns | 110.8 ns (9.0 M/s) |
| Insert | Depth 1000 | 37.1 ns | 57.6 ns | 94.7 ns (10.6 M/s) |
| Cancel | Single | 23.2 ns | 37.0 ns | 60.2 ns (16.6 M/s) |
| Cancel | Depth 100 | 24.5 ns | 52.2 ns | 76.7 ns (13.0 M/s) |
| Cancel | Depth 1000 | 35.3 ns | 63.0 ns | 98.3 ns (10.2 M/s) |
| Match | 1 level | 184.4 ns | 55.1 ns | 239.5 ns (4.2 M/s) |
| Match | 5 levels | 138.8 ns | 145.0 ns | 283.8 ns (3.5 M/s) |
| Match | 10 levels | 274.5 ns | 274.5 ns | 549.0 ns (1.8 M/s) |

*Total = Eval + Apply. Measured on Apple Silicon M4 Max.*

## usage

Implement the `OrderInterface` trait for your order type:

```rust
use obcore::{OrderInterface, OrderBook, Op};

#[derive(Clone, Debug, PartialEq, Eq)]
struct MyOrder {
    id: u64,
    is_buy: bool,
    price: u64,
    quantity: u64,
    remaining: u64,
}

impl OrderInterface for MyOrder {
    type T = u64;  // Order ID type
    type N = u64;  // Quantity type

    fn id(&self) -> &u64 { &self.id }
    fn price(&self) -> u64 { self.price }
    fn is_buy(&self) -> bool { self.is_buy }
    fn quantity(&self) -> u64 { self.quantity }
    fn remaining(&self) -> u64 { self.remaining }
    fn fill(&mut self, quantity: u64) { self.remaining -= quantity; }
}

// Create orderbook and process orders
let mut ob = OrderBook::<MyOrder>::default();

// Evaluate result on insertion
let (matches, instructions) = ob.eval(vec![Op::Insert(order)]);

// Apply state changes
ob.apply(instructions);
```

## license

Apache-2.0 or MIT

