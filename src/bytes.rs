
use super::{
    pool,
    Shared,
    Unique,
};

pub type Bytes = Shared<Vec<u8>>;
pub type BytesMut = Unique<Vec<u8>>;

#[derive(Clone, Debug)]
pub struct BytesPool {
    pool: pool::Pool<Vec<u8>>,
}

impl BytesPool {
    pub fn new() -> BytesPool {
        BytesPool { pool: pool::Pool::new(), }
    }

    pub fn lend(&self) -> BytesMut {
        self.pool.lend(Vec::new)
    }
}
