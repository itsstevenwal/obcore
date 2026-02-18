//! Eval: compute instructions from ops without mutating the book.
//!
//! - **Batch**: `eval(ob, [op1, op2, ...])` returns one instruction per op. The book `ob` is read-only.
//!   Later ops in the same batch "see" earlier effects via `temp`: we track virtual remaining qty
//!   for orders that were matched or cancelled by earlier ops, so we don't double-fill or match
//!   against already-cancelled orders.
//! - **Apply** (in `ob`) takes the instructions and mutates the book; call it after eval.

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
    // Order not found on the book.
    OrderNotFound,
    // Order already exists on the book.
    OrderAlreadyExists,
    /// Order was successfully cancelled (removed from book or rejected).
    UserCancelled,
    /// Post-only order would have crossed the spread (would fill).
    PostOnlyFilled,
    /// FOK order could not be fully filled.
    FOKNotFilled,
    /// IOC order had no match (nothing to do).
    IOCNoFill,
    /// STP CancelTaker: incoming order would match same-owner maker(s), taker cancelled.
    StpCancelTaker,
    /// STP CancelBoth: incoming order would match same-owner maker(s), both taker and maker(s) cancelled.
    StpCancelBoth,
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

/// Evaluator: turns ops into instructions without mutating the book.
/// `temp`: order id → virtual remaining qty for this batch (filled/cancelled by earlier ops).
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

    /// Evaluates a batch of ops; returns one instruction per op. Does not mutate `ob`.
    /// Uses `temp` so each op sees the effect of previous ops in the batch (e.g. maker qty already taken).
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

    /// Inserts an incoming order: reject if duplicate, else walk opposite side and compute
    /// matches (and any STP cancels). Returns Insert / Match / Delete(reject) without mutating the book.
    #[inline(always)]
    fn eval_insert_inner(&mut self, ob: &OrderBook<O>, order: O) -> Instruction<O> {
        // Reject if order id already on book.
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
        let mut maker_matches = Vec::new(); // (maker_id, qty) for each fill
        let mut cancel_maker_ids = Vec::new(); // same-owner makers to cancel (STP CancelMaker)
        let zero = O::N::default();

        // --- Matching loop: opposite side, best price first, FIFO within level ---
        // Stop when taker is filled (remaining_quantity == 0) or we leave the book (dominated).
        'outer: for level in opposite_side.iter() {
            let dominated = is_buy && price < level.price() || !is_buy && price > level.price();
            if dominated {
                break;
            }
            for resting_order in level.iter() {
                if remaining_quantity == zero {
                    break 'outer;
                }
                // Maker's available qty: from temp (if reduced by earlier ops in batch) else from book.
                let maker_remaining = *self
                    .temp
                    .get(resting_order.id())
                    .unwrap_or(&resting_order.remaining());
                if maker_remaining == zero {
                    continue;
                }
                let taken_quantity = remaining_quantity.min(maker_remaining);
                let same_owner = taker_owner == resting_order.owner();

                if post_only && taken_quantity > zero {
                    return Instruction::Delete((order.id().clone(), Msg::PostOnlyFilled), None);
                }

                // STP: same-owner → don't take from this maker (or reject taker / cancel both).
                // CancelMaker: record maker for cancel, skip fill. CancelBoth: reject taker + cancel this maker, return now.
                // CancelTaker: if we would take from same-owner, reject taker and return.
                let take_from_maker = !same_owner || matches!(stp, STP::None | STP::CancelTaker);
                if !take_from_maker {
                    if same_owner {
                        if stp == STP::CancelMaker {
                            cancel_maker_ids.push(resting_order.id().clone());
                            self.temp.insert(resting_order.id().clone(), zero);
                        } else if stp == STP::CancelBoth && taken_quantity > zero {
                            self.temp.insert(resting_order.id().clone(), zero);
                            return Instruction::Delete(
                                (order.id().clone(), Msg::StpCancelBoth),
                                Some((resting_order.id().clone(), Msg::StpCancelBoth)),
                            );
                        }
                    }
                    continue;
                }
                if stp == STP::CancelTaker && same_owner && taken_quantity > zero {
                    return Instruction::Delete((order.id().clone(), Msg::StpCancelTaker), None);
                }
                remaining_quantity -= taken_quantity;
                maker_matches.push((resting_order.id().clone(), taken_quantity));
            }
        }

        if tif == TIF::FOK && remaining_quantity > zero {
            return Instruction::Delete((order.id().clone(), Msg::FOKNotFilled), None);
        }

        // Record fills in temp so later ops in this batch see reduced maker qty.
        for (maker_id, taken_quantity) in &maker_matches {
            let remaining = self
                .temp
                .get(maker_id)
                .copied()
                .unwrap_or_else(|| ob.order(maker_id).map(|o| o.remaining()).unwrap_or(zero));
            self.temp
                .insert(maker_id.clone(), remaining - *taken_quantity);
        }

        // Emit instruction: Match (if any fill or STP CancelMaker), else Insert or IOC no-fill reject.
        let has_match_or_cancel = !maker_matches.is_empty() || !cancel_maker_ids.is_empty();
        let remaining = if tif == TIF::IOC {
            zero
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
        self.temp.insert(order_id.clone(), O::N::default()); // later ops in batch treat as gone
        Instruction::Delete((order_id, Msg::UserCancelled), None)
    }
}
