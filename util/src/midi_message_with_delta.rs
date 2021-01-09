use serde::{Serialize, Deserialize};
use vst::event::MidiEvent;
use crate::raw_message::RawMessage;


#[derive(Copy, Clone, Debug, Serialize, Deserialize)]
pub struct MidiMessageWithDelta {
    pub delta_frames: u16,
    pub data: RawMessage,
}


impl MidiMessageWithDelta {
    pub fn new_midi_event(&self) -> MidiEvent {
        MidiEvent {
            data: self.data.into(),
            delta_frames: self.delta_frames as i32,
            live: true,
            note_length: None,
            note_offset: None,
            detune: 0,
            note_off_velocity: 0,
        }
    }
}
