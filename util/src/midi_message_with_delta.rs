use crate::raw_message::RawMessage;
use serde::{Deserialize, Serialize};
use vst::buffer::PlaceholderEvent;
use vst::event::MidiEvent;

#[derive(Copy, Clone, Debug, Serialize, Deserialize)]
pub struct MidiMessageWithDelta {
    pub delta_frames: u16,
    pub data: RawMessage,
}

impl vst::buffer::WriteIntoPlaceholder for MidiMessageWithDelta {
    fn write_into(&self, out: &mut PlaceholderEvent) {
        MidiEvent {
            data: self.data.into(),
            delta_frames: self.delta_frames as i32,
            live: false,
            note_length: None,
            note_offset: None,
            detune: 0,
            note_off_velocity: 0,
        }
        .write_into(out)
    }
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
