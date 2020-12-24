use core::convert::From;
use super::raw_message::RawMessage;
use super::absolute_time_midi_message::AbsoluteTimeMidiMessage;
use super::messages::{NoteOn, NoteOff, CC, Pressure, PitchBend, GenericChannelMessage};


pub enum MidiMessageType {
    NoteOnMessage(NoteOn),
    NoteOffMessage(NoteOff),
    CCMessage(CC),
    PressureMessage(Pressure),
    PitchBendMessage(PitchBend),
    UnsupportedChannelMessage(GenericChannelMessage),
    Unsupported
}

impl MidiMessageType {
    pub fn is_same_note(&self, other: &MidiMessageType) -> bool {
        let (channel, pitch) = match self {
            MidiMessageType::NoteOnMessage(m) => (m.channel, m.pitch),
            MidiMessageType::NoteOffMessage(m) => (m.channel, m.pitch),
            _ => return false
        };

        let (channel2, pitch2) = match other {
            MidiMessageType::NoteOnMessage(m) => (m.channel, m.pitch),
            MidiMessageType::NoteOffMessage(m) => (m.channel, m.pitch),
            _ => return false
        };

        channel == channel2 && pitch == pitch2
    }
}


impl From<RawMessage> for MidiMessageType {
    fn from(data: RawMessage) -> Self {
        match data[0] & 0xF0 {
            0x80 => MidiMessageType::NoteOffMessage(NoteOff::from(data)),
            0x90 => MidiMessageType::NoteOnMessage(NoteOn::from(data)),
            0xB0 => MidiMessageType::CCMessage(CC::from(data)),
            0xD0 => MidiMessageType::PressureMessage(Pressure::from(data)),
            0xE0 => MidiMessageType::PitchBendMessage(PitchBend::from(data)),
            0xA0 | 0xC0 | 0xF0 => MidiMessageType::UnsupportedChannelMessage(GenericChannelMessage::from(data)),
            _ => MidiMessageType::Unsupported
        }
    }
}


impl From<&[u8; 3]> for MidiMessageType {
    fn from(data: &[u8; 3]) -> Self {
        Self::from(RawMessage::from(*data))
    }
}


impl From<&AbsoluteTimeMidiMessage> for MidiMessageType {
    fn from(m: &AbsoluteTimeMidiMessage) -> Self {
        m.data.into()
    }
}

impl From<&mut AbsoluteTimeMidiMessage> for MidiMessageType {
    fn from(m: &mut AbsoluteTimeMidiMessage) -> Self {
        m.data.into()
    }
}
