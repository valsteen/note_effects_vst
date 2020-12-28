use serde::{Deserialize, Serialize};

use crate::midi_message_with_delta::MidiMessageWithDelta;

#[derive(Debug, Serialize, Deserialize)]
pub struct PatternPayload {
    pub time: usize,
    pub messages: Vec<MidiMessageWithDelta>,
}
