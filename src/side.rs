use crate::{hash::FxHashMap, level::Level, list::Node, order::OrderInterface};
use std::collections::BTreeSet;

/// One side of an orderbook (bids or asks). Uses BTreeMap for price-sorted levels.
/// `is_bid` determines iteration direction at runtime.
pub struct Side<O: OrderInterface> {
    is_bid: bool,
    prices: BTreeSet<O::N>,
    levels: FxHashMap<O::N, Level<O>>,
}

impl<O: OrderInterface> Side<O> {
    #[inline]
    pub fn new(is_bid: bool) -> Self {
        Side {
            is_bid,
            prices: BTreeSet::new(),
            levels: FxHashMap::default(),
        }
    }

    #[inline]
    pub fn height(&self) -> usize {
        self.levels.len()
    }

    #[inline]
    pub fn is_empty(&self) -> bool {
        self.levels.is_empty()
    }

    /// Returns the best price level (price, total_quantity).
    /// For bids: highest price. For asks: lowest price.
    #[inline]
    pub fn best(&self) -> Option<(O::N, O::N)> {
        let price = if self.is_bid {
            self.prices.last()
        } else {
            self.prices.first()
        };
        price.map(|p| {
            let level = self.levels.get(p).unwrap();
            (*p, level.total_quantity())
        })
    }

    /// Returns the top `n` price levels as (price, total_quantity).
    /// For bids: highest prices first. For asks: lowest prices first.
    #[inline]
    pub fn top(&self, n: usize) -> Vec<(O::N, O::N)> {
        self.iter()
            .take(n)
            .map(|l| (l.price(), l.total_quantity()))
            .collect()
    }

    #[inline(always)]
    pub fn insert_order(&mut self, order: O) -> *mut Node<O> {
        let price = order.price();
        if let Some(level) = self.levels.get_mut(&price) {
            level.add_order(order)
        } else {
            let mut level = Level::new(price);
            let node_ptr = level.add_order(order);
            self.prices.insert(price);
            self.levels.insert(price, level);
            node_ptr
        }
    }

    #[inline(always)]
    fn level_mut(&mut self, price: O::N) -> &mut Level<O> {
        self.levels
            .get_mut(&price)
            .expect("node_ptr must point to valid order in this side")
    }

    #[inline(always)]
    fn cleanup_level(&mut self, price: O::N, level_empty: bool) {
        if level_empty {
            self.prices.remove(&price);
            self.levels.remove(&price);
        }
    }

    /// Fills an order and returns true if fully filled.
    /// Caller must ensure node_ptr is valid and in this side.
    #[inline(always)]
    #[allow(clippy::not_unsafe_ptr_arg_deref)]
    pub fn fill_order(&mut self, node_ptr: *mut Node<O>, fill: O::N) -> bool {
        let order = unsafe { &mut (*node_ptr).data };
        let price = order.price();
        let level = self.level_mut(price);
        let removed = level.fill_order(node_ptr, order, fill);
        let empty = level.is_empty();
        self.cleanup_level(price, empty);
        removed
    }

    /// Removes an order by its node pointer.
    /// Caller must ensure node_ptr is valid and in this side.
    #[inline(always)]
    #[allow(clippy::not_unsafe_ptr_arg_deref)]
    pub fn remove_order(&mut self, node_ptr: *mut Node<O>) {
        let price = unsafe { (*node_ptr).data.price() };
        let level = self.level_mut(price);
        level.remove_order(node_ptr);
        let empty = level.is_empty();
        self.cleanup_level(price, empty);
    }

    /// Bids: highest price first. Asks: lowest price first.
    #[inline]
    pub fn iter(&self) -> LevelIter<'_, O> {
        LevelIter {
            is_bid: self.is_bid,
            prices_iter: self.prices.iter(),
            levels: &self.levels,
        }
    }

    /// Bids: highest price first. Asks: lowest price first.
    #[inline]
    pub fn iter_mut(&mut self) -> LevelIterMut<'_, O> {
        LevelIterMut {
            is_bid: self.is_bid,
            prices_iter: self.prices.iter(),
            levels: &mut self.levels,
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Iterators
// ─────────────────────────────────────────────────────────────────────────────

use std::collections::btree_set;

pub struct LevelIter<'a, O: OrderInterface> {
    is_bid: bool,
    prices_iter: btree_set::Iter<'a, O::N>,
    levels: &'a FxHashMap<O::N, Level<O>>,
}

impl<'a, O: OrderInterface> Iterator for LevelIter<'a, O> {
    type Item = &'a Level<O>;

    #[inline(always)]
    fn next(&mut self) -> Option<Self::Item> {
        let price = if self.is_bid {
            self.prices_iter.next_back()
        } else {
            self.prices_iter.next()
        };
        price.map(|p| self.levels.get(p).unwrap())
    }
}

pub struct LevelIterMut<'a, O: OrderInterface> {
    is_bid: bool,
    prices_iter: btree_set::Iter<'a, O::N>,
    levels: &'a mut FxHashMap<O::N, Level<O>>,
}

impl<'a, O: OrderInterface> Iterator for LevelIterMut<'a, O> {
    type Item = &'a mut Level<O>;

    #[inline(always)]
    fn next(&mut self) -> Option<Self::Item> {
        let price = if self.is_bid {
            self.prices_iter.next_back()
        } else {
            self.prices_iter.next()
        };
        price.map(|p| {
            let level = self.levels.get_mut(p).unwrap();
            // SAFETY: We iterate through each price exactly once, so we never
            // create overlapping mutable references to the same level.
            unsafe { &mut *(level as *mut Level<O>) }
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::order::TestOrder;

    #[test]
    fn test_new_side() {
        let side = Side::<TestOrder>::new(true);
        assert!(side.is_empty());
        assert_eq!(side.height(), 0);
    }

    #[test]
    fn test_insert_order() {
        let mut side = Side::<TestOrder>::new(true);
        let _node_ptr = side.insert_order(TestOrder::new("1", true, 100, 50));
        assert!(!side.is_empty());
        assert_eq!(side.height(), 1);
    }

    #[test]
    fn test_insert_multiple_orders_same_price() {
        let mut side = Side::<TestOrder>::new(true);
        side.insert_order(TestOrder::new("1", true, 100, 50));
        side.insert_order(TestOrder::new("2", true, 100, 30));
        assert_eq!(side.height(), 1);
    }

    #[test]
    fn test_insert_orders_different_prices() {
        let mut side = Side::<TestOrder>::new(true);
        side.insert_order(TestOrder::new("1", true, 100, 50));
        side.insert_order(TestOrder::new("2", true, 200, 30));
        side.insert_order(TestOrder::new("3", true, 150, 20));
        assert_eq!(side.height(), 3);
    }

    #[test]
    fn test_remove_order() {
        let mut side = Side::<TestOrder>::new(true);
        let node_ptr = side.insert_order(TestOrder::new("1", true, 100, 50));
        side.insert_order(TestOrder::new("2", true, 100, 30));
        side.remove_order(node_ptr);
        assert_eq!(side.height(), 1);
    }

    #[test]
    fn test_remove_order_single_order() {
        let mut side = Side::<TestOrder>::new(true);
        let node_ptr = side.insert_order(TestOrder::new("1", true, 100, 50));
        side.remove_order(node_ptr);
        let level_count: usize = side.iter().count();
        assert_eq!(level_count, 0);
    }

    #[test]
    fn test_iter_bids() {
        let mut side = Side::<TestOrder>::new(true);
        side.insert_order(TestOrder::new("1", true, 100, 50));
        side.insert_order(TestOrder::new("2", true, 300, 30));
        side.insert_order(TestOrder::new("3", true, 200, 20));
        let prices: Vec<u64> = side.iter().map(|level| level.price()).collect();
        assert_eq!(prices, vec![300, 200, 100]);
    }

    #[test]
    fn test_iter_asks() {
        let mut side = Side::<TestOrder>::new(false);
        side.insert_order(TestOrder::new("1", false, 100, 50));
        side.insert_order(TestOrder::new("2", false, 300, 30));
        side.insert_order(TestOrder::new("3", false, 200, 20));
        let prices: Vec<u64> = side.iter().map(|level| level.price()).collect();
        assert_eq!(prices, vec![100, 200, 300]);
    }

    #[test]
    fn test_iter_mut() {
        let mut side = Side::<TestOrder>::new(true);
        side.insert_order(TestOrder::new("1", true, 100, 50));
        side.insert_order(TestOrder::new("2", true, 200, 30));
        for level in side.iter_mut() {
            let _ = level.price();
        }
        assert_eq!(side.height(), 2);
    }

    #[test]
    fn test_height() {
        let mut side = Side::<TestOrder>::new(true);
        assert_eq!(side.height(), 0);
        side.insert_order(TestOrder::new("1", true, 100, 50));
        assert_eq!(side.height(), 1);
        side.insert_order(TestOrder::new("2", true, 200, 30));
        assert_eq!(side.height(), 2);
        side.insert_order(TestOrder::new("3", true, 100, 20));
        assert_eq!(side.height(), 2);
    }
}
