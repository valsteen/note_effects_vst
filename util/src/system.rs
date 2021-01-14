use serde::{Deserialize, Serialize};
use std::fmt;
use std::fmt::{Debug, Display};

// generate unique IDs on top of Uuid, which does not implement Serialize/Deserialize

#[derive(Copy, Clone, Debug, Serialize, Deserialize)]
pub struct Uuid(u128);

impl Default for Uuid {
    fn default() -> Self {
        Self {
            0: uuid::Uuid::default().to_u128_le(),
        }
    }
}

impl Uuid {
    pub fn new_v4() -> Self {
        Self {
            0: uuid::Uuid::new_v4().to_u128_le(),
        }
    }
}

impl Display for Uuid {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        std::fmt::Display::fmt(&uuid::Uuid::from_u128_le(self.0), f)
    }
}
