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
    fn new(data: T) -> Box<Self> {
        Box::new(Node {
            data,
            prev: ptr::null_mut(),
            next: ptr::null_mut(),
        })
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
    pub fn push_back(&mut self, data: T) -> *mut Node<T> {
        let new_node = Box::into_raw(Node::new(data));
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
    pub fn remove_unchecked(&mut self, node_ptr: *mut Node<T>) -> T {
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
            Box::from_raw(node_ptr).data
        }
    }

    /// Removes node at pointer. Caller must ensure pointer is valid and in this list.
    #[inline(always)]
    #[allow(clippy::not_unsafe_ptr_arg_deref)]
    pub fn remove(&mut self, node_ptr: *mut Node<T>) -> Option<T> {
        if node_ptr.is_null() || self.length == 0 {
            return None;
        }
        Some(self.remove_unchecked(node_ptr))
    }
}

impl<T> Default for List<T> {
    fn default() -> Self {
        Self::new()
    }
}

impl<T> Drop for List<T> {
    fn drop(&mut self) {
        while self.pop_front().is_some() {}
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
        list.push_back(1);
        list.push_back(2);
        list.push_back(3);
        assert_eq!(list.len(), 3);
        assert_eq!(list.iter().last().unwrap(), &3);
    }

    #[test]
    fn test_pop_front() {
        let mut list = List::new();
        list.push_back(1);
        list.push_back(2);
        list.push_back(3);

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
        list.push_back(1);
        list.push_back(2);
        list.push_back(3);
        let vec: Vec<i32> = list.into_iter().collect();
        assert_eq!(vec, vec![1, 2, 3]);
    }

    #[test]
    fn test_drop() {
        let mut list = List::new();
        for i in 0..100 {
            list.push_back(i);
        }
    }

    #[test]
    fn test_iter() {
        let mut list = List::new();
        list.push_back(1);
        list.push_back(2);
        list.push_back(3);
        let vec: Vec<&i32> = list.iter().collect();
        assert_eq!(vec, vec![&1, &2, &3]);
        assert_eq!(list.len(), 3);
    }

    #[test]
    fn test_iter_mut() {
        let mut list = List::new();
        list.push_back(1);
        list.push_back(2);
        list.push_back(3);
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
        list.push_back(1);
        let result = list.remove(std::ptr::null_mut());
        assert_eq!(result, None);
        assert_eq!(list.len(), 1);
    }

    #[test]
    fn test_remove_head() {
        let mut list = List::new();
        let node1 = list.push_back(1);
        list.push_back(2);
        list.push_back(3);
        let removed = list.remove(node1);
        assert_eq!(removed, Some(1));
        assert_eq!(list.len(), 2);
        let vec: Vec<&i32> = list.iter().collect();
        assert_eq!(vec, vec![&2, &3]);
    }

    #[test]
    fn test_remove_tail() {
        let mut list = List::new();
        list.push_back(1);
        list.push_back(2);
        let node3 = list.push_back(3);
        let removed = list.remove(node3);
        assert_eq!(removed, Some(3));
        assert_eq!(list.len(), 2);
        let vec: Vec<&i32> = list.iter().collect();
        assert_eq!(vec, vec![&1, &2]);
    }

    #[test]
    fn test_remove_middle() {
        let mut list = List::new();
        list.push_back(1);
        let node2 = list.push_back(2);
        list.push_back(3);
        let removed = list.remove(node2);
        assert_eq!(removed, Some(2));
        assert_eq!(list.len(), 2);
        let vec: Vec<&i32> = list.iter().collect();
        assert_eq!(vec, vec![&1, &3]);
    }

    #[test]
    fn test_remove_only_node() {
        let mut list = List::new();
        let node1 = list.push_back(1);
        let removed = list.remove(node1);
        assert_eq!(removed, Some(1));
        assert_eq!(list.len(), 0);
        assert!(list.is_empty());
        let vec: Vec<&i32> = list.iter().collect();
        assert_eq!(vec, Vec::<&i32>::new());
    }

    #[test]
    fn test_remove_multiple_nodes() {
        let mut list = List::new();
        let node1 = list.push_back(1);
        let node2 = list.push_back(2);
        let node3 = list.push_back(3);
        let node4 = list.push_back(4);
        let node5 = list.push_back(5);

        let removed = list.remove(node3);
        assert_eq!(removed, Some(3));
        assert_eq!(list.len(), 4);
        let vec: Vec<&i32> = list.iter().collect();
        assert_eq!(vec, vec![&1, &2, &4, &5]);

        let removed = list.remove(node5);
        assert_eq!(removed, Some(5));
        assert_eq!(list.len(), 3);
        let vec: Vec<&i32> = list.iter().collect();
        assert_eq!(vec, vec![&1, &2, &4]);

        let removed = list.remove(node1);
        assert_eq!(removed, Some(1));
        assert_eq!(list.len(), 2);
        let vec: Vec<&i32> = list.iter().collect();
        assert_eq!(vec, vec![&2, &4]);

        let removed = list.remove(node2);
        assert_eq!(removed, Some(2));
        assert_eq!(list.len(), 1);

        let removed = list.remove(node4);
        assert_eq!(removed, Some(4));
        assert_eq!(list.len(), 0);
        assert!(list.is_empty());
    }
}
