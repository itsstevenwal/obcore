use crate::{hash::FxHashMap, list::Node, order::OrderInterface, side::Side};

/// A complete orderbook with bid and ask sides.
pub struct OrderBook<O: OrderInterface> {
    bids: Side<O>,
    asks: Side<O>,
    orders: FxHashMap<O::T, *mut Node<O>>,
}

impl<O: OrderInterface> Default for OrderBook<O> {
    fn default() -> Self {
        Self {
            bids: Side::new(true),
            asks: Side::new(false),
            orders: FxHashMap::default(),
        }
    }
}

/// Evaluator for computing orderbook operations without mutating state.
pub struct Evaluator<O: OrderInterface> {
    temp: FxHashMap<O::T, O::N>,
}

impl<O: OrderInterface> Default for Evaluator<O> {
    fn default() -> Self {
        Self {
            temp: FxHashMap::default(),
        }
    }
}

impl<O: OrderInterface> Evaluator<O> {
    /// Creates a new Evaluator.
    pub fn new() -> Self {
        Self::default()
    }

    /// Resets the evaluator's temporary state.
    #[inline]
    pub fn reset(&mut self) {
        self.temp.clear();
    }

    /// Evaluates a batch of operations against the orderbook without mutating it.
    /// Returns matches and instructions that can later be applied.
    #[inline]
    pub fn eval(
        &mut self,
        ob: &OrderBook<O>,
        ops: Vec<Op<O>>,
    ) -> (Vec<Match<O>>, Vec<Instruction<O>>) {
        let mut matches = Vec::new();
        let mut instructions = Vec::new();
        for op in ops {
            match op {
                Op::Insert(order) => {
                    let (match_result, mut instrs) = self.eval_insert(ob, order);
                    if let Some(m) = match_result {
                        matches.push(m);
                    }
                    instructions.append(&mut instrs);
                }
                Op::Delete(order_id) => instructions.push(self.eval_cancel(ob, order_id)),
            }
        }
        self.temp.clear();
        (matches, instructions)
    }

    #[inline(always)]
    fn eval_insert(
        &mut self,
        ob: &OrderBook<O>,
        order: O,
    ) -> (Option<Match<O>>, Vec<Instruction<O>>) {
        if ob.orders.contains_key(order.id()) {
            return Self::eval_insert_duplicate();
        }

        let mut remaining_quantity = order.remaining();
        let mut taker_quantity = O::N::default();
        let mut maker_quantities = Vec::new();
        let mut instructions = Vec::with_capacity(16);
        let is_buy = order.is_buy();
        let price = order.price();

        let opposite_side = if is_buy { &ob.asks } else { &ob.bids };

        'outer: for level in opposite_side.iter() {
            let dominated = if is_buy {
                price < level.price()
            } else {
                price > level.price()
            };
            if dominated {
                break;
            }
            for resting_order in level.iter() {
                if remaining_quantity == O::N::default() {
                    break 'outer;
                }
                let remaining = self
                    .temp
                    .get(resting_order.id())
                    .copied()
                    .unwrap_or_else(|| resting_order.remaining());
                if remaining == O::N::default() {
                    continue;
                }
                let taken_quantity = remaining_quantity.min(remaining);
                remaining_quantity -= taken_quantity;
                taker_quantity += taken_quantity;
                instructions.push(Instruction::Fill(
                    resting_order.id().clone(),
                    taken_quantity,
                ));
                maker_quantities.push((resting_order.id().clone(), taken_quantity));
                self.temp
                    .insert(resting_order.id().clone(), remaining - taken_quantity);
            }
        }

        let match_result = if taker_quantity > O::N::default() {
            Some(Match {
                taker: (order.id().clone(), taker_quantity),
                makers: maker_quantities,
            })
        } else {
            None
        };

        if remaining_quantity > O::N::default() {
            instructions.push(Instruction::Insert(order, remaining_quantity));
            instructions.rotate_right(1);
        }

        (match_result, instructions)
    }

    #[cold]
    #[inline(never)]
    fn eval_insert_duplicate() -> (Option<Match<O>>, Vec<Instruction<O>>) {
        (None, vec![Instruction::NoOp(Msg::OrderAlreadyExists)])
    }

    #[inline(always)]
    fn eval_cancel(&mut self, ob: &OrderBook<O>, order_id: O::T) -> Instruction<O> {
        if !ob.orders.contains_key(&order_id) {
            return Self::eval_cancel_not_found();
        }
        self.temp.insert(order_id.clone(), O::N::default());
        Instruction::Delete(order_id)
    }

    #[cold]
    #[inline(never)]
    fn eval_cancel_not_found() -> Instruction<O> {
        Instruction::NoOp(Msg::OrderNotFound)
    }
}

/// An operation to apply to the orderbook.
pub enum Op<O: OrderInterface> {
    Insert(O),
    Delete(O::T),
}

#[derive(Debug, PartialEq, Eq)]
pub enum Msg {
    OrderNotFound,
    OrderAlreadyExists,
}

#[derive(Debug, PartialEq, Eq)]
pub enum Instruction<O: OrderInterface> {
    Insert(O, O::N),
    Delete(O::T),
    Fill(O::T, O::N),
    NoOp(Msg),
}

/// A match between a taker and one or more makers.
pub struct Match<O: OrderInterface> {
    pub taker: (O::T, O::N),
    pub makers: Vec<(O::T, O::N)>,
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
    pub fn order(&self, order_id: &O::T) -> Option<&O> {
        self.orders
            .get(order_id)
            .map(|&ptr| unsafe { &(*ptr).data })
    }

    // ─────────────────────────────────────────────────────────────────────────
    // Internal helpers
    // ─────────────────────────────────────────────────────────────────────────

    #[inline(always)]
    fn side_mut(&mut self, is_buy: bool) -> &mut Side<O> {
        if is_buy {
            &mut self.bids
        } else {
            &mut self.asks
        }
    }

    /// Applies instructions to the orderbook, mutating state.
    #[inline]
    pub fn apply(&mut self, instructions: Vec<Instruction<O>>) {
        for instruction in instructions {
            match instruction {
                Instruction::Insert(order, remaining) => self.apply_insert(order, remaining),
                Instruction::Delete(order_id) => self.apply_delete(&order_id),
                Instruction::Fill(order_id, quantity) => self.apply_fill(&order_id, quantity),
                Instruction::NoOp(_) => {}
            }
        }
    }

    #[inline(always)]
    fn apply_insert(&mut self, mut order: O, remaining: O::N) {
        let filled = order.quantity() - remaining;
        if filled > O::N::default() {
            order.fill(filled);
        }
        let id = order.id().clone();
        let is_buy = order.is_buy();
        let node_ptr = self.side_mut(is_buy).insert_order(order);
        self.orders.insert(id, node_ptr);
    }

    #[inline(always)]
    fn apply_delete(&mut self, order_id: &O::T) {
        let Some(&node_ptr) = self.orders.get(order_id) else {
            return;
        };
        let is_buy = unsafe { (*node_ptr).data.is_buy() };
        self.side_mut(is_buy).remove_order(node_ptr);
        self.orders.remove(order_id);
    }

    #[inline(always)]
    fn apply_fill(&mut self, order_id: &O::T, quantity: O::N) {
        let Some(&node_ptr) = self.orders.get(order_id) else {
            return;
        };
        let is_buy = unsafe { (*node_ptr).data.is_buy() };
        let removed = self.side_mut(is_buy).fill_order(node_ptr, quantity);
        if removed {
            self.orders.remove(order_id);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::order::TestOrder;

    fn setup_order(ob: &mut OrderBook<TestOrder>, id: &str, is_buy: bool, price: u64, qty: u64) {
        let order = TestOrder::new(id, is_buy, price, qty);
        let node_ptr = ob.side_mut(is_buy).insert_order(order);
        ob.orders.insert(String::from(id), node_ptr);
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
        let mut eval = Evaluator::new();
        let order = TestOrder::new("1", true, 1000, 100);
        let (m, i) = eval.eval(&ob, vec![Op::Insert(order.clone())]);
        assert!(m.is_empty());
        assert_eq!(i[0], Instruction::Insert(order, 100));

        let ob = OrderBook::<TestOrder>::default();
        let mut eval = Evaluator::new();
        let order = TestOrder::new("1", false, 1000, 50);
        let (m, i) = eval.eval(&ob, vec![Op::Insert(order.clone())]);
        assert!(m.is_empty());
        assert_eq!(i[0], Instruction::Insert(order, 50));
    }

    #[test]
    fn test_eval_insert_duplicate() {
        let mut ob = OrderBook::<TestOrder>::default();
        setup_order(&mut ob, "1", true, 1000, 100);
        let mut eval = Evaluator::new();
        let (m, i) = eval.eval(&ob, vec![Op::Insert(TestOrder::new("1", true, 1000, 50))]);
        assert!(m.is_empty());
        assert_eq!(i[0], Instruction::NoOp(Msg::OrderAlreadyExists));
    }

    #[test]
    fn test_eval_cancel() {
        let mut ob = OrderBook::<TestOrder>::default();
        let mut eval = Evaluator::new();
        let (_, i) = eval.eval(&ob, vec![Op::Delete(String::from("x"))]);
        assert_eq!(i[0], Instruction::NoOp(Msg::OrderNotFound));

        setup_order(&mut ob, "1", true, 1000, 100);
        let mut eval = Evaluator::new();
        let (_, i) = eval.eval(&ob, vec![Op::Delete(String::from("1"))]);
        assert_eq!(i[0], Instruction::Delete(String::from("1")));
    }

    #[test]
    fn test_eval_matching() {
        let mut ob = OrderBook::<TestOrder>::default();
        setup_order(&mut ob, "s1", false, 1000, 100);
        let mut eval = Evaluator::new();
        let (m, i) = eval.eval(&ob, vec![Op::Insert(TestOrder::new("b1", true, 1000, 100))]);
        assert_eq!(m[0].taker.1, 100);
        assert_eq!(i.len(), 1);
        assert_eq!(i[0], Instruction::Fill(String::from("s1"), 100));

        let mut ob = OrderBook::<TestOrder>::default();
        setup_order(&mut ob, "s1", false, 1000, 50);
        let order = TestOrder::new("b1", true, 1000, 100);
        let mut eval = Evaluator::new();
        let (m, i) = eval.eval(&ob, vec![Op::Insert(order.clone())]);
        assert_eq!(m[0].taker.1, 50);
        assert_eq!(i.len(), 2);
        assert_eq!(i[0], Instruction::Insert(order, 50));
    }

    #[test]
    fn test_eval_price_crossing() {
        // Buy doesn't match higher sell
        let mut ob = OrderBook::<TestOrder>::default();
        setup_order(&mut ob, "s1", false, 1100, 100);
        let mut eval = Evaluator::new();
        let (m, _) = eval.eval(&ob, vec![Op::Insert(TestOrder::new("b1", true, 1000, 100))]);
        assert!(m.is_empty());

        // Buy at higher price matches lower sell
        let mut ob = OrderBook::<TestOrder>::default();
        setup_order(&mut ob, "s1", false, 1000, 100);
        let mut eval = Evaluator::new();
        let (m, _) = eval.eval(&ob, vec![Op::Insert(TestOrder::new("b1", true, 1100, 100))]);
        assert_eq!(m[0].taker.1, 100);

        // Sell doesn't match lower buy
        let mut ob = OrderBook::<TestOrder>::default();
        setup_order(&mut ob, "b1", true, 1000, 100);
        let mut eval = Evaluator::new();
        let (m, _) = eval.eval(
            &ob,
            vec![Op::Insert(TestOrder::new("s1", false, 1100, 100))],
        );
        assert!(m.is_empty());

        // Sell at lower price matches higher buy
        let mut ob = OrderBook::<TestOrder>::default();
        setup_order(&mut ob, "b1", true, 1100, 100);
        let mut eval = Evaluator::new();
        let (m, _) = eval.eval(
            &ob,
            vec![Op::Insert(TestOrder::new("s1", false, 1000, 100))],
        );
        assert_eq!(m[0].taker.1, 100);
    }

    #[test]
    fn test_eval_multi_maker_match() {
        let mut ob = OrderBook::<TestOrder>::default();
        setup_order(&mut ob, "b1", true, 1100, 30);
        setup_order(&mut ob, "b2", true, 1050, 40);
        let mut eval = Evaluator::new();
        let (m, i) = eval.eval(
            &ob,
            vec![Op::Insert(TestOrder::new("s1", false, 1000, 100))],
        );
        assert_eq!(m[0].makers.len(), 2);
        assert_eq!(i.len(), 3);
    }

    #[test]
    fn test_eval_quantity_exhausted() {
        let mut ob = OrderBook::<TestOrder>::default();
        setup_order(&mut ob, "s1", false, 1000, 50);
        setup_order(&mut ob, "s2", false, 1000, 50);
        let mut eval = Evaluator::new();
        let (m, i) = eval.eval(&ob, vec![Op::Insert(TestOrder::new("b1", true, 1000, 50))]);
        assert_eq!(m[0].makers.len(), 1);
        assert_eq!(i.len(), 1);

        let mut ob = OrderBook::<TestOrder>::default();
        setup_order(&mut ob, "b1", true, 1000, 50);
        setup_order(&mut ob, "b2", true, 1000, 50);
        let mut eval = Evaluator::new();
        let (m, i) = eval.eval(&ob, vec![Op::Insert(TestOrder::new("s1", false, 1000, 50))]);
        assert_eq!(m[0].makers.len(), 1);
        assert_eq!(i.len(), 1);
    }

    #[test]
    fn test_eval_with_ops() {
        let ob = OrderBook::<TestOrder>::default();
        let ops = vec![
            Op::Insert(TestOrder::new("b1", true, 1000, 100)),
            Op::Insert(TestOrder::new("s1", false, 1100, 50)),
            Op::Delete(String::from("b1")),
        ];
        let mut eval = Evaluator::new();
        let (matches, instructions) = eval.eval(&ob, ops);
        assert!(matches.is_empty());
        assert_eq!(instructions.len(), 3);
    }

    #[test]
    fn test_temp_state() {
        let mut ob = OrderBook::<TestOrder>::default();
        setup_order(&mut ob, "s1", false, 1000, 100);

        // Within a single eval call, temp state is tracked
        let mut eval = Evaluator::new();
        let ops = vec![
            Op::Insert(TestOrder::new("b1", true, 1000, 30)),
            Op::Insert(TestOrder::new("b2", true, 1000, 20)),
            Op::Delete(String::from("s1")),
            Op::Insert(TestOrder::new("b3", true, 1000, 50)),
        ];
        let (m, _) = eval.eval(&ob, ops);
        // b1 and b2 match against s1, then s1 is cancelled, so b3 doesn't match
        assert_eq!(m.len(), 2); // b1 and b2 matched
        assert_eq!(m[0].taker.1, 30);
        assert_eq!(m[1].taker.1, 20);
    }

    // ─────────────────────────────────────────────────────────────────────────
    // Apply Tests
    // ─────────────────────────────────────────────────────────────────────────

    #[test]
    fn test_apply_insert() {
        let mut ob = OrderBook::<TestOrder>::default();
        ob.apply(vec![Instruction::Insert(
            TestOrder::new("1", true, 1000, 100),
            100,
        )]);
        assert!(ob.orders.contains_key("1"));

        let mut ob = OrderBook::<TestOrder>::default();
        ob.apply(vec![Instruction::Insert(
            TestOrder::new("1", false, 1000, 100),
            100,
        )]);
        assert!(!ob.asks.is_empty());

        let mut ob = OrderBook::<TestOrder>::default();
        ob.apply(vec![Instruction::Insert(
            TestOrder::new("1", true, 1000, 100),
            70,
        )]);
        assert_eq!(ob.order(&String::from("1")).unwrap().remaining(), 70);
    }

    #[test]
    fn test_apply_delete() {
        let mut ob = OrderBook::<TestOrder>::default();
        setup_order(&mut ob, "1", true, 1000, 100);
        ob.apply(vec![Instruction::Delete(String::from("1"))]);
        assert!(ob.bids.is_empty());

        let mut ob = OrderBook::<TestOrder>::default();
        setup_order(&mut ob, "1", false, 1000, 100);
        ob.apply(vec![Instruction::Delete(String::from("1"))]);
        assert!(ob.asks.is_empty());

        // Non-existent (no panic)
        let mut ob = OrderBook::<TestOrder>::default();
        ob.apply(vec![Instruction::Delete(String::from("x"))]);
    }

    #[test]
    fn test_apply_fill() {
        // Partial fill sell
        let mut ob = OrderBook::<TestOrder>::default();
        setup_order(&mut ob, "1", false, 1000, 100);
        ob.apply(vec![Instruction::Fill(String::from("1"), 30)]);
        assert_eq!(ob.order(&String::from("1")).unwrap().remaining(), 70);

        // Complete fill sell
        let mut ob = OrderBook::<TestOrder>::default();
        setup_order(&mut ob, "1", false, 1000, 100);
        ob.apply(vec![Instruction::Fill(String::from("1"), 100)]);
        assert!(ob.asks.is_empty());

        // Partial fill buy
        let mut ob = OrderBook::<TestOrder>::default();
        setup_order(&mut ob, "1", true, 1000, 100);
        ob.apply(vec![Instruction::Fill(String::from("1"), 30)]);
        assert_eq!(ob.order(&String::from("1")).unwrap().remaining(), 70);

        // Complete fill buy
        let mut ob = OrderBook::<TestOrder>::default();
        setup_order(&mut ob, "1", true, 1000, 100);
        ob.apply(vec![Instruction::Fill(String::from("1"), 100)]);
        assert!(ob.bids.is_empty());

        // Non-existent (no panic)
        let mut ob = OrderBook::<TestOrder>::default();
        ob.apply(vec![Instruction::Fill(String::from("x"), 50)]);
    }

    #[test]
    fn test_apply_noop() {
        let mut ob = OrderBook::<TestOrder>::default();
        ob.apply(vec![
            Instruction::NoOp(Msg::OrderNotFound),
            Instruction::NoOp(Msg::OrderAlreadyExists),
        ]);
        assert!(ob.bids.is_empty());
    }

    // ─────────────────────────────────────────────────────────────────────────
    // Integration Tests
    // ─────────────────────────────────────────────────────────────────────────

    #[test]
    fn test_eval_then_apply() {
        let mut ob = OrderBook::<TestOrder>::default();
        setup_order(&mut ob, "s1", false, 1000, 100);
        let ops = vec![Op::Insert(TestOrder::new("b1", true, 1000, 60))];
        let mut eval = Evaluator::new();
        let (matches, instructions) = eval.eval(&ob, ops);
        assert_eq!(matches[0].taker.1, 60);
        ob.apply(instructions);
        assert_eq!(ob.order(&String::from("s1")).unwrap().remaining(), 40);
        assert!(!ob.orders.contains_key("b1"));
    }

    #[test]
    fn test_eval_then_apply_with_insert() {
        let mut ob = OrderBook::<TestOrder>::default();
        setup_order(&mut ob, "s1", false, 1000, 50);
        let ops = vec![Op::Insert(TestOrder::new("b1", true, 1000, 100))];
        let mut eval = Evaluator::new();
        let (_, instructions) = eval.eval(&ob, ops);
        ob.apply(instructions);
        assert!(ob.asks.is_empty());
        assert_eq!(ob.order(&String::from("b1")).unwrap().remaining(), 50);
    }
}
