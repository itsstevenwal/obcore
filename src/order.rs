use std::{
    fmt::{Debug, Display},
    hash::Hash,
    ops::{Add, AddAssign, Div, DivAssign, Mul, MulAssign, Sub, SubAssign},
};

/// Time in force for an order.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum TIF {
    /// Good till cancelled; remainder rests on the book.
    #[default]
    GTC,
    /// Fill entire order immediately or cancel (no resting quantity).
    FOK,
    /// Fill as much as possible immediately, cancel the rest (no resting quantity).
    IOC,
}

/// Self-trade protection mode when taker and maker share the same owner.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum STP {
    /// No self-trade protection.
    #[default]
    None,
    /// Cancel the incoming (taker) order when it would match against same-owner makers.
    CancelTaker,
    /// Cancel the resting (maker) order(s) that would self-trade; taker fills against others.
    CancelMaker,
    /// Cancel both taker and maker(s) involved in the self-trade.
    CancelBoth,
}

/// Trait defining the interface for orders in the orderbook.
/// T: Order identifier type (must be unique). N: Numeric type.
pub trait OrderInterface {
    type I: Eq + Display + Default + Hash + Clone + Debug;
    type N: Ord
        + Eq
        + Copy
        + Hash
        + Default
        + Display
        + Debug
        + Add<Output = Self::N>
        + Sub<Output = Self::N>
        + Mul<Output = Self::N>
        + Div<Output = Self::N>
        + AddAssign
        + SubAssign
        + MulAssign
        + DivAssign;

    /// Owner/trader id for self-trade detection.
    type O: Eq + Display + Default + Hash + Clone + Debug;

    fn id(&self) -> &Self::I;
    fn is_buy(&self) -> bool;
    fn price(&self) -> Self::N;

    /// Original quantity (not updated on fill).
    fn quantity(&self) -> Self::N;

    /// Remaining quantity (updated on fill).
    fn remaining(&self) -> Self::N;

    /// Fill the order, updating remaining quantity.
    fn fill(&mut self, quantity: Self::N);

    /// Owner id for self-trade protection.
    fn owner(&self) -> &Self::O;

    /// Time in force: FOK, IOC, or GTC. Default is GTC.
    fn tif(&self) -> TIF {
        TIF::GTC
    }

    fn stp(&self) -> STP {
        STP::None
    }

    /// If true, order must not take; it is rejected if it would cross the spread.
    fn post_only(&self) -> bool {
        false
    }
}

#[cfg(test)]
#[derive(Debug, PartialEq, Eq, Clone)]
pub struct TestOrder {
    id: String,
    is_buy: bool,
    price: u64,
    quantity: u64,
    remaining: u64,
    tif: TIF,
    stp: STP,
    post_only: bool,
    owner: String,
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
            tif: TIF::GTC,
            stp: STP::None,
            post_only: false,
            owner: id.to_string(),
        }
    }

    pub fn with_tif(mut self, tif: TIF) -> Self {
        self.tif = tif;
        self
    }

    pub fn with_post_only(mut self, post_only: bool) -> Self {
        self.post_only = post_only;
        self
    }

    pub fn with_stp(mut self, stp: STP) -> Self {
        self.stp = stp;
        self
    }

    pub fn with_owner(mut self, owner: &str) -> Self {
        self.owner = owner.to_string();
        self
    }
}

#[cfg(test)]
impl OrderInterface for TestOrder {
    type I = String;
    type N = u64;
    type O = String;

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

    fn owner(&self) -> &String {
        &self.owner
    }

    fn tif(&self) -> TIF {
        self.tif
    }

    fn post_only(&self) -> bool {
        self.post_only
    }

    fn stp(&self) -> STP {
        self.stp
    }
}
