#[inline]
pub fn pad_2(len: usize) -> usize {
    len + ((2 - (len & 1)) & 1)
}

#[inline]
pub fn pad_4(len: usize) -> usize {
    len + ((4 - (len & 3)) & 3)
}

#[inline]
pub fn pad_8(len: usize) -> usize {
    len + ((8 - (len & 7)) & 7)
}

#[inline]
pub fn pad_32(len: usize) -> usize {
    len + ((32 - (len & 31)) & 31)
}

#[inline]
pub fn pad_64(len: usize) -> usize {
    len + ((64 - (len & 63)) & 63)
}
