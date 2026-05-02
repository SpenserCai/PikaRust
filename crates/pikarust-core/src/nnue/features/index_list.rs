pub struct IndexList {
    values: [u32; 64],
    size: usize,
}

impl IndexList {
    pub const fn new() -> Self {
        Self {
            values: [0; 64],
            size: 0,
        }
    }

    #[inline]
    pub fn push(&mut self, index: u32) {
        debug_assert!(self.size < 64);
        self.values[self.size] = index;
        self.size += 1;
    }

    #[inline]
    pub fn push_if_lt(&mut self, index: u32, limit: u32) {
        if index < limit {
            self.push(index);
        }
    }

    #[inline]
    pub const fn len(&self) -> usize {
        self.size
    }

    #[inline]
    pub const fn is_empty(&self) -> bool {
        self.size == 0
    }

    #[inline]
    pub const fn clear(&mut self) {
        self.size = 0;
    }

    #[inline]
    pub fn as_slice(&self) -> &[u32] {
        &self.values[..self.size]
    }
}

impl Default for IndexList {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_index_list_basic() {
        let mut list = IndexList::new();
        assert!(list.is_empty());
        assert_eq!(list.len(), 0);

        list.push(42);
        assert_eq!(list.len(), 1);
        assert_eq!(list.as_slice(), &[42]);

        list.push(100);
        assert_eq!(list.len(), 2);
        assert_eq!(list.as_slice(), &[42, 100]);
    }

    #[test]
    fn test_index_list_push_if_lt() {
        let mut list = IndexList::new();
        list.push_if_lt(10, 100);
        assert_eq!(list.len(), 1);

        list.push_if_lt(200, 100);
        assert_eq!(list.len(), 1);

        list.push_if_lt(99, 100);
        assert_eq!(list.len(), 2);
    }

    #[test]
    fn test_index_list_clear() {
        let mut list = IndexList::new();
        list.push(1);
        list.push(2);
        list.clear();
        assert!(list.is_empty());
    }
}
