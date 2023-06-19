use std::ops::Index;

pub struct FixedVec<T, const MAX: usize> {
    buf: [T; MAX],
    len: usize,
}

impl<T, const MAX: usize> FixedVec<T, MAX> {
    pub fn new() -> Self
    where
        T: Default + Copy,
    {
        Self {
            buf: [Default::default(); MAX],
            len: 0,
        }
    }

    pub fn try_push(&mut self, item: T) -> Result<(), usize> {
        self.check_len()?;
        self.buf[self.len] = item;
        self.len += 1;
        Ok(())
    }

    #[inline(always)]
    fn check_len(&self) -> Result<(), usize> {
        if self.len >= MAX {
            Err(self.len)
        } else {
            Ok(())
        }
    }
}

impl<T: Default + Copy, const MAX: usize> Default for FixedVec<T, MAX> {
    fn default() -> Self {
        Self::new()
    }
}

impl<T, const MAX: usize> Index<usize> for FixedVec<T, MAX> {
    type Output = T;

    fn index(&self, index: usize) -> &Self::Output {
        &self.buf[index]
    }
}

impl<T, const MAX: usize> IntoIterator for FixedVec<T, MAX> {
    type Item = T;
    type IntoIter = std::array::IntoIter<T, MAX>;

    fn into_iter(self) -> Self::IntoIter {
        self.buf.into_iter()
    }
}

impl<'a, T, const MAX: usize> IntoIterator for &'a FixedVec<T, MAX> {
    type Item = &'a T;
    type IntoIter = std::slice::Iter<'a, T>;

    fn into_iter(self) -> Self::IntoIter {
        self.buf.iter()
    }
}
