# obcore

A single threaded, zero dependency price-time priority orderbook implementation in Rust.

## design

The orderbook separates state evaluation from state mutation:

- **`eval`**: evaluates operations against current state, returns matches and instructions without modifying the book
- **`apply`**: takes instructions from eval and commits them to the book

## benchmark

Run with: `cargo bench --features bench`

| Operation | Benchmark | Eval | Apply | Total* |
|-----------|-----------|------|-------|--------|
| Insert | Empty book | 7.8 ns | 60.3 ns | 68 ns (15 M/s) |
| Insert | Depth 100 | 17.1 ns | 37.4 ns | 55 ns (18 M/s) |
| Insert | Depth 1000 | 37.2 ns | 63.1 ns | 100 ns (10 M/s) |
| Cancel | Single | 6.2 ns | 30.6 ns | 37 ns (27 M/s) |
| Cancel | Depth 100 | 11.1 ns | 42.9 ns | 54 ns (19 M/s) |
| Cancel | Depth 1000 | 31.3 ns | 71.0 ns | 102 ns (9.8 M/s) |
| Match | 1 level | 31.6 ns | 64.4 ns | 96 ns (10 M/s) |
| Match | 5 levels | 106 ns | 375 ns | 481 ns (2.1 M/s) |
| Match | 10 levels | 183 ns | 765 ns | 948 ns (1.1 M/s) |

*Total = Eval + Apply. Throughput in millions of ops/sec.*

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

