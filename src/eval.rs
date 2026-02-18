use crate::{
    hash::FxHashMap,
    ob::OrderBook,
    order::{OrderInterface, STP, TIF},
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
    /// STP CancelTaker: incoming order would match same-owner maker(s), taker cancelled.
    StpCancelTaker,
    /// Order was successfully cancelled (removed from book or rejected).
    Cancelled,
}

#[derive(Debug, PartialEq, Eq)]
pub enum Instruction<O: OrderInterface> {
    // (Order, Remaining Quantity)
    Insert(O, O::N),
    /// Cancel/reject one or two orders. (id, reason), optional second. Apply outputs (id, 0) for each.
    Delete((O::T, Msg), Option<(O::T, Msg)>),
    /// (Taker order, remaining, maker matches, makers to cancel). Apply inserts the order when remaining > 0.
    Match(O, O::N, Vec<(O::T, O::N)>, Vec<O::T>),
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
            return Instruction::Delete((order.id().clone(), Msg::OrderAlreadyExists), None);
        }

        let tif = order.tif();
        let post_only = order.post_only();
        let stp = order.stp();
        let taker_owner = order.owner();
        let is_buy = order.is_buy();
        let price = order.price();
        let opposite_side = if is_buy { &ob.asks } else { &ob.bids };

        let mut remaining_quantity = order.remaining();
        let mut maker_matches = Vec::new();
        let mut cancel_maker_ids = Vec::new();

        'outer: for level in opposite_side.iter() {
            let dominated = is_buy && price < level.price() || !is_buy && price > level.price();
            if dominated {
                break;
            }
            for resting_order in level.iter() {
                if remaining_quantity == O::N::default() {
                    break 'outer;
                }
                let maker_remaining = *self
                    .temp
                    .get(resting_order.id())
                    .unwrap_or(&resting_order.remaining());
                if maker_remaining == O::N::default() {
                    continue;
                }
                let taken_quantity = remaining_quantity.min(maker_remaining);
                let same_owner = taker_owner == resting_order.owner();

                if post_only && taken_quantity > O::N::default() {
                    return Instruction::Delete((order.id().clone(), Msg::PostOnlyWouldTake), None);
                }

                // Same-owner: STP may reject taker, cancel maker(s), or skip fill
                let take_from_maker = !same_owner || matches!(stp, STP::None | STP::CancelTaker);
                if !take_from_maker {
                    if same_owner {
                        if stp == STP::CancelMaker {
                            cancel_maker_ids.push(resting_order.id().clone());
                            self.temp
                                .insert(resting_order.id().clone(), O::N::default());
                        } else if stp == STP::CancelBoth && taken_quantity > O::N::default() {
                            self.temp
                                .insert(resting_order.id().clone(), O::N::default());
                            return Instruction::Delete(
                                (order.id().clone(), Msg::StpCancelTaker),
                                Some((resting_order.id().clone(), Msg::Cancelled)),
                            );
                        }
                    }
                    continue;
                }
                if stp == STP::CancelTaker && same_owner && taken_quantity > O::N::default() {
                    return Instruction::Delete((order.id().clone(), Msg::StpCancelTaker), None);
                }
                remaining_quantity -= taken_quantity;
                maker_matches.push((resting_order.id().clone(), taken_quantity));
            }
        }

        if tif == TIF::FOK && remaining_quantity > O::N::default() {
            return Instruction::Delete((order.id().clone(), Msg::FOKNotFilled), None);
        }

        for (maker_id, taken_quantity) in &maker_matches {
            let remaining = self.temp.get(maker_id).copied().unwrap_or_else(|| {
                ob.order(maker_id)
                    .map(|o| o.remaining())
                    .unwrap_or(O::N::default())
            });
            self.temp
                .insert(maker_id.clone(), remaining - *taken_quantity);
        }

        let has_match_or_cancel = !maker_matches.is_empty() || !cancel_maker_ids.is_empty();
        let remaining = if tif == TIF::IOC {
            O::N::default()
        } else {
            remaining_quantity
        };
        if has_match_or_cancel {
            return Instruction::Match(order, remaining, maker_matches, cancel_maker_ids);
        }
        if tif == TIF::IOC {
            return Instruction::Delete((order.id().clone(), Msg::IOCNoFill), None);
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
            return Instruction::Delete((order_id, Msg::OrderNotFound), None);
        }
        self.temp.insert(order_id.clone(), O::N::default());
        Instruction::Delete((order_id, Msg::Cancelled), None)
    }
}
