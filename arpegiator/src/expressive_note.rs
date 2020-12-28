use util::raw_message::RawMessage;
use util::messages::{NoteOn, Timbre, Pressure, PitchBend};


pub struct ExpressiveNote {
    pub channel: u8,
    pub pitch: u8,
    pub velocity: u8,
    pub pressure: u8,
    pub timbre: u8,
    pub pitchbend: i32,
}

impl ExpressiveNote {
    pub fn into_rawmessages(self) -> Vec<RawMessage> {
        vec![
            Timbre {
                channel: self.channel,
                value: self.timbre,
            }.into(),
            Pressure {
                channel: self.channel,
                value: self.pressure,
            }.into(),
            PitchBend {
                channel: self.channel,
                millisemitones: self.pitchbend,
            }.into(),
            NoteOn {
                channel: self.channel,
                pitch: self.pitch,
                velocity: self.velocity, // todo mixing between pattern and note
            }.into(),
        ]
    }
}
