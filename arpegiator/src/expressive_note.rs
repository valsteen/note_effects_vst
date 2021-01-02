#[allow(unused_imports)]
use log::info;

use util::raw_message::RawMessage;
use util::messages::{NoteOn, Timbre, PitchBend, AfterTouch};

#[cfg(use_channel_pressure)]
use util::messages::Pressure;


pub struct ExpressiveNote {
    pub channel: u8,
    pub pitch: u8,
    pub velocity: u8,
    pub pressure: u8,
    pub timbre: u8,
    pub pitchbend: i32,
}


impl ExpressiveNote {
    #[cfg(not(use_channel_pressure))]
    #[inline]
    fn get_pressure_note(&self) -> RawMessage {
        AfterTouch {
            channel: self.channel,
            pitch: self.pitch,
            value: self.pressure,
        }.into()
    }

    #[cfg(use_channel_pressure)]
    #[inline]
    fn get_pressure_note(&self) -> RawMessage {
        Pressure {
            channel: self.channel,
            value: self.pressure,
        }.into()
    }

    pub fn into_rawmessages(self) -> Vec<RawMessage> {
        vec![
            PitchBend {
                channel: self.channel,
                millisemitones: self.pitchbend,
            }.into(),
            Timbre {
                channel: self.channel,
                value: self.timbre,
            }.into(),
            self.get_pressure_note(),
            NoteOn {
                channel: self.channel,
                pitch: self.pitch,
                velocity: self.velocity, // todo mixing between pattern and note
            }.into(),
        ]
    }
}
