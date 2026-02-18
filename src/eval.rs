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
    /// STP CancelMaker: resting maker(s) cancelled (same-owner); taker may still fill against others.
    StpCancelMaker,
}

#[derive(Debug, PartialEq, Eq)]
pub enum InstructionPrimitive<O: OrderInterface> {
    /// (Order, Remaining Quantity)
    Insert(O, O::N),
    /// (Order ID, Reason)
    Delete(O::T, Msg),
    /// (Order ID, Quantity)
    Fill(O::T, O::N),
}

#[derive(Debug, PartialEq, Eq)]
pub enum Instruction<O: OrderInterface> {
    Single(InstructionPrimitive<O>),
    Multi(Vec<InstructionPrimitive<O>>),
}

/// Evaluator: turns ops into instructions without mutating the book.
///
/// Reusable — call `reset()` between independent batches. Within a single `eval()` call,
/// `temp` tracks virtual remaining qty so later ops see earlier effects. `fills` and
/// `stp_cancels` are kept as struct fields to avoid per-call heap allocation.
pub struct Evaluator<O: OrderInterface> {
    temp: FxHashMap<O::T, O::N>,
    // (maker_id, fill_qty, maker_avail_before_fill) — avail cached to skip re-hashing in temp update
    fills: Vec<(O::T, O::N, O::N)>,
    stp_cancels: Vec<O::T>,
}

impl<O: OrderInterface> Default for Evaluator<O> {
    fn default() -> Self {
        Self {
            temp: FxHashMap::default(),
            fills: Vec::new(),
            stp_cancels: Vec::new(),
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
        let mut instructions = Vec::with_capacity(ops.len());
        for op in ops {
            match op {
                Op::Insert(order) => instructions.push(self.eval_insert(ob, order)),
                Op::Delete(order_id) => instructions.push(self.eval_cancel(ob, order_id)),
            }
        }
        instructions
    }

    /// Evaluates a single insert operation.
    #[inline(always)]
    pub fn eval_insert(&mut self, ob: &OrderBook<O>, order: O) -> Instruction<O> {
        if ob.orders.contains_key(order.id()) {
            return Instruction::Single(InstructionPrimitive::Delete(
                order.id().clone(),
                Msg::OrderAlreadyExists,
            ));
        }

        let tif = order.tif();
        let post_only = order.post_only();
        let stp = order.stp();
        let taker_owner = order.owner();
        let is_buy = order.is_buy();
        let price = order.price();
        let opposite = if is_buy { &ob.asks } else { &ob.bids };

        let zero = O::N::default();
        let mut remaining = order.remaining();

        let Evaluator {
            temp,
            fills,
            stp_cancels,
        } = self;
        fills.clear();
        stp_cancels.clear();

        'outer: for level in opposite.iter() {
            if (is_buy && price < level.price()) || (!is_buy && price > level.price()) {
                break;
            }
            for maker in level.iter() {
                if remaining == zero {
                    break 'outer;
                }
                let maker_avail = *temp.get(maker.id()).unwrap_or(&maker.remaining());
                if maker_avail == zero {
                    continue;
                }
                let fill_qty = remaining.min(maker_avail);

                if post_only {
                    return Instruction::Single(InstructionPrimitive::Delete(
                        order.id().clone(),
                        Msg::PostOnlyFilled,
                    ));
                }

                if taker_owner == maker.owner() {
                    match stp {
                        STP::None => {}
                        STP::CancelTaker => {
                            return Instruction::Single(InstructionPrimitive::Delete(
                                order.id().clone(),
                                Msg::StpCancelTaker,
                            ));
                        }
                        STP::CancelMaker => {
                            stp_cancels.push(maker.id().clone());
                            temp.insert(maker.id().clone(), zero);
                            continue;
                        }
                        STP::CancelBoth => {
                            temp.insert(maker.id().clone(), zero);
                            return Instruction::Multi(vec![
                                InstructionPrimitive::Delete(
                                    order.id().clone(),
                                    Msg::StpCancelBoth,
                                ),
                                InstructionPrimitive::Delete(
                                    maker.id().clone(),
                                    Msg::StpCancelBoth,
                                ),
                            ]);
                        }
                    }
                }

                remaining -= fill_qty;
                fills.push((maker.id().clone(), fill_qty, maker_avail));
            }
        }

        if tif == TIF::FOK && remaining > zero {
            return Instruction::Single(InstructionPrimitive::Delete(
                order.id().clone(),
                Msg::FOKNotFilled,
            ));
        }

        // Record fills in temp so later ops in this batch see reduced maker qty.
        // maker_avail was cached in fills to avoid re-hashing temp here.
        for &(ref id, qty, avail) in fills.iter() {
            temp.insert(id.clone(), avail - qty);
        }

        let has_activity = !fills.is_empty() || !stp_cancels.is_empty();
        let rest = if tif == TIF::IOC { zero } else { remaining };

        if has_activity {
            let cap = fills.len() + stp_cancels.len() + usize::from(rest > zero);
            let mut out = Vec::with_capacity(cap);
            out.extend(
                fills
                    .drain(..)
                    .map(|(id, qty, _)| InstructionPrimitive::Fill(id, qty)),
            );
            for id in stp_cancels.drain(..) {
                out.push(InstructionPrimitive::Delete(id, Msg::StpCancelMaker));
            }
            if rest > zero {
                out.push(InstructionPrimitive::Insert(order, rest));
            }
            return Instruction::Multi(out);
        }

        if tif == TIF::IOC {
            return Instruction::Single(InstructionPrimitive::Delete(
                order.id().clone(),
                Msg::IOCNoFill,
            ));
        }

        Instruction::Single(InstructionPrimitive::Insert(order, remaining))
    }

    /// Evaluates a single cancel operation.
    #[inline(always)]
    pub fn eval_cancel(&mut self, ob: &OrderBook<O>, order_id: O::T) -> Instruction<O> {
        if !ob.orders.contains_key(&order_id) {
            return Instruction::Single(InstructionPrimitive::Delete(order_id, Msg::OrderNotFound));
        }
        self.temp.insert(order_id.clone(), O::N::default()); // later ops in batch treat as gone
        Instruction::Single(InstructionPrimitive::Delete(order_id, Msg::UserCancelled))
    }
}
