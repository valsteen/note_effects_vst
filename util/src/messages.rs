#[allow(unused_imports)]
use log::{info,error};

use vst::event::Event::Midi;
use vst::event::{Event, MidiEvent};

use super::constants::{NOTE_ON, NOTE_OFF, PRESSURE, PITCHBEND};
use super::raw_message::RawMessage;
use crate::constants::{TIMBRECC, AFTERTOUCH};

pub fn format_midi_event(e: &MidiEvent) -> String {
    format!(
        "[{:#04X} {:#04X} {:#04X}] delta_frames={}",
        e.data[0], e.data[1], e.data[2], e.delta_frames
    )
}

pub fn format_event(e: &Event) -> String {
    // notice bitwig only gives midi events anyway
    match e {
        Midi(e) => format_midi_event(e),
        Event::SysEx(e) => {
            format!(
                "SysEx [{}] delta_frames={}",
                e.payload
                    .iter()
                    .fold(String::new(), |x, u| x + &*format!(" {:#04X}", u)),
                e.delta_frames
            )
        }
        Event::Deprecated(e) => {
            format!(
                "Deprecated [{}] delta_frames={}",
                e._reserved
                    .iter()
                    .fold(String::new(), |x, u| x + &*format!(" {:#04X}", u)),
                e.delta_frames
            )
        }
    }
}

/*
    this contains midi events that have a play time not relative to the current buffer,
    but to the amount of samples since the plugin was active
*/

pub trait ChannelMessage {
    fn get_channel(&self) -> u8 ;
}

pub struct NoteOn {
    pub channel: u8,
    pub pitch: u8,
    pub velocity: u8
}

impl Into<RawMessage> for NoteOn {
    fn into(self) -> RawMessage {
        [NOTE_ON + self.channel, self.pitch, self.velocity].into()
    }
}

impl NoteMessage for NoteOn {
    fn get_pitch(&self) -> u8 {
        self.pitch
    }

    fn get_velocity(&self) -> u8 {
        self.velocity
    }
}


impl From<RawMessage> for NoteOn {
    fn from(data: RawMessage) -> Self {
        NoteOn {
            channel: data[0] & 0x0F,
            pitch: data[1],
            velocity: data[2]
        }
    }
}


impl From<RawMessage> for NoteOff {
    fn from(data: RawMessage) -> Self {
        NoteOff {
            channel: data[0] & 0x0F,
            pitch: data[1],
            velocity: data[2]
        }
    }
}

impl ChannelMessage for NoteOn {
    fn get_channel(&self) -> u8 {
        self.channel
    }
}

pub trait NoteMessage where Self: ChannelMessage {
    fn get_pitch(&self) -> u8;
    fn get_velocity(&self) -> u8 ;
}

pub struct NoteOff {
    pub channel: u8,
    pub pitch: u8,
    pub velocity: u8
}

impl From<NoteOn> for NoteOff {
    fn from(m: NoteOn) -> Self {
        NoteOff{
            channel: m.channel,
            pitch: m.pitch,
            velocity: 0
        }
    }
}

impl Into<RawMessage> for NoteOff {
    fn into(self) -> RawMessage {
        [NOTE_OFF + self.channel, self.pitch, self.velocity].into()
    }
}

impl ChannelMessage for NoteOff {
    fn get_channel(&self) -> u8 {
        self.channel
    }
}

impl NoteMessage for NoteOff {
    fn get_pitch(&self) -> u8 {
        self.pitch
    }

    fn get_velocity(&self) -> u8 {
        self.velocity
    }
}

pub struct Pressure {
    pub channel: u8,
    pub value: u8
}

impl Into<RawMessage> for Pressure {
    fn into(self) -> RawMessage {
        [PRESSURE + self.channel, self.value, 0].into()
    }
}

impl From<RawMessage> for Pressure {
    fn from(data: RawMessage) -> Self {
        Pressure {
            channel: data[0] & 0x0F,
            value: data[1]
        }
    }
}

impl ChannelMessage for Pressure {
    fn get_channel(&self) -> u8 {
        self.channel
    }
}

pub struct PitchBend {
    pub channel: u8,
    pub millisemitones: i32
}

impl ChannelMessage for PitchBend {
    fn get_channel(&self) -> u8 {
        self.channel
    }
}

impl Into<RawMessage> for PitchBend {
    fn into(self) -> RawMessage {
        // 96000 millisemitones are expressed over the possible values of 14 bits ( 16384 )
        // which never gets us an exact integer amount of semitones
        let value = ((self.millisemitones + 48000) * 16384) / 96000;
        let msb = value >> 7;
        let lsb = value & 0x7F;
        [self.channel + PITCHBEND, lsb as u8, msb as u8].into()
    }
}

impl From<RawMessage> for PitchBend {
    fn from(data: RawMessage) -> Self {
        let lsb : i32 = data[1] as i32;
        let msb : i32 = data[2] as i32;
        let value = lsb + (msb << 7);
        let millisemitones = (value * 96000 / 16384) - 48000;

        PitchBend {
            channel: data[0] & 0x0F,
            millisemitones
        }
    }
}


#[derive(Debug)]
pub struct AfterTouch {
    pub channel: u8,
    pub pitch: u8,
    pub value: u8
}

impl ChannelMessage for AfterTouch {
    fn get_channel(&self) -> u8 {
        self.channel
    }
}

impl Into<RawMessage> for AfterTouch {
    fn into(self) -> RawMessage {
        [self.channel + AFTERTOUCH, self.pitch, self.value].into()
    }
}

impl From<RawMessage> for AfterTouch {
    fn from(data: RawMessage) -> Self {
        AfterTouch {
            channel: data[0] & 0x0F,
            pitch: data[1],
            value: data[2]
        }
    }
}

pub struct CC {
    pub channel: u8,
    pub cc: u8,
    pub value: u8
}

impl Into<RawMessage> for CC {
    fn into(self) -> RawMessage {
        [0xB0 + self.channel, self.cc, self.value].into()
    }
}

impl From<RawMessage> for CC {
    fn from(data: RawMessage) -> Self {
        CC {
            channel: data[0] & 0x0F,
            cc: data[1],
            value: data[2]
        }
    }
}

impl ChannelMessage for CC {
    fn get_channel(&self) -> u8 {
        self.channel
    }
}

pub struct GenericChannelMessage(RawMessage);

impl ChannelMessage for GenericChannelMessage {
    fn get_channel(&self) -> u8 {
        self.0[0] & 0x0F
    }
}

impl From<RawMessage> for GenericChannelMessage {
    fn from(data: RawMessage) -> Self {
        GenericChannelMessage(data)
    }
}


impl From<&[u8; 3]> for GenericChannelMessage {
    fn from(data: &[u8; 3]) -> Self {
        GenericChannelMessage(RawMessage::from(*data))
    }
}

pub struct Timbre {
    pub channel: u8,
    pub value: u8
}

impl Into<RawMessage> for Timbre {
    fn into(self) -> RawMessage {
        CC {
            channel: self.channel,
            cc: TIMBRECC,
            value: self.value
        }.into()
    }
}
