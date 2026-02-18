use crate::{
    hash::FxHashMap,
    ob::OrderBook,
    order::{OrderInterface, TimeInForce},
};

/// An operation to apply to the orderbook.
pub enum Op<O: OrderInterface> {
    Insert(O),
    Delete(O::T),
}

#[derive(Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum Msg {
    OrderNotFound,
    OrderAlreadyExists,
    /// Post-only order would have crossed the spread (would take).
    PostOnlyWouldTake,
    /// FOK order could not be fully filled.
    FOKNotFilled,
    /// IOC order had no match (nothing to do).
    IOCNoFill,
}

#[derive(Debug, PartialEq, Eq)]
pub enum Instruction<O: OrderInterface> {
    // (Order, Remaining Quantity)
    Insert(O, O::N),
    // (Order ID)
    Delete(O::T),
    // (Taker order, remaining, makers). Apply inserts the order when remaining > 0.
    Match(O, O::N, Vec<(O::T, O::N)>),
    // (Order ID, Message)
    NoOp(O::T, Msg),
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
    /// Resets the evaluator's temporary state.
    #[inline]
    pub fn reset(&mut self) {
        self.temp.clear();
    }

    /// Evaluates a batch of operations against the orderbook without mutating it.
    /// Returns one instruction per op.
    #[inline]
    pub fn eval(&mut self, ob: &OrderBook<O>, ops: Vec<Op<O>>) -> Vec<Instruction<O>> {
        let mut instructions = Vec::new();
        for op in ops {
            match op {
                Op::Insert(order) => instructions.push(self.eval_insert_inner(ob, order)),
                Op::Delete(order_id) => instructions.push(self.eval_cancel_inner(ob, order_id)),
            }
        }
        instructions
    }

    /// Evaluates a single insert operation. Only available with the `bench` feature.
    #[cfg(feature = "bench")]
    #[inline(always)]
    pub fn eval_insert(&mut self, ob: &OrderBook<O>, order: O) -> Instruction<O> {
        self.eval_insert_inner(ob, order)
    }

    #[inline(always)]
    fn eval_insert_inner(&mut self, ob: &OrderBook<O>, order: O) -> Instruction<O> {
        if ob.orders.contains_key(order.id()) {
            return Instruction::NoOp(order.id().clone(), Msg::OrderAlreadyExists);
        }

        let tif = order.tif();
        let post_only = order.post_only();

        let mut remaining_quantity = order.remaining();
        let mut maker_matches = Vec::new();

        let is_buy = order.is_buy();
        let price = order.price();

        let opposite_side = if is_buy { &ob.asks } else { &ob.bids };

        // Compute matches without updating temp yet (so we can reject post_only / FOK without side effects)
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
                let remaining = *self
                    .temp
                    .get(resting_order.id())
                    .unwrap_or(&resting_order.remaining());

                if remaining == O::N::default() {
                    continue;
                }
                let taken_quantity = remaining_quantity.min(remaining);
                if post_only && taken_quantity > O::N::default() {
                    return Instruction::NoOp(order.id().clone(), Msg::PostOnlyWouldTake);
                }
                remaining_quantity -= taken_quantity;
                maker_matches.push((resting_order.id().clone(), taken_quantity));
            }
        }

        if tif == TimeInForce::FOK && remaining_quantity > O::N::default() {
            return Instruction::NoOp(order.id().clone(), Msg::FOKNotFilled);
        }

        // Apply maker quantity updates for subsequent ops in the batch
        for (maker_id, taken_quantity) in &maker_matches {
            let remaining = self.temp.get(maker_id).copied().unwrap_or_else(|| {
                ob.order(maker_id)
                    .map(|o| o.remaining())
                    .unwrap_or(O::N::default())
            });
            self.temp
                .insert(maker_id.clone(), remaining - *taken_quantity);
        }

        if !maker_matches.is_empty() {
            // IOC: set remaining to 0 so apply does not insert the unfilled portion
            let remaining = if tif == TimeInForce::IOC {
                O::N::default()
            } else {
                remaining_quantity
            };
            return Instruction::Match(order, remaining, maker_matches);
        }

        if tif == TimeInForce::IOC {
            return Instruction::NoOp(order.id().clone(), Msg::IOCNoFill);
        }

        Instruction::Insert(order, remaining_quantity)
    }

    /// Evaluates a single cancel operation. Only available with the `bench` feature.
    #[cfg(feature = "bench")]
    #[inline(always)]
    pub fn eval_cancel(&mut self, ob: &OrderBook<O>, order_id: O::T) -> Instruction<O> {
        self.eval_cancel_inner(ob, order_id)
    }

    #[inline(always)]
    fn eval_cancel_inner(&mut self, ob: &OrderBook<O>, order_id: O::T) -> Instruction<O> {
        if !ob.orders.contains_key(&order_id) {
            return Instruction::NoOp(order_id, Msg::OrderNotFound);
        }
        self.temp.insert(order_id.clone(), O::N::default());
        Instruction::Delete(order_id)
    }
}
