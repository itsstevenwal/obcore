use crate::{hash::FxHashMap, ob::OrderBook, order::OrderInterface};

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
}

#[derive(Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum Role {
    Taker,
    Maker,
}

#[derive(Debug, PartialEq, Eq)]
pub enum Instruction<O: OrderInterface> {
    // (Order, Remaining Quantity)
    Insert(O, O::N),
    // (Order ID)
    Delete(O::T),
    // (Order ID, Quantity, Role)
    Fill(O::T, O::N, Role),
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
    /// Returns matches and instructions that can later be applied.
    #[inline]
    pub fn eval(&mut self, ob: &OrderBook<O>, ops: Vec<Op<O>>) -> Vec<Instruction<O>> {
        let mut instructions = Vec::with_capacity(ops.len() * 10);
        for op in ops {
            match op {
                Op::Insert(order) => {
                    self.eval_insert(ob, order, &mut instructions);
                }
                Op::Delete(order_id) => {
                    self.eval_cancel(ob, order_id, &mut instructions);
                }
            }
        }

        instructions
    }

    #[inline(always)]
    fn eval_insert(&mut self, ob: &OrderBook<O>, order: O, instructions: &mut Vec<Instruction<O>>) {
        if ob.orders.contains_key(order.id()) {
            instructions.push(Instruction::NoOp(
                order.id().clone(),
                Msg::OrderAlreadyExists,
            ));
        }

        let mut remaining_quantity = order.remaining();
        let mut taker_quantity = O::N::default();

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
                let remaining = *self
                    .temp
                    .get(resting_order.id())
                    .unwrap_or(&resting_order.remaining());

                if remaining == O::N::default() {
                    continue;
                }
                let taken_quantity = remaining_quantity.min(remaining);
                remaining_quantity -= taken_quantity;
                taker_quantity += taken_quantity;
                self.temp
                    .insert(resting_order.id().clone(), remaining - taken_quantity);

                // Leave space for the taker instruction
                instructions.push(Instruction::Fill(
                    resting_order.id().clone(),
                    taken_quantity,
                    Role::Maker,
                ));
            }
        }

        let taker_id = order.id().clone();
        if taker_quantity > O::N::default() {
            instructions.push(Instruction::Fill(taker_id, taker_quantity, Role::Taker));
        }

        if remaining_quantity > O::N::default() {
            instructions.push(Instruction::Insert(order, remaining_quantity));
        }
    }

    #[inline(always)]
    fn eval_cancel(
        &mut self,
        ob: &OrderBook<O>,
        order_id: O::T,
        instructions: &mut Vec<Instruction<O>>,
    ) {
        if !ob.orders.contains_key(&order_id) {
            instructions.push(Instruction::NoOp(order_id, Msg::OrderNotFound));
            return;
        }
        self.temp.insert(order_id.clone(), O::N::default());
        instructions.push(Instruction::Delete(order_id));
    }
}
