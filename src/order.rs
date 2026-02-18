use std::{
    fmt::Display,
    hash::Hash,
    ops::{Add, AddAssign, Div, DivAssign, Mul, MulAssign, Sub, SubAssign},
};

/// Time in force for an order.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum TimeInForce {
    /// Fill entire order immediately or cancel (no resting quantity).
    FOK,
    /// Fill as much as possible immediately, cancel the rest (no resting quantity).
    IOC,
    /// Good till cancelled; remainder rests on the book.
    #[default]
    GTC,
}

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

    /// Time in force: FOK, IOC, or GTC. Default is GTC.
    fn tif(&self) -> TimeInForce {
        TimeInForce::GTC
    }

    /// If true, order must not take; it is rejected if it would cross the spread.
    fn post_only(&self) -> bool {
        false
    }
}

#[cfg(test)]
#[derive(Default, Debug, PartialEq, Eq, Clone)]
pub struct TestOrder {
    id: String,
    is_buy: bool,
    price: u64,
    quantity: u64,
    remaining: u64,
    tif: TimeInForce,
    post_only: bool,
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
            tif: TimeInForce::GTC,
            post_only: false,
        }
    }

    pub fn with_tif(mut self, tif: TimeInForce) -> Self {
        self.tif = tif;
        self
    }

    pub fn with_post_only(mut self, post_only: bool) -> Self {
        self.post_only = post_only;
        self
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

    fn tif(&self) -> TimeInForce {
        self.tif
    }

    fn post_only(&self) -> bool {
        self.post_only
    }
}
