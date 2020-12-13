use std::{
    ops::{
        Deref,
        DerefMut,
    },
};

use super::{
    pool,
    Shared,
    Unique,
};

pub type Bytes = Shared<Vec<u8>>;
type BytesMutInner = Unique<Vec<u8>>;

#[derive(PartialEq, Hash, Debug)]
pub struct BytesMut {
    unique: BytesMutInner,
}

impl BytesMut {
    pub fn freeze(mut self) -> Bytes {
        self.unique.shrink_to_fit();
        self.unique.freeze()
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
