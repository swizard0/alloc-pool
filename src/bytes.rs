use std::{
    ops::{
        Deref,
        DerefMut,
    },
    hash::{
        Hash,
        Hasher,
    },
    ops::{
        Bound,
        RangeBounds,
    },
};

use super::{
    pool,
    Shared,
    Unique,
};

type BytesInner = Shared<Vec<u8>>;
type BytesMutInner = Unique<Vec<u8>>;

#[derive(PartialEq, Hash, Debug)]
pub struct BytesMut {
    unique: BytesMutInner,
}

impl BytesMut {
    pub fn new_detached(value: Vec<u8>) -> Self {
        Self { unique: BytesMutInner::new_detached(value), }
    }

    pub fn freeze(mut self) -> Bytes {
        self.unique.shrink_to_fit();
        let inner = self.unique.freeze();
        let offset_to = inner.len();
        Bytes { inner, offset_from: 0, offset_to, }
    }

    pub fn freeze_range<R>(self, range: R) -> Bytes where R: RangeBounds<usize> {
        let mut bytes = self.freeze();
        match range.start_bound() {
            Bound::Unbounded =>
                (),
            Bound::Included(&offset) if offset <= bytes.offset_to =>
                bytes.offset_from = offset,
            Bound::Included(offset) =>
                panic!("BytesMut::freeze_range start offset = {} greater than slice length {}", offset, bytes.offset_to),
            Bound::Excluded(..) =>
                unreachable!(),
        }
        match range.end_bound() {
            Bound::Unbounded =>
                (),
            Bound::Included(&offset) if offset < bytes.offset_to =>
                bytes.offset_to = offset + 1,
            Bound::Included(offset) =>
                panic!(
                    "BytesMut::freeze_range included end offset = {} greater or equal than slice length {}",
                    offset,
                    bytes.offset_to,
                ),
            Bound::Excluded(&offset) if offset <= bytes.offset_to =>
                bytes.offset_to = offset,
            Bound::Excluded(offset) =>
                panic!(
                    "BytesMut::freeze_range excluded end offset = {} greater than slice length {}",
                    offset,
                    bytes.offset_to,
                ),
        }
        bytes
    }
}

impl AsRef<BytesMutInner> for BytesMut {
    #[inline]
    fn as_ref(&self) -> &BytesMutInner {
        &self.unique
    }
}

impl AsRef<Vec<u8>> for BytesMut {
    #[inline]
    fn as_ref(&self) -> &Vec<u8> {
        self.unique.as_ref()
    }
}

impl Deref for BytesMut {
    type Target = BytesMutInner;

    #[inline]
    fn deref(&self) -> &BytesMutInner {
        self.as_ref()
    }
}

impl AsMut<BytesMutInner> for BytesMut {
    #[inline]
    fn as_mut(&mut self) -> &mut BytesMutInner {
        &mut self.unique
    }
}

impl AsMut<Vec<u8>> for BytesMut {
    #[inline]
    fn as_mut(&mut self) -> &mut Vec<u8> {
        self.unique.as_mut()
    }
}

impl DerefMut for BytesMut {
    #[inline]
    fn deref_mut(&mut self) -> &mut BytesMutInner {
        self.as_mut()
    }
}

#[derive(Clone, Debug)]
pub struct Bytes {
    inner: BytesInner,
    offset_from: usize,
    offset_to: usize,
}

impl AsRef<[u8]> for Bytes {
    #[inline]
    fn as_ref(&self) -> &[u8] {
        &self.inner[self.offset_from .. self.offset_to]
    }
}

impl Deref for Bytes {
    type Target = [u8];

    #[inline]
    fn deref(&self) -> &[u8] {
        self.as_ref()
    }
}

impl PartialEq for Bytes {
    fn eq(&self, other: &Bytes) -> bool {
        self.as_ref() == other.as_ref()
    }
}

impl Eq for Bytes { }

impl Hash for Bytes {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.as_ref().hash(state);
    }
}


#[derive(Clone, Debug)]
pub struct BytesPool {
    pool: pool::Pool<Vec<u8>>,
}

impl BytesPool {
    pub fn new() -> BytesPool {
        BytesPool { pool: pool::Pool::new(), }
    }

    pub fn lend(&self) -> BytesMut {
        let mut bytes = self.pool.lend(Vec::new);
        bytes.clear();
        BytesMut { unique: bytes, }
    }
}

#[cfg(test)]
mod tests {
    use super::{
        BytesMut,
    };

    #[test]
    fn freeze_00() {
        let bytes = BytesMut::new_detached(vec![0, 1, 2, 3, 4])
            .freeze();
        assert_eq!(&*bytes, &[0, 1, 2, 3, 4]);
    }

    #[test]
    fn freeze_range_00() {
        let bytes = BytesMut::new_detached(vec![0, 1, 2, 3, 4])
            .freeze_range(.. 3);
        assert_eq!(&*bytes, &[0, 1, 2]);
    }

    #[test]
    fn freeze_range_01() {
        let bytes = BytesMut::new_detached(vec![0, 1, 2, 3, 4])
            .freeze_range(..= 3);
        assert_eq!(&*bytes, &[0, 1, 2, 3]);
    }

    #[test]
    fn freeze_range_02() {
        let bytes = BytesMut::new_detached(vec![0, 1, 2, 3, 4])
            .freeze_range(2 ..);
        assert_eq!(&*bytes, &[2, 3, 4]);
    }

    #[test]
    fn freeze_range_03() {
        let bytes = BytesMut::new_detached(vec![0, 1, 2, 3, 4])
            .freeze_range(2 .. 4);
        assert_eq!(&*bytes, &[2, 3]);
    }

    #[test]
    #[should_panic]
    fn freeze_range_04() {
        let _bytes = BytesMut::new_detached(vec![0, 1, 2, 3, 4])
            .freeze_range(2 ..= 5);
    }

    #[test]
    #[should_panic]
    fn freeze_range_05() {
        let _bytes = BytesMut::new_detached(vec![0, 1, 2, 3, 4])
            .freeze_range(.. 6);
    }
}
