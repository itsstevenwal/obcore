#[cfg(feature = "bench")]
use crate::eval::Msg;
use crate::eval::{Instruction, InstructionPrimitive};
use crate::{hash::FxHashMap, list::Node, order::OrderInterface, side::Side};

/// A complete orderbook with bid and ask sides.
pub struct OrderBook<O: OrderInterface> {
    pub(crate) bids: Side<O>,
    pub(crate) asks: Side<O>,
    pub(crate) orders: FxHashMap<O::T, *mut Node<O>>,
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

#[derive(Debug, PartialEq, Eq, Clone)]
pub enum OutputPrimitive<O: OrderInterface> {
    // Resting quantity
    Inserted(O::N),
    // Deleted
    Deleted,
    // Partial fill
    Partial,
    // Filled
    Filled,
}

/// Output of a single instruction.
#[allow(type_alias_bounds)]
pub enum Output<O: OrderInterface> {
    Single(OutputPrimitive<O>),
    Multi(Vec<OutputPrimitive<O>>),
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

    /// Applies a single insert instruction. Only available with the `bench` feature.
    #[cfg(feature = "bench")]
    #[inline(always)]
    pub fn apply_insert(&mut self, order: O, remaining: O::N) -> Output<O> {
        Output::Single(self.apply_single(InstructionPrimitive::Insert(order, remaining)))
    }

    /// Applies a single delete instruction. Only available with the `bench` feature.
    #[cfg(feature = "bench")]
    #[inline(always)]
    pub fn apply_delete(&mut self, order_id: O::T) -> Output<O> {
        Output::Single(
            self.apply_single(InstructionPrimitive::Delete(order_id, Msg::UserCancelled)),
        )
    }

    /// Applies instructions to the orderbook, mutating state.
    /// Returns one `Output` per instruction.
    #[inline]
    pub fn apply(&mut self, instructions: Vec<Instruction<O>>) -> Vec<Output<O>> {
        let mut outputs = Vec::with_capacity(instructions.len());
        for instruction in instructions {
            match instruction {
                Instruction::Single(p) => {
                    outputs.push(Output::Single(self.apply_single(p)));
                }
                Instruction::Multi(primitives) => {
                    outputs.push(Output::Multi(self.apply_multi(primitives)));
                }
            }
        }
        outputs
    }

    #[inline(always)]
    fn apply_single(&mut self, p: InstructionPrimitive<O>) -> OutputPrimitive<O> {
        let Self { bids, asks, orders } = self;
        Self::apply_primitive(bids, asks, orders, p)
    }

    #[inline(always)]
    fn apply_multi(&mut self, primitives: Vec<InstructionPrimitive<O>>) -> Vec<OutputPrimitive<O>> {
        let Self { bids, asks, orders } = self;
        let mut out = Vec::with_capacity(primitives.len());
        for p in primitives {
            out.push(Self::apply_primitive(bids, asks, orders, p));
        }
        out
    }

    #[inline(always)]
    fn apply_primitive(
        bids: &mut Side<O>,
        asks: &mut Side<O>,
        orders: &mut FxHashMap<O::T, *mut Node<O>>,
        p: InstructionPrimitive<O>,
    ) -> OutputPrimitive<O> {
        match p {
            InstructionPrimitive::Fill(order_id, quantity) => {
                let &node_ptr = orders.get(&order_id).unwrap();
                let is_buy = unsafe { (*node_ptr).data.is_buy() };
                let side = if is_buy { bids } else { asks };
                let removed = side.fill_order(node_ptr, quantity);
                if removed {
                    orders.remove(&order_id);
                    OutputPrimitive::Filled
                } else {
                    OutputPrimitive::Partial
                }
            }
            InstructionPrimitive::Insert(mut order, remaining) => {
                if remaining > O::N::default() {
                    let filled = order.quantity() - remaining;
                    if filled > O::N::default() {
                        order.fill(filled);
                    }
                    let id = order.id().clone();
                    let is_buy = order.is_buy();
                    let side = if is_buy { bids } else { asks };
                    let node_ptr = side.insert_order(order);
                    orders.insert(id, node_ptr);
                }
                OutputPrimitive::Inserted(remaining)
            }
            InstructionPrimitive::Delete(order_id, _msg) => {
                if let Some(&node_ptr) = orders.get(&order_id) {
                    let is_buy = unsafe { (*node_ptr).data.is_buy() };
                    let side = if is_buy { bids } else { asks };
                    side.remove_order(node_ptr);
                    orders.remove(&order_id);
                }
                OutputPrimitive::Deleted
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::eval::{Evaluator, Instruction, InstructionPrimitive, Msg, Op};
    use crate::order::{STP, TIF, TestOrder};

    fn setup_order(ob: &mut OrderBook<TestOrder>, id: &str, is_buy: bool, price: u64, qty: u64) {
        let order = TestOrder::new(id, is_buy, price, qty);
        let side = if is_buy { &mut ob.bids } else { &mut ob.asks };
        let node_ptr = side.insert_order(order);
        ob.orders.insert(String::from(id), node_ptr);
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
        let side = if is_buy { &mut ob.bids } else { &mut ob.asks };
        let node_ptr = side.insert_order(order);
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
        let mut eval = Evaluator::default();
        let order = TestOrder::new("1", true, 1000, 100);
        let i = eval.eval(&ob, vec![Op::Insert(order.clone())]);
        assert_eq!(
            i[0],
            Instruction::Single(InstructionPrimitive::Insert(order, 100))
        );

        let ob = OrderBook::<TestOrder>::default();
        let mut eval = Evaluator::default();
        let order = TestOrder::new("1", false, 1000, 50);
        let i = eval.eval(&ob, vec![Op::Insert(order.clone())]);
        assert_eq!(
            i[0],
            Instruction::Single(InstructionPrimitive::Insert(order, 50))
        );
    }

    #[test]
    fn test_eval_insert_duplicate() {
        let mut ob = OrderBook::<TestOrder>::default();
        setup_order(&mut ob, "1", true, 1000, 100);
        let mut eval = Evaluator::default();
        let i = eval.eval(&ob, vec![Op::Insert(TestOrder::new("1", true, 1000, 50))]);
        assert_eq!(
            i[0],
            Instruction::Single(InstructionPrimitive::Delete(
                String::from("1"),
                Msg::OrderAlreadyExists,
            ))
        );
    }

    #[test]
    fn test_eval_cancel() {
        let mut ob = OrderBook::<TestOrder>::default();
        let mut eval = Evaluator::default();
        let i = eval.eval(&ob, vec![Op::Delete(String::from("x"))]);
        assert_eq!(
            i[0],
            Instruction::Single(InstructionPrimitive::Delete(
                String::from("x"),
                Msg::OrderNotFound,
            ))
        );

        setup_order(&mut ob, "1", true, 1000, 100);
        let mut eval = Evaluator::default();
        let i = eval.eval(&ob, vec![Op::Delete(String::from("1"))]);
        assert_eq!(
            i[0],
            Instruction::Single(InstructionPrimitive::Delete(
                String::from("1"),
                Msg::UserCancelled,
            ))
        );
    }

    #[test]
    fn test_eval_matching() {
        let mut ob = OrderBook::<TestOrder>::default();
        setup_order(&mut ob, "s1", false, 1000, 100);
        let order = TestOrder::new("b1", true, 1000, 100);
        let mut eval = Evaluator::default();
        let i = eval.eval(&ob, vec![Op::Insert(order.clone())]);
        // One Match (taker b1, maker s1)
        assert_eq!(i.len(), 1);
        assert_eq!(
            i[0],
            Instruction::Multi(vec![InstructionPrimitive::Fill(String::from("s1"), 100,)])
        );

        let mut ob = OrderBook::<TestOrder>::default();
        setup_order(&mut ob, "s1", false, 1000, 50);
        let order = TestOrder::new("b1", true, 1000, 100);
        let mut eval = Evaluator::default();
        let i = eval.eval(&ob, vec![Op::Insert(order.clone())]);
        // One Match with remaining (apply will insert)
        assert_eq!(i.len(), 1);
        assert_eq!(
            i[0],
            Instruction::Multi(vec![
                InstructionPrimitive::Fill(String::from("s1"), 50),
                InstructionPrimitive::Insert(order, 50),
            ])
        );
    }

    #[test]
    fn test_eval_price_crossing() {
        // Buy doesn't match higher sell
        let mut ob = OrderBook::<TestOrder>::default();
        setup_order(&mut ob, "s1", false, 1100, 100);
        let mut eval = Evaluator::default();
        let i = eval.eval(&ob, vec![Op::Insert(TestOrder::new("b1", true, 1000, 100))]);
        // Only an insert, no fills
        assert_eq!(i.len(), 1);
        assert!(matches!(
            i[0],
            Instruction::Single(InstructionPrimitive::Insert(_, _))
        ));

        // Buy at higher price matches lower sell
        let mut ob = OrderBook::<TestOrder>::default();
        setup_order(&mut ob, "s1", false, 1000, 100);
        let mut eval = Evaluator::default();
        let order = TestOrder::new("b1", true, 1100, 100);
        let i = eval.eval(&ob, vec![Op::Insert(order.clone())]);
        assert_eq!(
            i[0],
            Instruction::Multi(vec![InstructionPrimitive::Fill(String::from("s1"), 100,)])
        );

        // Sell doesn't match lower buy
        let mut ob = OrderBook::<TestOrder>::default();
        setup_order(&mut ob, "b1", true, 1000, 100);
        let mut eval = Evaluator::default();
        let i = eval.eval(
            &ob,
            vec![Op::Insert(TestOrder::new("s1", false, 1100, 100))],
        );
        // Only an insert, no fills
        assert_eq!(i.len(), 1);
        assert!(matches!(
            i[0],
            Instruction::Single(InstructionPrimitive::Insert(_, _))
        ));

        // Sell at lower price matches higher buy
        let mut ob = OrderBook::<TestOrder>::default();
        setup_order(&mut ob, "b1", true, 1100, 100);
        let order = TestOrder::new("s1", false, 1000, 100);
        let mut eval = Evaluator::default();
        let i = eval.eval(&ob, vec![Op::Insert(order.clone())]);
        assert_eq!(
            i[0],
            Instruction::Multi(vec![InstructionPrimitive::Fill(String::from("b1"), 100,)])
        );
    }

    #[test]
    fn test_eval_multi_maker_match() {
        let mut ob = OrderBook::<TestOrder>::default();
        setup_order(&mut ob, "b1", true, 1100, 30);
        setup_order(&mut ob, "b2", true, 1050, 40);
        let mut eval = Evaluator::default();
        let i = eval.eval(
            &ob,
            vec![Op::Insert(TestOrder::new("s1", false, 1000, 100))],
        );
        // 1 Match (s1 vs b1 30, b2 40) with remaining 30 (apply will insert)
        assert_eq!(i.len(), 1);
        assert!(matches!(
            &i[0],
            Instruction::Multi(v) if v.iter().any(|p| matches!(p, InstructionPrimitive::Insert(_, r) if *r == 30))
        ));
    }

    #[test]
    fn test_eval_quantity_exhausted() {
        let mut ob = OrderBook::<TestOrder>::default();
        setup_order(&mut ob, "s1", false, 1000, 50);
        setup_order(&mut ob, "s2", false, 1000, 50);
        let mut eval = Evaluator::default();
        let i = eval.eval(&ob, vec![Op::Insert(TestOrder::new("b1", true, 1000, 50))]);
        // 1 Match (b1 vs s1 50)
        assert_eq!(i.len(), 1);

        let mut ob = OrderBook::<TestOrder>::default();
        setup_order(&mut ob, "b1", true, 1000, 50);
        setup_order(&mut ob, "b2", true, 1000, 50);
        let mut eval = Evaluator::default();
        let i = eval.eval(&ob, vec![Op::Insert(TestOrder::new("s1", false, 1000, 50))]);
        // 1 Match (s1 vs b1 50)
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
        let mut eval = Evaluator::default();
        let instructions = eval.eval(&ob, ops);
        // 2 inserts + 1 delete
        assert_eq!(instructions.len(), 3);
    }

    #[test]
    fn test_temp_state() {
        let mut ob = OrderBook::<TestOrder>::default();
        setup_order(&mut ob, "s1", false, 1000, 100);

        // Within a single eval call, temp state is tracked
        let mut eval = Evaluator::default();
        let ops = vec![
            Op::Insert(TestOrder::new("b1", true, 1000, 30)),
            Op::Insert(TestOrder::new("b2", true, 1000, 20)),
            Op::Delete(String::from("s1")),
            Op::Insert(TestOrder::new("b3", true, 1000, 50)),
        ];
        let i = eval.eval(&ob, ops);
        // b1: Match(b1, [(s1, 30)])
        // b2: Match(b2, [(s1, 20)])
        // s1: delete
        // b3: insert (no match since s1 was deleted in temp state)
        let matches: Vec<_> = i
            .iter()
            .filter(|instr| {
                matches!(instr, Instruction::Multi(v) if v.iter().any(|p| matches!(p, InstructionPrimitive::Fill(_, _))))
            })
            .collect();
        assert_eq!(matches.len(), 2); // b1 and b2 matched
    }

    // ─────────────────────────────────────────────────────────────────────────
    // Apply Tests
    // ─────────────────────────────────────────────────────────────────────────

    #[test]
    fn test_apply_insert() {
        let mut ob = OrderBook::<TestOrder>::default();
        ob.apply(vec![Instruction::Single(InstructionPrimitive::Insert(
            TestOrder::new("1", true, 1000, 100),
            100,
        ))]);
        assert!(ob.orders.contains_key("1"));

        let mut ob = OrderBook::<TestOrder>::default();
        ob.apply(vec![Instruction::Single(InstructionPrimitive::Insert(
            TestOrder::new("1", false, 1000, 100),
            100,
        ))]);
        assert!(!ob.asks.is_empty());

        let mut ob = OrderBook::<TestOrder>::default();
        ob.apply(vec![Instruction::Single(InstructionPrimitive::Insert(
            TestOrder::new("1", true, 1000, 100),
            70,
        ))]);
        assert_eq!(ob.order(&String::from("1")).unwrap().remaining(), 70);
    }

    #[test]
    fn test_apply_delete() {
        let mut ob = OrderBook::<TestOrder>::default();
        setup_order(&mut ob, "1", true, 1000, 100);
        ob.apply(vec![Instruction::Single(InstructionPrimitive::Delete(
            String::from("1"),
            Msg::UserCancelled,
        ))]);
        assert!(ob.bids.is_empty());

        let mut ob = OrderBook::<TestOrder>::default();
        setup_order(&mut ob, "1", false, 1000, 100);
        ob.apply(vec![Instruction::Single(InstructionPrimitive::Delete(
            String::from("1"),
            Msg::UserCancelled,
        ))]);
        assert!(ob.asks.is_empty());

        // Non-existent (no panic)
        let mut ob = OrderBook::<TestOrder>::default();
        ob.apply(vec![Instruction::Single(InstructionPrimitive::Delete(
            String::from("x"),
            Msg::OrderNotFound,
        ))]);
    }

    #[test]
    fn test_apply_match() {
        // Partial fill sell (taker "t1", maker "1" filled 30)
        let mut ob = OrderBook::<TestOrder>::default();
        setup_order(&mut ob, "1", false, 1000, 100);
        let t1 = TestOrder::new("t1", true, 1000, 100);
        ob.apply(vec![Instruction::Multi(vec![
            InstructionPrimitive::Fill(String::from("1"), 30),
            InstructionPrimitive::Insert(t1, 70),
        ])]);
        assert_eq!(ob.order(&String::from("1")).unwrap().remaining(), 70);

        // Complete fill sell
        let mut ob = OrderBook::<TestOrder>::default();
        setup_order(&mut ob, "1", false, 1000, 100);
        let t1 = TestOrder::new("t1", true, 1000, 100);
        ob.apply(vec![Instruction::Multi(vec![
            InstructionPrimitive::Fill(String::from("1"), 100),
            InstructionPrimitive::Insert(t1, 0),
        ])]);
        assert!(ob.asks.is_empty());

        // Partial fill buy
        let mut ob = OrderBook::<TestOrder>::default();
        setup_order(&mut ob, "1", true, 1000, 100);
        let t1 = TestOrder::new("t1", false, 1000, 100);
        ob.apply(vec![Instruction::Multi(vec![
            InstructionPrimitive::Fill(String::from("1"), 30),
            InstructionPrimitive::Insert(t1, 70),
        ])]);
        assert_eq!(ob.order(&String::from("1")).unwrap().remaining(), 70);

        // Complete fill buy
        let mut ob = OrderBook::<TestOrder>::default();
        setup_order(&mut ob, "1", true, 1000, 100);
        let t1 = TestOrder::new("t1", false, 1000, 100);
        ob.apply(vec![Instruction::Multi(vec![
            InstructionPrimitive::Fill(String::from("1"), 100),
            InstructionPrimitive::Insert(t1, 0),
        ])]);
        assert!(ob.bids.is_empty());

        // Empty maker list (no panic)
        let mut ob = OrderBook::<TestOrder>::default();
        let x = TestOrder::new("x", true, 1000, 100);
        ob.apply(vec![Instruction::Multi(vec![
            InstructionPrimitive::Insert(x, 0),
        ])]);
    }

    #[test]
    fn test_apply_noop() {
        let mut ob = OrderBook::<TestOrder>::default();
        ob.apply(vec![
            Instruction::Single(InstructionPrimitive::Delete(
                String::from("x"),
                Msg::OrderNotFound,
            )),
            Instruction::Single(InstructionPrimitive::Delete(
                String::from("y"),
                Msg::OrderAlreadyExists,
            )),
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
        let mut eval = Evaluator::default();
        let instructions = eval.eval(&ob, ops);
        // Check there's a Match for b1 with total matched 60
        let taker_match = instructions.iter().find(|i| {
            matches!(i, Instruction::Multi(v) if {
                let filled: u64 = v.iter().filter_map(|p| match p { InstructionPrimitive::Fill(_, q) => Some(*q), _ => None }).sum();
                filled == 60
            })
        });
        assert!(taker_match.is_some());
        ob.apply(instructions);
        assert_eq!(ob.order(&String::from("s1")).unwrap().remaining(), 40);
        assert!(!ob.orders.contains_key("b1"));
    }

    #[test]
    fn test_eval_then_apply_with_insert() {
        let mut ob = OrderBook::<TestOrder>::default();
        setup_order(&mut ob, "s1", false, 1000, 50);
        let ops = vec![Op::Insert(TestOrder::new("b1", true, 1000, 100))];
        let mut eval = Evaluator::default();
        let instructions = eval.eval(&ob, ops);
        ob.apply(instructions);
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
        let i = eval.eval(&ob, vec![Op::Insert(order.clone())]);
        // FOK fully filled: one Match
        assert_eq!(i.len(), 1);
        assert_eq!(
            i[0],
            Instruction::Multi(vec![InstructionPrimitive::Fill(String::from("s1"), 100,)])
        );
    }

    #[test]
    fn test_fok_partial_reject() {
        let mut ob = OrderBook::<TestOrder>::default();
        setup_order(&mut ob, "s1", false, 1000, 50);
        let order = TestOrder::new("b1", true, 1000, 100).with_tif(TIF::FOK);
        let mut eval = Evaluator::default();
        let i = eval.eval(&ob, vec![Op::Insert(order)]);
        assert_eq!(i.len(), 1);
        assert_eq!(
            i[0],
            Instruction::Single(InstructionPrimitive::Delete(
                String::from("b1"),
                Msg::FOKNotFilled,
            ))
        );
    }

    #[test]
    fn test_ioc_partial_fill() {
        let mut ob = OrderBook::<TestOrder>::default();
        setup_order(&mut ob, "s1", false, 1000, 50);
        let order = TestOrder::new("b1", true, 1000, 100).with_tif(TIF::IOC);
        let mut eval = Evaluator::default();
        let i = eval.eval(&ob, vec![Op::Insert(order.clone())]);
        // IOC: Match 50, remaining set to 0 so apply does not insert
        assert_eq!(i.len(), 1);
        assert_eq!(
            i[0],
            Instruction::Multi(vec![InstructionPrimitive::Fill(String::from("s1"), 50,)])
        );
    }

    #[test]
    fn test_ioc_no_match() {
        let ob = OrderBook::<TestOrder>::default();
        let order = TestOrder::new("b1", true, 1000, 100).with_tif(TIF::IOC);
        let mut eval = Evaluator::default();
        let i = eval.eval(&ob, vec![Op::Insert(order)]);
        // IOC with no liquidity: one Delete (reject)
        assert_eq!(i.len(), 1);
        assert_eq!(
            i[0],
            Instruction::Single(InstructionPrimitive::Delete(
                String::from("b1"),
                Msg::IOCNoFill,
            ))
        );
    }

    #[test]
    fn test_gtc_unchanged() {
        let mut ob = OrderBook::<TestOrder>::default();
        setup_order(&mut ob, "s1", false, 1000, 50);
        let order = TestOrder::new("b1", true, 1000, 100); // GTC default
        let mut eval = Evaluator::default();
        let i = eval.eval(&ob, vec![Op::Insert(order.clone())]);
        assert_eq!(i.len(), 1);
        assert_eq!(
            i[0],
            Instruction::Multi(vec![
                InstructionPrimitive::Fill(String::from("s1"), 50),
                InstructionPrimitive::Insert(order, 50),
            ])
        );
    }

    #[test]
    fn test_post_only_reject_if_would_take() {
        let mut ob = OrderBook::<TestOrder>::default();
        setup_order(&mut ob, "s1", false, 1000, 100);
        let order = TestOrder::new("b1", true, 1000, 100).with_post_only(true);
        let mut eval = Evaluator::default();
        let i = eval.eval(&ob, vec![Op::Insert(order)]);
        assert_eq!(i.len(), 1);
        assert_eq!(
            i[0],
            Instruction::Single(InstructionPrimitive::Delete(
                String::from("b1"),
                Msg::PostOnlyFilled,
            ))
        );
    }

    #[test]
    fn test_post_only_accept_if_maker() {
        let mut ob = OrderBook::<TestOrder>::default();
        setup_order(&mut ob, "s1", false, 1100, 100);
        let order = TestOrder::new("b1", true, 1000, 100).with_post_only(true);
        let mut eval = Evaluator::default();
        let i = eval.eval(&ob, vec![Op::Insert(order.clone())]);
        assert_eq!(i.len(), 1);
        assert_eq!(
            i[0],
            Instruction::Single(InstructionPrimitive::Insert(order, 100))
        );
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
        let i = eval.eval(&ob, vec![Op::Insert(order)]);
        assert_eq!(i.len(), 1);
        assert_eq!(
            i[0],
            Instruction::Single(InstructionPrimitive::Delete(
                String::from("b1"),
                Msg::StpCancelTaker,
            ))
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
        let i = eval.eval(&ob, vec![Op::Insert(order.clone())]);
        assert_eq!(i.len(), 1);
        assert_eq!(
            i[0],
            Instruction::Multi(vec![InstructionPrimitive::Fill(String::from("s1"), 100,)])
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
        let i = eval.eval(&ob, vec![Op::Insert(order.clone())]);
        assert_eq!(i.len(), 1);
        assert!(matches!(
            &i[0],
            Instruction::Multi(v) if {
                let insert_b1 = v.iter().any(|p| matches!(p, InstructionPrimitive::Insert(o, r) if o.id() == "b1" && *r == 100));
                let cancels: Vec<_> = v.iter().filter_map(|p| match p { InstructionPrimitive::Delete(id, _) => Some(id.clone()), _ => None }).collect();
                insert_b1 && cancels == &[String::from("s1")]
            }
        ));
        let mut ob2 = OrderBook::<TestOrder>::default();
        setup_order_with_owner(&mut ob2, "s1", false, 1000, 100, "alice");
        let out = ob2.apply(i);
        assert_eq!(out.len(), 1);
        // b1 inserted with 100 remaining; s1 deleted
        match &out[0] {
            Output::Multi(v) => {
                assert!(v.contains(&OutputPrimitive::Inserted(100)));
                assert!(v.contains(&OutputPrimitive::Deleted));
            }
            _ => panic!("expected Multi output"),
        }
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
        let i = eval.eval(&ob, vec![Op::Insert(order.clone())]);
        assert_eq!(i.len(), 1);
        assert!(matches!(
            &i[0],
            Instruction::Multi(v) if {
                let fills: Vec<_> = v.iter().filter_map(|p| match p { InstructionPrimitive::Fill(id, q) => Some((id.clone(), *q)), _ => None }).collect();
                let cancel_ids: Vec<_> = v.iter().filter_map(|p| match p { InstructionPrimitive::Delete(id, _) => Some(id.clone()), _ => None }).collect();
                v.iter().any(|p| matches!(p, InstructionPrimitive::Insert(o, r) if o.id() == "b1" && *r == 50))
                    && fills == &[(String::from("s2"), 50)]
                    && cancel_ids == &[String::from("s1")]
            }
        ));
    }

    #[test]
    fn test_stp_cancel_both() {
        let mut ob = OrderBook::<TestOrder>::default();
        setup_order_with_owner(&mut ob, "s1", false, 1000, 100, "alice");
        let order = TestOrder::new("b1", true, 1000, 100)
            .with_owner("alice")
            .with_stp(STP::CancelBoth);
        let mut eval = Evaluator::default();
        let i = eval.eval(&ob, vec![Op::Insert(order)]);
        assert_eq!(i.len(), 1);
        assert_eq!(
            i[0],
            Instruction::Multi(vec![
                InstructionPrimitive::Delete(String::from("b1"), Msg::StpCancelBoth),
                InstructionPrimitive::Delete(String::from("s1"), Msg::StpCancelBoth),
            ])
        );
        let mut ob2 = OrderBook::<TestOrder>::default();
        setup_order_with_owner(&mut ob2, "s1", false, 1000, 100, "alice");
        let out = ob2.apply(i);
        assert_eq!(out.len(), 1);
        match &out[0] {
            Output::Multi(v) => {
                assert_eq!(
                    v.iter()
                        .filter(|p| matches!(p, OutputPrimitive::Deleted))
                        .count(),
                    2
                );
            }
            _ => panic!("expected Multi output with two Deleted"),
        }
        assert!(ob2.is_empty());
    }
}
