use crate::{level::Level, list::Node, order::OrderInterface};
use std::collections::BTreeMap;

/// One side of an orderbook (bids or asks).
/// `is_bid` determines iteration direction (highest-first vs lowest-first).
pub struct Side<O: OrderInterface> {
    is_bid: bool,
    levels: BTreeMap<O::N, Level<O>>,
}

impl<O: OrderInterface> Side<O> {
    #[inline]
    pub fn new(is_bid: bool) -> Self {
        Side {
            is_bid,
            levels: BTreeMap::new(),
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
        let (_, level) = if self.is_bid {
            self.levels.last_key_value()?
        } else {
            self.levels.first_key_value()?
        };
        Some((level.price(), level.total_quantity()))
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
        self.levels
            .entry(price)
            .or_insert_with(|| Level::new(price))
            .add_order(order)
    }

    /// Fills an order and returns true if fully filled.
    /// Caller must ensure node_ptr is valid and in this side.
    #[inline(always)]
    #[allow(clippy::not_unsafe_ptr_arg_deref)]
    pub fn fill_order(&mut self, node_ptr: *mut Node<O>, fill: O::N) -> bool {
        let order = unsafe { &mut (*node_ptr).data };
        let price = order.price();
        let btree_map::Entry::Occupied(mut entry) = self.levels.entry(price) else {
            unreachable!()
        };
        let level = entry.get_mut();
        let removed = level.fill_order(node_ptr, order, fill);
        if level.is_empty() {
            entry.remove();
        }
        removed
    }

    /// Removes an order by its node pointer.
    /// Caller must ensure node_ptr is valid and in this side.
    #[inline(always)]
    #[allow(clippy::not_unsafe_ptr_arg_deref)]
    pub fn remove_order(&mut self, node_ptr: *mut Node<O>) {
        let price = unsafe { (*node_ptr).data.price() };
        let btree_map::Entry::Occupied(mut entry) = self.levels.entry(price) else {
            unreachable!()
        };
        let level = entry.get_mut();
        level.remove_order(node_ptr);
        if level.is_empty() {
            entry.remove();
        }
    }

    #[inline]
    pub fn iter(&self) -> LevelIter<'_, O> {
        LevelIter {
            is_bid: self.is_bid,
            inner: self.levels.iter(),
        }
    }

    #[inline]
    pub fn iter_mut(&mut self) -> LevelIterMut<'_, O> {
        LevelIterMut {
            is_bid: self.is_bid,
            inner: self.levels.iter_mut(),
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Iterators
// ─────────────────────────────────────────────────────────────────────────────

use std::collections::btree_map;

pub struct LevelIter<'a, O: OrderInterface> {
    is_bid: bool,
    inner: btree_map::Iter<'a, O::N, Level<O>>,
}

impl<'a, O: OrderInterface> Iterator for LevelIter<'a, O> {
    type Item = &'a Level<O>;

    #[inline(always)]
    fn next(&mut self) -> Option<Self::Item> {
        let (_, level) = if self.is_bid {
            self.inner.next_back()?
        } else {
            self.inner.next()?
        };
        Some(level)
    }
}

pub struct LevelIterMut<'a, O: OrderInterface> {
    is_bid: bool,
    inner: btree_map::IterMut<'a, O::N, Level<O>>,
}

impl<'a, O: OrderInterface> Iterator for LevelIterMut<'a, O> {
    type Item = &'a mut Level<O>;

    #[inline(always)]
    fn next(&mut self) -> Option<Self::Item> {
        let (_, level) = if self.is_bid {
            self.inner.next_back()?
        } else {
            self.inner.next()?
        };
        Some(level)
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
