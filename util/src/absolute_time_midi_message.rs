use core::clone::Clone;
use core::fmt;
use core::fmt::Display;
//use core::option::Option::{Some, None};
//use vst::event::{Event, MidiEvent};

use super::raw_message::RawMessage;
use crate::delayed_message_consumer::MessageReason;
use std::cmp::max;
use vst::event::MidiEvent;

#[derive(Copy)]
pub struct AbsoluteTimeMidiMessage {
    pub data: RawMessage,
    // helps figuring note on/note off pair without relying on channel/pitch
    pub id: usize,
    pub reason: MessageReason,
    pub play_time_in_samples: usize,
}

impl AbsoluteTimeMidiMessage {
    pub fn new_midi_event(&self, current_time_in_samples: usize) -> MidiEvent {
        MidiEvent {
            data: self.data.into(),
            delta_frames: max(0, self.play_time_in_samples - current_time_in_samples) as i32,
            live: true,
            note_length: None,
            note_offset: None,
            detune: 0,
            note_off_velocity: 0,
        }
    }

    pub fn get_channel(&self) -> u8 {
        assert!(self.data[0] >= 0x80 && self.data[0] <= 0x9F);
        self.data[0] & 0x0F
    }

    pub fn get_pitch(&self) -> u8 {
        assert!(self.data[0] >= 0x80 && self.data[0] <= 0x9F);
        self.data[1]
    }
}

impl Clone for AbsoluteTimeMidiMessage {
    fn clone(&self) -> Self {
        AbsoluteTimeMidiMessage {
            id: self.id,
            data: self.data,
            play_time_in_samples: self.play_time_in_samples,
            reason: self.reason,
        }
    }

    fn clone_from(&mut self, source: &Self) {
        self.id = source.id;
        self.data = source.data;
        self.play_time_in_samples = source.play_time_in_samples;
        self.reason = source.reason
    }
}

impl Display for AbsoluteTimeMidiMessage {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.write_str(&*format!(
            "{} {} [{:#04X} {:#04X} {:#04X}]",
            self.play_time_in_samples, self.id, self.data[0], self.data[1], self.data[2]
        ))
    }
}
