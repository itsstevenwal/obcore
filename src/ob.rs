use crate::eval::Instruction;
#[cfg(feature = "bench")]
use crate::eval::Msg;
use crate::{
    hash::FxHashMap,
    list::{Node, Pool},
    order::OrderInterface,
    side::Side,
};

/// A complete orderbook with bid and ask sides.
pub struct OrderBook<O: OrderInterface> {
    pub(crate) bids: Side<O>,
    pub(crate) asks: Side<O>,
    pub(crate) orders: FxHashMap<O::I, *mut Node<O>>,
    pub(crate) pool: Pool<O>,
}

impl<O: OrderInterface> Default for OrderBook<O> {
    fn default() -> Self {
        Self {
            bids: Side::new(true),
            asks: Side::new(false),
            orders: FxHashMap::default(),
            pool: Pool::new(),
        }
    }
}

#[derive(Debug, PartialEq, Eq, Clone)]
pub enum Output<O: OrderInterface> {
    // Resting quantity
    Inserted(O::N),
    // Deleted
    Deleted,
    // Partial fill
    Partial,
    // Filled
    Filled,
    // No operation
    NoOp,
}

// ─────────────────────────────────────────────────────────────────────────────
// OrderBook Implementation
// ─────────────────────────────────────────────────────────────────────────────

impl<O: OrderInterface> OrderBook<O> {
    // ─────────────────────────────────────────────────────────────────────────
    // Getters
    // ─────────────────────────────────────────────────────────────────────────

    /// Returns an iterator over all bid levels, highest price first.
    #[inline]
    pub fn bids(&self) -> impl Iterator<Item = &crate::level::Level<O>> {
        self.bids.iter()
    }

    /// Returns an iterator over all ask levels, lowest price first.
    #[inline]
    pub fn asks(&self) -> impl Iterator<Item = &crate::level::Level<O>> {
        self.asks.iter()
    }

    /// Returns the best (highest) bid as (price, total_quantity), if any.
    #[inline]
    pub fn best_bid(&self) -> Option<(O::N, O::N)> {
        self.bids.best()
    }

    /// Returns the best (lowest) ask as (price, total_quantity), if any.
    #[inline]
    pub fn best_ask(&self) -> Option<(O::N, O::N)> {
        self.asks.best()
    }

    /// Returns the top `n` bid levels as (price, total_quantity), highest price first.
    #[inline]
    pub fn top_bids(&self, n: usize) -> Vec<(O::N, O::N)> {
        self.bids.top(n)
    }

    /// Returns the top `n` ask levels as (price, total_quantity), lowest price first.
    #[inline]
    pub fn top_asks(&self, n: usize) -> Vec<(O::N, O::N)> {
        self.asks.top(n)
    }

    /// Returns the number of price levels on the bid side.
    #[inline]
    pub fn bid_depth(&self) -> usize {
        self.bids.height()
    }

    /// Returns the number of price levels on the ask side.
    #[inline]
    pub fn ask_depth(&self) -> usize {
        self.asks.height()
    }

    /// Returns the total number of orders in the book.
    #[inline]
    pub fn len(&self) -> usize {
        self.orders.len()
    }

    /// Returns true if the orderbook has no orders.
    #[inline]
    pub fn is_empty(&self) -> bool {
        self.orders.is_empty()
    }

    /// Returns a reference to the order with the given ID, if it exists.
    #[inline]
    pub fn order(&self, order_id: &O::I) -> Option<&O> {
        self.orders
            .get(order_id)
            .map(|&ptr| unsafe { &(*ptr).data })
    }

    // ─────────────────────────────────────────────────────────────────────────
    // Internal helpers
    // ─────────────────────────────────────────────────────────────────────────

    /// Applies a single insert instruction. Only available with the `bench` feature.
    #[cfg(feature = "bench")]
    #[inline(always)]
    pub fn apply_insert(&mut self, order: O, remaining: O::N) -> Output<O> {
        self.apply(Instruction::Insert(order, remaining))
    }

    /// Applies a single delete instruction. Only available with the `bench` feature.
    #[cfg(feature = "bench")]
    #[inline(always)]
    pub fn apply_delete(&mut self, order_id: O::I) -> Output<O> {
        self.apply(Instruction::Delete(order_id, Msg::UserCancelled))
    }

    /// Applies a single instruction to the orderbook, mutating state.
    #[inline]
    pub fn apply(&mut self, instruction: Instruction<O>) -> Output<O> {
        let Self {
            bids,
            asks,
            orders,
            pool,
        } = self;
        match instruction {
            Instruction::Fill(order_id, _, _price, quantity, is_taker) => {
                if is_taker {
                    return Output::Filled;
                }
                let &node_ptr = orders.get(&order_id).unwrap();
                let is_buy = unsafe { (*node_ptr).data.is_buy() };
                let side = if is_buy { bids } else { asks };
                let removed = side.fill_order(node_ptr, quantity, pool);
                if removed {
                    orders.remove(&order_id);
                    Output::Filled
                } else {
                    Output::Partial
                }
            }
            Instruction::Insert(mut order, remaining) => {
                if remaining > O::N::default() {
                    let filled = order.quantity() - remaining;
                    if filled > O::N::default() {
                        order.fill(filled);
                    }
                    let id = order.id().clone();
                    let is_buy = order.is_buy();
                    let side = if is_buy { bids } else { asks };
                    let node_ptr = side.insert_order(order, pool);
                    orders.insert(id, node_ptr);
                }
                Output::Inserted(remaining)
            }
            Instruction::Delete(order_id, _msg) => {
                if let Some(&node_ptr) = orders.get(&order_id) {
                    let is_buy = unsafe { (*node_ptr).data.is_buy() };
                    let side = if is_buy { bids } else { asks };
                    side.remove_order(node_ptr, pool);
                    orders.remove(&order_id);
                }
                Output::Deleted
            }
            Instruction::NoOp(_, _) => Output::NoOp,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::eval::{Evaluator, Instruction, Msg, Op};
    use crate::order::{STP, TIF, TestOrder};

    fn setup_order(ob: &mut OrderBook<TestOrder>, id: &str, is_buy: bool, price: u64, qty: u64) {
        let order = TestOrder::new(id, is_buy, price, qty);
        let OrderBook {
            bids,
            asks,
            orders,
            pool,
        } = ob;
        let side = if is_buy { bids } else { asks };
        let node_ptr = side.insert_order(order, pool);
        orders.insert(String::from(id), node_ptr);
    }

    fn setup_order_with_owner(
        ob: &mut OrderBook<TestOrder>,
        id: &str,
        is_buy: bool,
        price: u64,
        qty: u64,
        owner: &str,
    ) {
        let order = TestOrder::new(id, is_buy, price, qty).with_owner(owner);
        let OrderBook {
            bids,
            asks,
            orders,
            pool,
        } = ob;
        let side = if is_buy { bids } else { asks };
        let node_ptr = side.insert_order(order, pool);
        orders.insert(String::from(id), node_ptr);
    }

    // ─────────────────────────────────────────────────────────────────────────
    // Getter Tests
    // ─────────────────────────────────────────────────────────────────────────

    #[test]
    fn test_empty_book() {
        let ob = OrderBook::<TestOrder>::default();
        assert!(ob.is_empty());
        assert_eq!(ob.len(), 0);
        assert_eq!(ob.bid_depth(), 0);
        assert_eq!(ob.ask_depth(), 0);
        assert!(ob.best_bid().is_none());
        assert!(ob.best_ask().is_none());
        assert!(ob.top_bids(5).is_empty());
        assert!(ob.top_asks(5).is_empty());
    }

    #[test]
    fn test_best_bid() {
        let mut ob = OrderBook::<TestOrder>::default();
        setup_order(&mut ob, "b1", true, 100, 50);
        setup_order(&mut ob, "b2", true, 100, 30); // same price
        setup_order(&mut ob, "b3", true, 90, 20);

        let (price, qty) = ob.best_bid().unwrap();
        assert_eq!(price, 100);
        assert_eq!(qty, 80); // 50 + 30 at price 100
    }

    #[test]
    fn test_best_ask() {
        let mut ob = OrderBook::<TestOrder>::default();
        setup_order(&mut ob, "s1", false, 100, 50);
        setup_order(&mut ob, "s2", false, 100, 30); // same price
        setup_order(&mut ob, "s3", false, 110, 20);

        let (price, qty) = ob.best_ask().unwrap();
        assert_eq!(price, 100);
        assert_eq!(qty, 80); // 50 + 30 at price 100
    }

    #[test]
    fn test_top_bids() {
        let mut ob = OrderBook::<TestOrder>::default();
        setup_order(&mut ob, "b1", true, 100, 50);
        setup_order(&mut ob, "b2", true, 90, 30);
        setup_order(&mut ob, "b3", true, 80, 20);

        let levels = ob.top_bids(2);
        assert_eq!(levels.len(), 2);
        assert_eq!(levels[0], (100, 50)); // highest first
        assert_eq!(levels[1], (90, 30));

        // Request more than available
        let levels = ob.top_bids(10);
        assert_eq!(levels.len(), 3);
    }

    #[test]
    fn test_top_asks() {
        let mut ob = OrderBook::<TestOrder>::default();
        setup_order(&mut ob, "s1", false, 100, 50);
        setup_order(&mut ob, "s2", false, 110, 30);
        setup_order(&mut ob, "s3", false, 120, 20);

        let levels = ob.top_asks(2);
        assert_eq!(levels.len(), 2);
        assert_eq!(levels[0], (100, 50)); // lowest first
        assert_eq!(levels[1], (110, 30));

        // Request more than available
        let levels = ob.top_asks(10);
        assert_eq!(levels.len(), 3);
    }

    #[test]
    fn test_depth() {
        let mut ob = OrderBook::<TestOrder>::default();
        setup_order(&mut ob, "b1", true, 100, 50);
        setup_order(&mut ob, "b2", true, 100, 30); // same level
        setup_order(&mut ob, "b3", true, 90, 20);
        setup_order(&mut ob, "s1", false, 110, 40);

        assert_eq!(ob.bid_depth(), 2); // 2 price levels
        assert_eq!(ob.ask_depth(), 1);
    }

    #[test]
    fn test_len_and_is_empty() {
        let mut ob = OrderBook::<TestOrder>::default();
        assert!(ob.is_empty());
        assert_eq!(ob.len(), 0);

        setup_order(&mut ob, "b1", true, 100, 50);
        assert!(!ob.is_empty());
        assert_eq!(ob.len(), 1);

        setup_order(&mut ob, "b2", true, 100, 30);
        setup_order(&mut ob, "s1", false, 110, 40);
        assert_eq!(ob.len(), 3);
    }

    #[test]
    fn test_order_lookup() {
        let mut ob = OrderBook::<TestOrder>::default();
        setup_order(&mut ob, "b1", true, 100, 50);
        setup_order(&mut ob, "s1", false, 110, 40);

        let order = ob.order(&String::from("b1")).unwrap();
        assert_eq!(order.price(), 100);
        assert_eq!(order.remaining(), 50);

        let order = ob.order(&String::from("s1")).unwrap();
        assert_eq!(order.price(), 110);

        assert!(ob.order(&String::from("nonexistent")).is_none());
    }

    #[test]
    fn test_bids_asks_iterators() {
        let mut ob = OrderBook::<TestOrder>::default();
        setup_order(&mut ob, "b1", true, 100, 50);
        setup_order(&mut ob, "b2", true, 90, 30);
        setup_order(&mut ob, "s1", false, 110, 40);
        setup_order(&mut ob, "s2", false, 120, 20);

        // Bids: highest price first
        let bid_prices: Vec<u64> = ob.bids().map(|o| o.price()).collect();
        assert_eq!(bid_prices, vec![100, 90]);

        // Asks: lowest price first
        let ask_prices: Vec<u64> = ob.asks().map(|o| o.price()).collect();
        assert_eq!(ask_prices, vec![110, 120]);
    }

    // ─────────────────────────────────────────────────────────────────────────
    // Eval Tests
    // ─────────────────────────────────────────────────────────────────────────

    #[test]
    fn test_eval_insert_no_match() {
        let ob = OrderBook::<TestOrder>::default();
        let mut eval = Evaluator::default();
        let order = TestOrder::new("1", true, 1000, 100);
        let i: Vec<_> = eval.eval(&ob, Op::Insert(order.clone())).collect();
        assert_eq!(i, vec![Instruction::Insert(order, 100)]);

        let ob = OrderBook::<TestOrder>::default();
        let mut eval = Evaluator::default();
        let order = TestOrder::new("1", false, 1000, 50);
        let i: Vec<_> = eval.eval(&ob, Op::Insert(order.clone())).collect();
        assert_eq!(i, vec![Instruction::Insert(order, 50)]);
    }

    #[test]
    fn test_eval_insert_duplicate() {
        let mut ob = OrderBook::<TestOrder>::default();
        setup_order(&mut ob, "1", true, 1000, 100);
        let mut eval = Evaluator::default();
        let i: Vec<_> = eval.eval(&ob, Op::Insert(TestOrder::new("1", true, 1000, 50))).collect();
        assert_eq!(
            i,
            vec![Instruction::NoOp(
                String::from("1"),
                Msg::OrderAlreadyExists
            )]
        );
    }

    #[test]
    fn test_eval_cancel() {
        let mut ob = OrderBook::<TestOrder>::default();
        let mut eval = Evaluator::default();
        let i: Vec<_> = eval.eval(&ob, Op::Delete(String::from("x"))).collect();
        assert_eq!(
            i,
            vec![Instruction::NoOp(String::from("x"), Msg::OrderNotFound)]
        );

        setup_order(&mut ob, "1", true, 1000, 100);
        let mut eval = Evaluator::default();
        let i: Vec<_> = eval.eval(&ob, Op::Delete(String::from("1"))).collect();
        assert_eq!(
            i,
            vec![Instruction::Delete(String::from("1"), Msg::UserCancelled)]
        );
    }

    #[test]
    fn test_eval_matching() {
        let mut ob = OrderBook::<TestOrder>::default();
        setup_order(&mut ob, "s1", false, 1000, 100);
        let order = TestOrder::new("b1", true, 1000, 100);
        let mut eval = Evaluator::default();
        let i: Vec<_> = eval.eval(&ob, Op::Insert(order.clone())).collect();
        assert_eq!(
            i,
            vec![
                Instruction::Fill(String::from("b1"), String::from("b1"), 1000, 100, true),
                Instruction::Fill(String::from("s1"), String::from("s1"), 1000, 100, false),
            ]
        );

        let mut ob = OrderBook::<TestOrder>::default();
        setup_order(&mut ob, "s1", false, 1000, 50);
        let order = TestOrder::new("b1", true, 1000, 100);
        let mut eval = Evaluator::default();
        let i: Vec<_> = eval.eval(&ob, Op::Insert(order.clone())).collect();
        assert_eq!(
            i,
            vec![
                Instruction::Fill(String::from("b1"), String::from("b1"), 1000, 50, true),
                Instruction::Fill(String::from("s1"), String::from("s1"), 1000, 50, false),
                Instruction::Insert(order, 50),
            ]
        );
    }

    #[test]
    fn test_eval_price_crossing() {
        // Buy doesn't match higher sell
        let mut ob = OrderBook::<TestOrder>::default();
        setup_order(&mut ob, "s1", false, 1100, 100);
        let mut eval = Evaluator::default();
        let i: Vec<_> = eval.eval(&ob, Op::Insert(TestOrder::new("b1", true, 1000, 100))).collect();
        assert!(matches!(i.as_slice(), [Instruction::Insert(_, _)]));

        // Buy at higher price matches lower sell
        let mut ob = OrderBook::<TestOrder>::default();
        setup_order(&mut ob, "s1", false, 1000, 100);
        let mut eval = Evaluator::default();
        let order = TestOrder::new("b1", true, 1100, 100);
        let i: Vec<_> = eval.eval(&ob, Op::Insert(order.clone())).collect();
        assert_eq!(
            i,
            vec![
                Instruction::Fill(String::from("b1"), String::from("b1"), 1000, 100, true),
                Instruction::Fill(String::from("s1"), String::from("s1"), 1000, 100, false),
            ]
        );

        // Sell doesn't match lower buy
        let mut ob = OrderBook::<TestOrder>::default();
        setup_order(&mut ob, "b1", true, 1000, 100);
        let mut eval = Evaluator::default();
        let i: Vec<_> = eval.eval(&ob, Op::Insert(TestOrder::new("s1", false, 1100, 100))).collect();
        assert!(matches!(i.as_slice(), [Instruction::Insert(_, _)]));

        // Sell at lower price matches higher buy
        let mut ob = OrderBook::<TestOrder>::default();
        setup_order(&mut ob, "b1", true, 1100, 100);
        let order = TestOrder::new("s1", false, 1000, 100);
        let mut eval = Evaluator::default();
        let i: Vec<_> = eval.eval(&ob, Op::Insert(order.clone())).collect();
        assert_eq!(
            i,
            vec![
                Instruction::Fill(String::from("s1"), String::from("s1"), 1100, 100, true),
                Instruction::Fill(String::from("b1"), String::from("b1"), 1100, 100, false),
            ]
        );
    }

    #[test]
    fn test_eval_multi_maker_match() {
        let mut ob = OrderBook::<TestOrder>::default();
        setup_order(&mut ob, "b1", true, 1100, 30);
        setup_order(&mut ob, "b2", true, 1050, 40);
        let mut eval = Evaluator::default();
        let i: Vec<_> = eval.eval(&ob, Op::Insert(TestOrder::new("s1", false, 1000, 100))).collect();
        assert!(
            i.iter()
                .any(|p| matches!(p, Instruction::Insert(_, r) if *r == 30))
        );
    }

    #[test]
    fn test_eval_quantity_exhausted() {
        let mut ob = OrderBook::<TestOrder>::default();
        setup_order(&mut ob, "s1", false, 1000, 50);
        setup_order(&mut ob, "s2", false, 1000, 50);
        let mut eval = Evaluator::default();
        let i: Vec<_> = eval.eval(&ob, Op::Insert(TestOrder::new("b1", true, 1000, 50))).collect();
        assert!(i.iter().any(|p| matches!(p, Instruction::Fill(..))));

        let mut ob = OrderBook::<TestOrder>::default();
        setup_order(&mut ob, "b1", true, 1000, 50);
        setup_order(&mut ob, "b2", true, 1000, 50);
        let mut eval = Evaluator::default();
        let i: Vec<_> = eval.eval(&ob, Op::Insert(TestOrder::new("s1", false, 1000, 50))).collect();
        assert!(i.iter().any(|p| matches!(p, Instruction::Fill(..))));
    }

    #[test]
    fn test_eval_with_ops() {
        let ob = OrderBook::<TestOrder>::default();
        let mut eval = Evaluator::default();
        let i1: Vec<_> = eval.eval(&ob, Op::Insert(TestOrder::new("b1", true, 1000, 100))).collect();
        let i2: Vec<_> = eval.eval(&ob, Op::Insert(TestOrder::new("s1", false, 1100, 50))).collect();
        let i3: Vec<_> = eval.eval(&ob, Op::Delete(String::from("b1"))).collect();
        assert!(matches!(i1.as_slice(), [Instruction::Insert(_, _)]));
        assert!(matches!(i2.as_slice(), [Instruction::Insert(_, _)]));
        assert!(matches!(
            i3.as_slice(),
            [Instruction::NoOp(_, Msg::OrderNotFound)]
        ));
    }

    #[test]
    fn test_temp_state() {
        let mut ob = OrderBook::<TestOrder>::default();
        setup_order(&mut ob, "s1", false, 1000, 100);

        // Across eval calls, temp state is tracked (until reset)
        let mut eval = Evaluator::default();
        let i1: Vec<_> = eval.eval(&ob, Op::Insert(TestOrder::new("b1", true, 1000, 30))).collect();
        let i2: Vec<_> = eval.eval(&ob, Op::Insert(TestOrder::new("b2", true, 1000, 20))).collect();
        let i3: Vec<_> = eval.eval(&ob, Op::Delete(String::from("s1"))).collect();
        let i4: Vec<_> = eval.eval(&ob, Op::Insert(TestOrder::new("b3", true, 1000, 50))).collect();
        // b1: Match(b1, [(s1, 30)])
        // b2: Match(b2, [(s1, 20)])
        // s1: delete
        // b3: insert (no match since s1 was deleted in temp state)
        assert!(i1.iter().any(|p| matches!(p, Instruction::Fill(..))));
        assert!(i2.iter().any(|p| matches!(p, Instruction::Fill(..))));
        assert!(matches!(i3.as_slice(), [Instruction::Delete(..)]));
        assert!(matches!(i4.as_slice(), [Instruction::Insert(..)]));
    }

    // ─────────────────────────────────────────────────────────────────────────
    // Apply Tests
    // ─────────────────────────────────────────────────────────────────────────

    #[test]
    fn test_apply_insert() {
        let mut ob = OrderBook::<TestOrder>::default();
        ob.apply(Instruction::Insert(
            TestOrder::new("1", true, 1000, 100),
            100,
        ));
        assert!(ob.orders.contains_key("1"));

        let mut ob = OrderBook::<TestOrder>::default();
        ob.apply(Instruction::Insert(
            TestOrder::new("1", false, 1000, 100),
            100,
        ));
        assert!(!ob.asks.is_empty());

        let mut ob = OrderBook::<TestOrder>::default();
        ob.apply(Instruction::Insert(
            TestOrder::new("1", true, 1000, 100),
            70,
        ));
        assert_eq!(ob.order(&String::from("1")).unwrap().remaining(), 70);
    }

    #[test]
    fn test_apply_delete() {
        let mut ob = OrderBook::<TestOrder>::default();
        setup_order(&mut ob, "1", true, 1000, 100);
        ob.apply(Instruction::Delete(String::from("1"), Msg::UserCancelled));
        assert!(ob.bids.is_empty());

        let mut ob = OrderBook::<TestOrder>::default();
        setup_order(&mut ob, "1", false, 1000, 100);
        ob.apply(Instruction::Delete(String::from("1"), Msg::UserCancelled));
        assert!(ob.asks.is_empty());

        // Non-existent (no panic)
        let mut ob = OrderBook::<TestOrder>::default();
        ob.apply(Instruction::Delete(String::from("x"), Msg::OrderNotFound));
    }

    #[test]
    fn test_apply_match() {
        // Partial fill sell (taker "t1", maker "1" filled 30)
        let mut ob = OrderBook::<TestOrder>::default();
        setup_order(&mut ob, "1", false, 1000, 100);
        let t1 = TestOrder::new("t1", true, 1000, 100);
        for instr in vec![
            Instruction::Fill(String::from("1"), String::from("1"), 1000, 30, false),
            Instruction::Insert(t1, 70),
        ] {
            ob.apply(instr);
        }
        assert_eq!(ob.order(&String::from("1")).unwrap().remaining(), 70);

        // Complete fill sell
        let mut ob = OrderBook::<TestOrder>::default();
        setup_order(&mut ob, "1", false, 1000, 100);
        let t1 = TestOrder::new("t1", true, 1000, 100);
        for instr in vec![
            Instruction::Fill(String::from("1"), String::from("1"), 1000, 100, false),
            Instruction::Insert(t1, 0),
        ] {
            ob.apply(instr);
        }
        assert!(ob.asks.is_empty());

        // Partial fill buy
        let mut ob = OrderBook::<TestOrder>::default();
        setup_order(&mut ob, "1", true, 1000, 100);
        let t1 = TestOrder::new("t1", false, 1000, 100);
        for instr in vec![
            Instruction::Fill(String::from("1"), String::from("1"), 1000, 30, false),
            Instruction::Insert(t1, 70),
        ] {
            ob.apply(instr);
        }
        assert_eq!(ob.order(&String::from("1")).unwrap().remaining(), 70);

        // Complete fill buy
        let mut ob = OrderBook::<TestOrder>::default();
        setup_order(&mut ob, "1", true, 1000, 100);
        let t1 = TestOrder::new("t1", false, 1000, 100);
        for instr in vec![
            Instruction::Fill(String::from("1"), String::from("1"), 1000, 100, false),
            Instruction::Insert(t1, 0),
        ] {
            ob.apply(instr);
        }
        assert!(ob.bids.is_empty());

        // Empty maker list (no panic)
        let mut ob = OrderBook::<TestOrder>::default();
        let x = TestOrder::new("x", true, 1000, 100);
        ob.apply(Instruction::Insert(x, 0));
    }

    #[test]
    fn test_apply_noop() {
        let mut ob = OrderBook::<TestOrder>::default();
        ob.apply(Instruction::Delete(String::from("x"), Msg::OrderNotFound));
        ob.apply(Instruction::Delete(
            String::from("y"),
            Msg::OrderAlreadyExists,
        ));
        assert!(ob.bids.is_empty());
    }

    // ─────────────────────────────────────────────────────────────────────────
    // Integration Tests
    // ─────────────────────────────────────────────────────────────────────────

    #[test]
    fn test_eval_then_apply() {
        let mut ob = OrderBook::<TestOrder>::default();
        setup_order(&mut ob, "s1", false, 1000, 100);
        let mut eval = Evaluator::default();
        let instructions: Vec<_> = eval.eval(&ob, Op::Insert(TestOrder::new("b1", true, 1000, 60))).collect();
        let filled: u64 = instructions
            .iter()
            .filter_map(|p| match p {
                Instruction::Fill(_, _, _, q, false) => Some(*q),
                _ => None,
            })
            .sum();
        assert_eq!(filled, 60);
        for instr in instructions {
            ob.apply(instr);
        }
        assert_eq!(ob.order(&String::from("s1")).unwrap().remaining(), 40);
        assert!(!ob.orders.contains_key("b1"));
    }

    #[test]
    fn test_eval_then_apply_with_insert() {
        let mut ob = OrderBook::<TestOrder>::default();
        setup_order(&mut ob, "s1", false, 1000, 50);
        let mut eval = Evaluator::default();
        for instr in eval.eval(&ob, Op::Insert(TestOrder::new("b1", true, 1000, 100))) {
            ob.apply(instr);
        }
        assert!(ob.asks.is_empty());
        assert_eq!(ob.order(&String::from("b1")).unwrap().remaining(), 50);
    }

    // ─────────────────────────────────────────────────────────────────────────
    // TIF and post_only tests
    // ─────────────────────────────────────────────────────────────────────────

    #[test]
    fn test_fok_full_fill() {
        let mut ob = OrderBook::<TestOrder>::default();
        setup_order(&mut ob, "s1", false, 1000, 100);
        let order = TestOrder::new("b1", true, 1000, 100).with_tif(TIF::FOK);
        let mut eval = Evaluator::default();
        let i: Vec<_> = eval.eval(&ob, Op::Insert(order.clone())).collect();
        assert_eq!(
            i,
            vec![
                Instruction::Fill(String::from("b1"), String::from("b1"), 1000, 100, true),
                Instruction::Fill(String::from("s1"), String::from("s1"), 1000, 100, false),
            ]
        );
    }

    #[test]
    fn test_fok_partial_reject() {
        let mut ob = OrderBook::<TestOrder>::default();
        setup_order(&mut ob, "s1", false, 1000, 50);
        let order = TestOrder::new("b1", true, 1000, 100).with_tif(TIF::FOK);
        let mut eval = Evaluator::default();
        let i: Vec<_> = eval.eval(&ob, Op::Insert(order)).collect();
        assert_eq!(
            i,
            vec![Instruction::NoOp(String::from("b1"), Msg::FOKNotFilled)]
        );
    }

    #[test]
    fn test_ioc_partial_fill() {
        let mut ob = OrderBook::<TestOrder>::default();
        setup_order(&mut ob, "s1", false, 1000, 50);
        let order = TestOrder::new("b1", true, 1000, 100).with_tif(TIF::IOC);
        let mut eval = Evaluator::default();
        let i: Vec<_> = eval.eval(&ob, Op::Insert(order.clone())).collect();
        assert_eq!(
            i,
            vec![
                Instruction::Fill(String::from("b1"), String::from("b1"), 1000, 50, true),
                Instruction::Fill(String::from("s1"), String::from("s1"), 1000, 50, false),
                Instruction::Delete(String::from("b1"), Msg::IOCLeftover),
            ]
        );
    }

    #[test]
    fn test_ioc_no_match() {
        let ob = OrderBook::<TestOrder>::default();
        let order = TestOrder::new("b1", true, 1000, 100).with_tif(TIF::IOC);
        let mut eval = Evaluator::default();
        let i: Vec<_> = eval.eval(&ob, Op::Insert(order)).collect();
        assert_eq!(
            i,
            vec![Instruction::NoOp(String::from("b1"), Msg::IOCNoFill)]
        );
    }

    #[test]
    fn test_gtc_unchanged() {
        let mut ob = OrderBook::<TestOrder>::default();
        setup_order(&mut ob, "s1", false, 1000, 50);
        let order = TestOrder::new("b1", true, 1000, 100); // GTC default
        let mut eval = Evaluator::default();
        let i: Vec<_> = eval.eval(&ob, Op::Insert(order.clone())).collect();
        assert_eq!(
            i,
            vec![
                Instruction::Fill(String::from("b1"), String::from("b1"), 1000, 50, true),
                Instruction::Fill(String::from("s1"), String::from("s1"), 1000, 50, false),
                Instruction::Insert(order, 50),
            ]
        );
    }

    #[test]
    fn test_post_only_reject_if_would_take() {
        let mut ob = OrderBook::<TestOrder>::default();
        setup_order(&mut ob, "s1", false, 1000, 100);
        let order = TestOrder::new("b1", true, 1000, 100).with_post_only(true);
        let mut eval = Evaluator::default();
        let i: Vec<_> = eval.eval(&ob, Op::Insert(order)).collect();
        assert_eq!(
            i,
            vec![Instruction::NoOp(String::from("b1"), Msg::PostOnlyFilled)]
        );
    }

    #[test]
    fn test_post_only_accept_if_maker() {
        let mut ob = OrderBook::<TestOrder>::default();
        setup_order(&mut ob, "s1", false, 1100, 100);
        let order = TestOrder::new("b1", true, 1000, 100).with_post_only(true);
        let mut eval = Evaluator::default();
        let i: Vec<_> = eval.eval(&ob, Op::Insert(order.clone())).collect();
        assert_eq!(i, vec![Instruction::Insert(order, 100)]);
    }

    // ─────────────────────────────────────────────────────────────────────────
    // STP (self-trade prevention) tests
    // ─────────────────────────────────────────────────────────────────────────

    #[test]
    fn test_stp_cancel_taker() {
        let mut ob = OrderBook::<TestOrder>::default();
        setup_order_with_owner(&mut ob, "s1", false, 1000, 100, "alice");
        let order = TestOrder::new("b1", true, 1000, 100)
            .with_owner("alice")
            .with_stp(STP::CancelTaker);
        let mut eval = Evaluator::default();
        let i: Vec<_> = eval.eval(&ob, Op::Insert(order)).collect();
        assert_eq!(
            i,
            vec![Instruction::NoOp(String::from("b1"), Msg::StpCancelTaker)]
        );
    }

    #[test]
    fn test_stp_cancel_taker_no_self_trade() {
        let mut ob = OrderBook::<TestOrder>::default();
        setup_order_with_owner(&mut ob, "s1", false, 1000, 100, "alice");
        let order = TestOrder::new("b1", true, 1000, 100)
            .with_owner("bob")
            .with_stp(STP::CancelTaker);
        let mut eval = Evaluator::default();
        let i: Vec<_> = eval.eval(&ob, Op::Insert(order.clone())).collect();
        assert_eq!(
            i,
            vec![
                Instruction::Fill(String::from("b1"), String::from("bob"), 1000, 100, true),
                Instruction::Fill(String::from("s1"), String::from("alice"), 1000, 100, false),
            ]
        );
    }

    #[test]
    fn test_stp_cancel_maker() {
        let mut ob = OrderBook::<TestOrder>::default();
        setup_order_with_owner(&mut ob, "s1", false, 1000, 100, "alice");
        let order = TestOrder::new("b1", true, 1000, 100)
            .with_owner("alice")
            .with_stp(STP::CancelMaker);
        let mut eval = Evaluator::default();
        let i: Vec<_> = eval.eval(&ob, Op::Insert(order.clone())).collect();
        let insert_b1 = i
            .iter()
            .any(|p| matches!(p, Instruction::Insert(o, r) if o.id() == "b1" && *r == 100));
        let cancels: Vec<_> = i
            .iter()
            .filter_map(|p| match p {
                Instruction::Delete(id, _) => Some(id.clone()),
                _ => None,
            })
            .collect();
        assert!(insert_b1 && cancels == &[String::from("s1")]);

        let mut ob2 = OrderBook::<TestOrder>::default();
        setup_order_with_owner(&mut ob2, "s1", false, 1000, 100, "alice");
        let outputs: Vec<_> = i.into_iter().map(|instr| ob2.apply(instr)).collect();
        assert!(outputs.contains(&Output::Inserted(100)));
        assert!(outputs.contains(&Output::Deleted));
    }

    #[test]
    fn test_stp_cancel_maker_fill_from_others() {
        let mut ob = OrderBook::<TestOrder>::default();
        setup_order_with_owner(&mut ob, "s1", false, 1000, 50, "alice");
        setup_order_with_owner(&mut ob, "s2", false, 1000, 50, "bob");
        let order = TestOrder::new("b1", true, 1000, 100)
            .with_owner("alice")
            .with_stp(STP::CancelMaker);
        let mut eval = Evaluator::default();
        let i: Vec<_> = eval.eval(&ob, Op::Insert(order.clone())).collect();
        let fills: Vec<_> = i
            .iter()
            .filter_map(|p| match p {
                Instruction::Fill(id, _, _, q, false) => Some((id.clone(), *q)),
                _ => None,
            })
            .collect();
        let cancel_ids: Vec<_> = i
            .iter()
            .filter_map(|p| match p {
                Instruction::Delete(id, _) => Some(id.clone()),
                _ => None,
            })
            .collect();
        assert!(
            i.iter()
                .any(|p| matches!(p, Instruction::Insert(o, r) if o.id() == "b1" && *r == 50))
        );
        assert_eq!(fills, &[(String::from("s2"), 50)]);
        assert_eq!(cancel_ids, &[String::from("s1")]);
    }

    #[test]
    fn test_stp_cancel_both() {
        let mut ob = OrderBook::<TestOrder>::default();
        setup_order_with_owner(&mut ob, "s1", false, 1000, 100, "alice");
        let order = TestOrder::new("b1", true, 1000, 100)
            .with_owner("alice")
            .with_stp(STP::CancelBoth);
        let mut eval = Evaluator::default();
        let i: Vec<_> = eval.eval(&ob, Op::Insert(order)).collect();
        assert_eq!(
            i,
            vec![
                Instruction::NoOp(String::from("b1"), Msg::StpCancelBoth),
                Instruction::Delete(String::from("s1"), Msg::StpCancelBoth),
            ]
        );
        let mut ob2 = OrderBook::<TestOrder>::default();
        setup_order_with_owner(&mut ob2, "s1", false, 1000, 100, "alice");
        let outputs: Vec<_> = i.into_iter().map(|instr| ob2.apply(instr)).collect();
        assert_eq!(
            outputs
                .iter()
                .filter(|p| matches!(p, Output::Deleted))
                .count(),
            1
        );
        assert_eq!(
            outputs.iter().filter(|p| matches!(p, Output::NoOp)).count(),
            1
        );
        assert!(ob2.is_empty());
    }
}
