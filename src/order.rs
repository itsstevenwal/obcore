use std::{
    fmt::Display,
    hash::Hash,
    ops::{Add, AddAssign, Div, DivAssign, Mul, MulAssign, Sub, SubAssign},
};

/// Trait defining the interface for orders in the orderbook.
/// T: Order identifier type (must be unique). N: Numeric type.
pub trait OrderInterface {
    type T: Eq + Display + Default + Hash + Clone;
    type N: Ord
        + Eq
        + Copy
        + Hash
        + Default
        + Display
        + Add<Output = Self::N>
        + Sub<Output = Self::N>
        + Mul<Output = Self::N>
        + Div<Output = Self::N>
        + AddAssign
        + SubAssign
        + MulAssign
        + DivAssign;

    fn id(&self) -> &Self::T;
    fn is_buy(&self) -> bool;
    fn price(&self) -> Self::N;

    /// Original quantity (not updated on fill).
    fn quantity(&self) -> Self::N;

    /// Remaining quantity (updated on fill).
    fn remaining(&self) -> Self::N;

    /// Fill the order, updating remaining quantity.
    fn fill(&mut self, quantity: Self::N);
}

#[cfg(test)]
#[derive(Default, Debug, PartialEq, Eq, Clone)]
pub struct TestOrder {
    id: String,
    is_buy: bool,
    price: u64,
    quantity: u64,
    remaining: u64,
}

#[cfg(test)]
impl TestOrder {
    pub fn new(id: &str, is_buy: bool, price: u64, quantity: u64) -> Self {
        Self {
            id: id.to_string(),
            is_buy,
            price,
            quantity,
            remaining: quantity,
        }
    }
}

#[cfg(test)]
impl OrderInterface for TestOrder {
    type T = String;
    type N = u64;

    fn id(&self) -> &String {
        &self.id
    }

    fn price(&self) -> u64 {
        self.price
    }

    fn is_buy(&self) -> bool {
        self.is_buy
    }

    fn quantity(&self) -> u64 {
        self.quantity
    }

    fn remaining(&self) -> u64 {
        self.remaining
    }

    fn fill(&mut self, quantity: u64) {
        self.remaining -= quantity;
    }
}
