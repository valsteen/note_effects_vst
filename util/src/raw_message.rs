use core::clone::Clone;
use core::convert::{From, Into};
use core::ops::Index;

#[derive(Copy)]
pub struct RawMessage([u8; 3]);


impl Clone for RawMessage {
    fn clone(&self) -> RawMessage {
        RawMessage(self.0)
    }
}

impl From<[u8;3]> for RawMessage {
    fn from(e: [u8; 3]) -> Self {
        RawMessage(e)
    }
}

impl Into<[u8;3]> for RawMessage {
    fn into(self) -> [u8; 3] {
        self.0
    }
}

impl Index<usize> for RawMessage {
    type Output = u8;

    fn index(&self, index: usize) -> &Self::Output {
        &self.0[index]
    }
}
