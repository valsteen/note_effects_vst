use core::clone::Clone;
use core::fmt::Display;
use core::fmt;
use core::option::Option::{Some, None};
use core::option::Option;
use global_counter::primitive::exact::CounterUsize;
use vst::event::{Event, MidiEvent};

use super::raw_message::RawMessage;

static NOTE_SEQUENCE_ID: CounterUsize = CounterUsize::new(0);

#[derive(Copy)]
pub struct AbsoluteTimeMidiMessage {
    pub data: RawMessage,
    // helps figuring note on/note off pair without relying on channel/pitch
    pub id: usize,
    pub play_time_in_samples: usize,
}

impl AbsoluteTimeMidiMessage {
    pub fn new(raw_message: RawMessage, play_time_in_samples: usize) -> Self {
        AbsoluteTimeMidiMessage {
            id: NOTE_SEQUENCE_ID.inc(),
            data: raw_message,
            play_time_in_samples,
        }
    }

    pub fn from_event(event: &Event, current_time_in_samples: usize) -> Option<AbsoluteTimeMidiMessage> {
        match event {
            Event::Midi(e) => {
                Some(AbsoluteTimeMidiMessage::new(
                    e.data.into(),
                    current_time_in_samples + e.delta_frames as usize,
                ))
            }
            Event::SysEx(_) => { None }
            Event::Deprecated(_) => { None }
        }
    }

    pub fn new_midi_event(&self, current_time_in_samples: usize) -> MidiEvent {
        MidiEvent {
            data: self.data.into(),
            delta_frames: (self.play_time_in_samples - current_time_in_samples) as i32,
            live: true,
            note_length: None,
            note_offset: None,
            detune: 0,
            note_off_velocity: 0
        }
    }
}


impl Clone for AbsoluteTimeMidiMessage {
    fn clone(&self) -> Self {
        AbsoluteTimeMidiMessage {
            id: self.id,
            data: self.data,
            play_time_in_samples: self.play_time_in_samples,
        }
    }

    fn clone_from(&mut self, source: &Self) {
        self.id = source.id;
        self.data = source.data;
        self.play_time_in_samples = source.play_time_in_samples
    }
}

impl Display for AbsoluteTimeMidiMessage {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.write_str(&*format!("{} [{:#04X} {:#04X} {:#04X}]", self.play_time_in_samples, self.data[0], self.data[1], self.data[2]))
    }
}
