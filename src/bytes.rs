
use super::{
    Shared,
    Unique,
};

pub type Bytes = Shared<Vec<u8>>;
pub type BytesMut = Unique<Vec<u8>>;
