use crate::{
    list::{Iter, IterMut, List, Node},
    order::OrderInterface,
};

/// A price level containing all orders at a specific price point.
pub struct Level<O: OrderInterface> {
    price: O::N,
    orders: List<O>,
    /// Total quantity across all orders (cached for performance).
    total_quantity: O::N,
}

impl<O: OrderInterface> Level<O> {
    #[inline]
    pub fn new(price: O::N) -> Self {
        Self {
            price,
            orders: List::new(),
            total_quantity: O::N::default(),
        }
    }

    #[inline]
    pub fn price(&self) -> O::N {
        self.price
    }

    #[inline]
    pub fn total_quantity(&self) -> O::N {
        self.total_quantity
    }

    #[inline]
    pub fn len(&self) -> usize {
        self.orders.len()
    }

    #[inline(always)]
    pub fn is_empty(&self) -> bool {
        self.orders.is_empty()
    }

    /// Adds an order to this level (FIFO). Returns pointer to the inserted node.
    #[inline(always)]
    pub fn add_order(&mut self, order: O) -> *mut Node<O> {
        self.total_quantity += order.remaining();
        self.orders.push_back(order)
    }

    /// Fills an order and returns true if fully filled.
    #[inline(always)]
    pub fn fill_order(&mut self, node_ptr: *mut Node<O>, order: &mut O, fill: O::N) -> bool {
        order.fill(fill);
        self.total_quantity -= fill;
        if order.remaining() == O::N::default() {
            self.orders.remove(node_ptr);
            return true;
        }
        false
    }

    #[inline(always)]
    pub fn remove_order(&mut self, node_ptr: *mut Node<O>) {
        if let Some(order) = self.orders.remove(node_ptr) {
            self.total_quantity -= order.remaining();
        }
    }

    #[inline(always)]
    pub fn iter(&self) -> Iter<'_, O> {
        self.orders.iter()
    }

    #[inline(always)]
    pub fn iter_mut(&mut self) -> IterMut<'_, O> {
        self.orders.iter_mut()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::order::TestOrder;

    #[test]
    fn test_new_level() {
        let level = Level::<TestOrder>::new(100);
        assert_eq!(level.price(), 100);
        assert_eq!(level.total_quantity(), 0);
        assert_eq!(level.len(), 0);
        assert!(level.is_empty());
    }

    #[test]
    fn test_add_order() {
        let mut level = Level::<TestOrder>::new(100);
        level.add_order(TestOrder::new("1", true, 100, 50));
        assert_eq!(level.total_quantity(), 50);
        assert_eq!(level.len(), 1);
        assert!(!level.is_empty());
    }

    #[test]
    fn test_add_multiple_orders() {
        let mut level = Level::<TestOrder>::new(100);
        level.add_order(TestOrder::new("1", true, 100, 50));
        level.add_order(TestOrder::new("2", true, 100, 30));
        level.add_order(TestOrder::new("3", true, 100, 20));
        assert_eq!(level.total_quantity(), 100);
        assert_eq!(level.len(), 3);
    }

    #[test]
    fn test_remove_order() {
        let mut level = Level::<TestOrder>::new(100);
        level.add_order(TestOrder::new("1", true, 100, 50));
        let node_ptr = level.add_order(TestOrder::new("2", true, 100, 30));
        level.add_order(TestOrder::new("3", true, 100, 20));
        level.remove_order(node_ptr);
        assert_eq!(level.total_quantity(), 70);
        assert_eq!(level.len(), 2);
    }

    #[test]
    fn test_remove_nonexistent_order() {
        let mut level = Level::<TestOrder>::new(100);
        level.add_order(TestOrder::new("1", true, 50, 50));
        level.remove_order(std::ptr::null_mut());
        assert_eq!(level.total_quantity(), 50);
        assert_eq!(level.len(), 1);
    }

    #[test]
    fn test_iter() {
        let mut level = Level::<TestOrder>::new(100);
        level.add_order(TestOrder::new("1", true, 100, 50));
        level.add_order(TestOrder::new("2", true, 100, 30));
        level.add_order(TestOrder::new("3", true, 100, 20));

        let ids: Vec<&String> = level.iter().map(|o| o.id()).collect();
        assert_eq!(ids, vec!["1", "2", "3"]);
    }

    #[test]
    fn test_iter_mut() {
        let mut level = Level::<TestOrder>::new(100);
        level.add_order(TestOrder::new("1", true, 100, 50));
        level.add_order(TestOrder::new("2", true, 100, 30));

        let count = level.iter_mut().count();
        assert_eq!(count, 2);
    }

    #[test]
    fn test_fill_order_partial() {
        let mut level = Level::<TestOrder>::new(100);
        let node_ptr = level.add_order(TestOrder::new("1", true, 100, 100));
        let order = unsafe { &mut (*node_ptr).data };
        let removed = level.fill_order(node_ptr, order, 30);
        assert!(!removed);
        assert_eq!(level.total_quantity(), 70);
        assert_eq!(level.len(), 1);
    }

    #[test]
    fn test_fill_order_complete() {
        let mut level = Level::<TestOrder>::new(100);
        let node_ptr = level.add_order(TestOrder::new("1", true, 100, 100));
        let order = unsafe { &mut (*node_ptr).data };
        let removed = level.fill_order(node_ptr, order, 100);
        assert!(removed);
        assert_eq!(level.total_quantity(), 0);
        assert_eq!(level.len(), 0);
        assert!(level.is_empty());
    }
}
