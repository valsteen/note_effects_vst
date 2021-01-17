use super::messages::ChannelMessage;
use crate::constants::PRESSURE;
use core::clone::Clone;
use core::convert::{From, Into};
use core::ops::Index;
use serde::{Deserialize, Serialize};
use std::cmp::Ordering;

#[derive(Copy, Debug, Serialize, Deserialize)]
pub struct RawMessage([u8; 3]);


impl RawMessage {
    #[inline]
    pub fn get_bytes(&self) -> &[u8] {
        // RawMessage is 3 bytes long to keep moving around a known-size message, but some actually are 2 bytes long
        &self.0

        // bitwig crashes when receiving pressure messages from a VST ; commented out as it might help, still unlikely
        // if self.0[0] & 0xF0 == PRESSURE {
        //     &self.0[..2]
        // } else {
        //     &self.0
        // }
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
        // attempt of stopping crash at pressure
        // TODO log what is output
        if self.0[0] & 0xF0 == PRESSURE {
            [self.0[0],self.0[1],0]
        } else {
            self.0
        }
    }
}

impl Index<usize> for RawMessage {
    type Output = u8;

    fn index(&self, index: usize) -> &Self::Output {
        &self.0[index]
    }
}


impl PartialOrd for RawMessage {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        let cmp_0 = self.0[0].cmp(&other.0[0]);
        match cmp_0 {
            Ordering::Equal => {
                Some(self.0[1].cmp(&other.0[1]))
            }
            _ => Some(cmp_0)
        }
    }
}

impl PartialEq for RawMessage {
    fn eq(&self, other: &Self) -> bool {
        self.0[0] == other.0[0] && self.0[1] == other.0[1]
    }
}

impl Ord for RawMessage {
    fn cmp(&self, other: &Self) -> Ordering {
        self.partial_cmp(other).unwrap()
    }
}


impl Eq for RawMessage {}
