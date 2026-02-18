use std::ptr;

/// A node in the doubly linked list.
#[repr(C)]
pub struct Node<T> {
    pub data: T,
    pub prev: *mut Node<T>,
    pub next: *mut Node<T>,
}

impl<T> Node<T> {
    #[inline(always)]
    fn new(data: T) -> Self {
        Node {
            data,
            prev: ptr::null_mut(),
            next: ptr::null_mut(),
        }
    }
}

/// Intrusive free-list pool for reusing Node allocations.
/// Freed nodes are linked through their `next` pointer, so push/pop is O(1)
/// with zero auxiliary allocations.
pub struct Pool<T> {
    free: *mut Node<T>,
}

impl<T> Pool<T> {
    #[inline]
    pub fn new() -> Self {
        Pool {
            free: ptr::null_mut(),
        }
    }

    #[inline(always)]
    pub fn alloc(&mut self, data: T) -> *mut Node<T> {
        let ptr = self.free;
        if !ptr.is_null() {
            unsafe {
                self.free = (*ptr).next;
                ptr::write(ptr, Node::new(data));
            }
            ptr
        } else {
            Box::into_raw(Box::new(Node::new(data)))
        }
    }

    #[inline(always)]
    pub fn dealloc(&mut self, ptr: *mut Node<T>) -> T {
        unsafe {
            let data = ptr::read(&(*ptr).data);
            (*ptr).next = self.free;
            self.free = ptr;
            data
        }
    }
}

impl<T> Default for Pool<T> {
    fn default() -> Self {
        Self::new()
    }
}

impl<T> Drop for Pool<T> {
    fn drop(&mut self) {
        let mut current = self.free;
        while !current.is_null() {
            unsafe {
                let next = (*current).next;
                std::alloc::dealloc(current as *mut u8, std::alloc::Layout::new::<Node<T>>());
                current = next;
            }
        }
    }
}

/// A doubly linked list using unsafe raw pointers.
pub struct List<T> {
    head: *mut Node<T>,
    tail: *mut Node<T>,
    length: usize,
}

impl<T> List<T> {
    #[inline(always)]
    pub fn new() -> Self {
        List {
            head: ptr::null_mut(),
            tail: ptr::null_mut(),
            length: 0,
        }
    }

    #[inline(always)]
    pub fn len(&self) -> usize {
        self.length
    }

    #[inline(always)]
    pub fn is_empty(&self) -> bool {
        self.length == 0
    }

    /// Returns pointer to the newly inserted node.
    #[inline(always)]
    pub fn push_back(&mut self, data: T, pool: &mut Pool<T>) -> *mut Node<T> {
        let new_node = pool.alloc(data);
        unsafe {
            if self.tail.is_null() {
                self.head = new_node;
            } else {
                (*self.tail).next = new_node;
                (*new_node).prev = self.tail;
            }
            self.tail = new_node;
        }
        self.length += 1;
        new_node
    }

    #[inline(always)]
    pub fn pop_front(&mut self) -> Option<*mut Node<T>> {
        if self.head.is_null() {
            return None;
        }
        unsafe {
            let old_head = self.head;
            self.head = (*old_head).next;
            if self.head.is_null() {
                self.tail = ptr::null_mut();
            } else {
                (*self.head).prev = ptr::null_mut();
            }
            self.length -= 1;
            Some(old_head)
        }
    }

    /// Removes node at pointer. Caller must ensure pointer is valid and in this list.
    /// Use this when the pointer is known to be non-null and in the list (e.g. from `push_back`).
    #[inline(always)]
    #[allow(clippy::not_unsafe_ptr_arg_deref)]
    pub fn remove_unchecked(&mut self, node_ptr: *mut Node<T>, pool: &mut Pool<T>) -> T {
        unsafe {
            let prev = (*node_ptr).prev;
            let next = (*node_ptr).next;
            if prev.is_null() {
                self.head = next;
            } else {
                (*prev).next = next;
            }
            if next.is_null() {
                self.tail = prev;
            } else {
                (*next).prev = prev;
            }
            self.length -= 1;
            pool.dealloc(node_ptr)
        }
    }

    /// Removes node at pointer. Caller must ensure pointer is valid and in this list.
    #[inline(always)]
    #[allow(clippy::not_unsafe_ptr_arg_deref)]
    pub fn remove(&mut self, node_ptr: *mut Node<T>, pool: &mut Pool<T>) -> Option<T> {
        if node_ptr.is_null() || self.length == 0 {
            return None;
        }
        Some(self.remove_unchecked(node_ptr, pool))
    }
}

impl<T> Default for List<T> {
    fn default() -> Self {
        Self::new()
    }
}

impl<T> Drop for List<T> {
    fn drop(&mut self) {
        let mut current = self.head;
        while !current.is_null() {
            unsafe {
                let next = (*current).next;
                let _ = Box::from_raw(current);
                current = next;
            }
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Iterators
// ─────────────────────────────────────────────────────────────────────────────

pub struct IntoIter<T>(List<T>);

impl<T> Iterator for IntoIter<T> {
    type Item = T;

    fn next(&mut self) -> Option<Self::Item> {
        self.0
            .pop_front()
            .map(|node_ptr| unsafe { Box::from_raw(node_ptr).data })
    }
}

pub struct Iter<'a, T> {
    current: *mut Node<T>,
    _marker: std::marker::PhantomData<&'a T>,
}

impl<'a, T> Iterator for Iter<'a, T> {
    type Item = &'a T;

    #[inline(always)]
    fn next(&mut self) -> Option<Self::Item> {
        if self.current.is_null() {
            return None;
        }
        unsafe {
            let data = &(*self.current).data;
            self.current = (*self.current).next;
            Some(data)
        }
    }
}

pub struct IterMut<'a, T> {
    current: *mut Node<T>,
    _marker: std::marker::PhantomData<&'a mut T>,
}

impl<'a, T> Iterator for IterMut<'a, T> {
    type Item = &'a mut T;

    #[inline(always)]
    fn next(&mut self) -> Option<Self::Item> {
        if self.current.is_null() {
            return None;
        }
        unsafe {
            let data = &mut (*self.current).data;
            let next = (*self.current).next;
            self.current = next;
            Some(data)
        }
    }
}

impl<T> IntoIterator for List<T> {
    type Item = T;
    type IntoIter = IntoIter<T>;

    fn into_iter(self) -> Self::IntoIter {
        IntoIter(self)
    }
}

impl<T> List<T> {
    #[inline]
    pub fn iter(&self) -> Iter<'_, T> {
        Iter {
            current: self.head,
            _marker: std::marker::PhantomData,
        }
    }

    #[inline]
    pub fn iter_mut(&mut self) -> IterMut<'_, T> {
        IterMut {
            current: self.head,
            _marker: std::marker::PhantomData,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new() {
        let list: List<i32> = List::new();
        assert!(list.is_empty());
        assert_eq!(list.len(), 0);
    }

    #[test]
    fn test_default() {
        let list: List<i32> = List::default();
        assert!(list.is_empty());
        assert_eq!(list.len(), 0);
    }

    #[test]
    fn test_push_back() {
        let mut list = List::new();
        let mut pool = Pool::new();
        list.push_back(1, &mut pool);
        list.push_back(2, &mut pool);
        list.push_back(3, &mut pool);
        assert_eq!(list.len(), 3);
        assert_eq!(list.iter().last().unwrap(), &3);
    }

    #[test]
    fn test_pop_front() {
        let mut list = List::new();
        let mut pool = Pool::new();
        list.push_back(1, &mut pool);
        list.push_back(2, &mut pool);
        list.push_back(3, &mut pool);

        let node1 = list.pop_front().unwrap();
        assert_eq!(unsafe { (*node1).data }, 1);
        unsafe {
            let _ = Box::from_raw(node1);
        }

        let node2 = list.pop_front().unwrap();
        assert_eq!(unsafe { (*node2).data }, 2);
        unsafe {
            let _ = Box::from_raw(node2);
        }

        let node3 = list.pop_front().unwrap();
        assert_eq!(unsafe { (*node3).data }, 3);
        unsafe {
            let _ = Box::from_raw(node3);
        }

        assert_eq!(list.pop_front(), None);
        assert!(list.is_empty());
        assert_eq!(list.len(), 0);
    }

    #[test]
    fn test_into_iter() {
        let mut list = List::new();
        let mut pool = Pool::new();
        list.push_back(1, &mut pool);
        list.push_back(2, &mut pool);
        list.push_back(3, &mut pool);
        let vec: Vec<i32> = list.into_iter().collect();
        assert_eq!(vec, vec![1, 2, 3]);
    }

    #[test]
    fn test_drop() {
        let mut list = List::new();
        let mut pool = Pool::new();
        for i in 0..100 {
            list.push_back(i, &mut pool);
        }
    }

    #[test]
    fn test_iter() {
        let mut list = List::new();
        let mut pool = Pool::new();
        list.push_back(1, &mut pool);
        list.push_back(2, &mut pool);
        list.push_back(3, &mut pool);
        let vec: Vec<&i32> = list.iter().collect();
        assert_eq!(vec, vec![&1, &2, &3]);
        assert_eq!(list.len(), 3);
    }

    #[test]
    fn test_iter_mut() {
        let mut list = List::new();
        let mut pool = Pool::new();
        list.push_back(1, &mut pool);
        list.push_back(2, &mut pool);
        list.push_back(3, &mut pool);
        for item in list.iter_mut() {
            *item *= 2;
        }
        let vec: Vec<&i32> = list.iter().collect();
        assert_eq!(vec, vec![&2, &4, &6]);
        assert_eq!(list.len(), 3);
    }

    #[test]
    fn test_remove_null_pointer() {
        let mut list = List::new();
        let mut pool = Pool::new();
        list.push_back(1, &mut pool);
        let result = list.remove(std::ptr::null_mut(), &mut pool);
        assert_eq!(result, None);
        assert_eq!(list.len(), 1);
    }

    #[test]
    fn test_remove_head() {
        let mut list = List::new();
        let mut pool = Pool::new();
        let node1 = list.push_back(1, &mut pool);
        list.push_back(2, &mut pool);
        list.push_back(3, &mut pool);
        let removed = list.remove(node1, &mut pool);
        assert_eq!(removed, Some(1));
        assert_eq!(list.len(), 2);
        let vec: Vec<&i32> = list.iter().collect();
        assert_eq!(vec, vec![&2, &3]);
    }

    #[test]
    fn test_remove_tail() {
        let mut list = List::new();
        let mut pool = Pool::new();
        list.push_back(1, &mut pool);
        list.push_back(2, &mut pool);
        let node3 = list.push_back(3, &mut pool);
        let removed = list.remove(node3, &mut pool);
        assert_eq!(removed, Some(3));
        assert_eq!(list.len(), 2);
        let vec: Vec<&i32> = list.iter().collect();
        assert_eq!(vec, vec![&1, &2]);
    }

    #[test]
    fn test_remove_middle() {
        let mut list = List::new();
        let mut pool = Pool::new();
        list.push_back(1, &mut pool);
        let node2 = list.push_back(2, &mut pool);
        list.push_back(3, &mut pool);
        let removed = list.remove(node2, &mut pool);
        assert_eq!(removed, Some(2));
        assert_eq!(list.len(), 2);
        let vec: Vec<&i32> = list.iter().collect();
        assert_eq!(vec, vec![&1, &3]);
    }

    #[test]
    fn test_remove_only_node() {
        let mut list = List::new();
        let mut pool = Pool::new();
        let node1 = list.push_back(1, &mut pool);
        let removed = list.remove(node1, &mut pool);
        assert_eq!(removed, Some(1));
        assert_eq!(list.len(), 0);
        assert!(list.is_empty());
        let vec: Vec<&i32> = list.iter().collect();
        assert_eq!(vec, Vec::<&i32>::new());
    }

    #[test]
    fn test_remove_multiple_nodes() {
        let mut list = List::new();
        let mut pool = Pool::new();
        let node1 = list.push_back(1, &mut pool);
        let node2 = list.push_back(2, &mut pool);
        let node3 = list.push_back(3, &mut pool);
        let node4 = list.push_back(4, &mut pool);
        let node5 = list.push_back(5, &mut pool);

        let removed = list.remove(node3, &mut pool);
        assert_eq!(removed, Some(3));
        assert_eq!(list.len(), 4);
        let vec: Vec<&i32> = list.iter().collect();
        assert_eq!(vec, vec![&1, &2, &4, &5]);

        let removed = list.remove(node5, &mut pool);
        assert_eq!(removed, Some(5));
        assert_eq!(list.len(), 3);
        let vec: Vec<&i32> = list.iter().collect();
        assert_eq!(vec, vec![&1, &2, &4]);

        let removed = list.remove(node1, &mut pool);
        assert_eq!(removed, Some(1));
        assert_eq!(list.len(), 2);
        let vec: Vec<&i32> = list.iter().collect();
        assert_eq!(vec, vec![&2, &4]);

        let removed = list.remove(node2, &mut pool);
        assert_eq!(removed, Some(2));
        assert_eq!(list.len(), 1);

        let removed = list.remove(node4, &mut pool);
        assert_eq!(removed, Some(4));
        assert_eq!(list.len(), 0);
        assert!(list.is_empty());
    }

    #[test]
    fn test_pool_reuse() {
        let mut pool = Pool::new();
        let mut list = List::new();

        let p1 = list.push_back(1, &mut pool);
        let p2 = list.push_back(2, &mut pool);

        list.remove(p1, &mut pool);
        list.remove(p2, &mut pool);
        assert!(!pool.free.is_null());

        // Pool reuses freed nodes for new allocations
        let _p3 = list.push_back(3, &mut pool);
        let _p4 = list.push_back(4, &mut pool);
        assert!(pool.free.is_null());

        assert_eq!(list.len(), 2);
        let vec: Vec<&i32> = list.iter().collect();
        assert_eq!(vec, vec![&3, &4]);
    }
}
