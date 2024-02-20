use std::ops::AddAssign;

pub enum VersionedIter<M, L> {
    Modern(M),
    Legacy(L),
}

pub struct RowIdIter<I, N> {
    iter: I,
    id: N
}

pub(crate) trait EnumId where Self: Sized {
    fn enum_id<N>(self, base_id: N) -> RowIdIter<Self, N>;
}

impl<I, N, It> Iterator for RowIdIter<I, N>
where
    I: Iterator<Item = It>,
    N: Copy + AddAssign + From<u8>
{
    type Item = (N, It);

    fn next(&mut self) -> Option<Self::Item> {
        let item = self.iter.next()?;
        let id = self.id;
        self.id += N::from(1);
        Some((id, item))
    }
}

impl<M, L, I> Iterator for VersionedIter<M, L>
where
    M: Iterator<Item = I>,
    L: Iterator<Item = I>,
{
    type Item = I;

    fn next(&mut self) -> Option<Self::Item> {
        match self {
            VersionedIter::Modern(m) => m.next(),
            VersionedIter::Legacy(l) => l.next(),
        }
    }
}

impl<I, It> EnumId for I
where
    I: Iterator<Item = It>
{
    fn enum_id<N>(self, base_id: N) -> RowIdIter<Self, N> {
        RowIdIter { iter: self, id: base_id }
    }
}
