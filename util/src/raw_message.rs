use super::messages::ChannelMessage;
use crate::constants::PRESSURE;
use core::clone::Clone;
use core::convert::{From, Into};
use core::ops::Index;
use serde::{Deserialize, Serialize};

#[derive(Copy, Debug, Serialize, Deserialize)]
pub struct RawMessage([u8; 3]);

impl RawMessage {
    pub fn get_bytes(&self) -> &[u8] {
        // RawMessage is 3 bytes long to keep moving around a known-size message, but some actually are 2 bytes long
        if self.0[0] & 0xF0 == PRESSURE {
            &self.0[..2]
        } else {
            &self.0
        }
    }
}

impl ChannelMessage for RawMessage {
    fn get_channel(&self) -> u8 {
        self.0[0] & 0x0F
    }
}

impl Clone for RawMessage {
    fn clone(&self) -> RawMessage {
        RawMessage(self.0)
    }
}

impl From<[u8; 3]> for RawMessage {
    fn from(e: [u8; 3]) -> Self {
        RawMessage(e)
    }
}

impl Into<[u8; 3]> for RawMessage {
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
