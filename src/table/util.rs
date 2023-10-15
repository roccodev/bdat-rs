pub enum VersionedIter<M, L> {
    Modern(M),
    Legacy(L),
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
