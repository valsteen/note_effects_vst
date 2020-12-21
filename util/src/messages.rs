use vst::event::Event::Midi;
use vst::event::{Event, MidiEvent};
use crate::constants::{NOTE_ON, NOTE_OFF, PRESSURE, PITCHBEND};
use std::fmt::Display;
use std::fmt;
use std::ops::Index;


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


impl Clone for AbsoluteTimeMidiMessage {
    fn clone(&self) -> Self {
        AbsoluteTimeMidiMessage {
            data: self.data,
            play_time_in_samples: self.play_time_in_samples,
        }
    }

    fn clone_from(&mut self, source: &Self) {
        self.data = source.data;
        self.play_time_in_samples = source.play_time_in_samples
    }
}

impl Display for AbsoluteTimeMidiMessage {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.write_str(&*format!("{} [{:#04X} {:#04X} {:#04X}]", self.play_time_in_samples, self.data[0], self.data[1], self.data[2]))
    }
}

impl Clone for RawMessage {
    fn clone(&self) -> RawMessage {
        RawMessage(self.0)
    }
}

#[derive(Copy)]
pub struct AbsoluteTimeMidiMessage {
    pub data: RawMessage,
    pub play_time_in_samples: usize,
}

impl AbsoluteTimeMidiMessage {
    pub fn from_event(event: &Event, current_time_in_samples: usize) -> Option<AbsoluteTimeMidiMessage> {
        match event {
            Event::Midi(e) => {
                Some(AbsoluteTimeMidiMessage {
                    data: RawMessage(e.data),
                    play_time_in_samples: current_time_in_samples + e.delta_frames as usize,
                })
            }
            Event::SysEx(_) => { None }
            Event::Deprecated(_) => { None }
        }
    }

    pub fn new_midi_event(&self, current_time_in_samples: usize) -> MidiEvent {
        MidiEvent {
            data: self.data.0,
            delta_frames: (self.play_time_in_samples - current_time_in_samples) as i32,
            live: true,
            note_length: None,
            note_offset: None,
            detune: 0,
            note_off_velocity: 0
        }
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

#[derive(Copy)]
pub struct RawMessage([u8; 3]);

impl From<[u8;3]> for RawMessage {
    fn from(e: [u8; 3]) -> Self {
        RawMessage(e)
    }
}

impl Into<[u8;3]> for RawMessage {
    fn into(self) -> [u8; 3] {
        self.0
    }
}

impl Index<usize> for RawMessage {
    type Output = u8;

    fn index(&self, index: usize) -> &Self::Output {
        &self.0[index]
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
        RawMessage([NOTE_ON + self.channel, self.pitch, self.velocity])
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
        RawMessage([NOTE_OFF + self.channel, self.pitch, self.velocity])
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
    channel: u8,
    value: u8
}

impl Into<RawMessage> for Pressure {
    fn into(self) -> RawMessage {
        RawMessage([PRESSURE + self.channel, self.value, 0])
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
    channel: u8,
    semitones: u8,
    millisemitones: u8
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
        let millisemitones = (self.semitones as i32 * 1000) + self.millisemitones as i32 ;
        let value = ((millisemitones + 48000) * 16383) / 96000;
        let msb = value >> 7;
        let lsb = value & 0x7F;
        RawMessage([self.channel + PITCHBEND, lsb as u8, msb as u8])
    }
}

impl From<RawMessage> for PitchBend {
    fn from(data: RawMessage) -> Self {
        let lsb : i32 = data[1] as i32;
        let msb : i32 = data[2] as i32;
        let value = lsb + (msb << 7);
        let millisemitones = (value * 96000 / 16383) - 48000;

        PitchBend {
            channel: data[0] & 0x0F,
            semitones: (millisemitones / 1000) as u8,
            millisemitones: (millisemitones % 1000) as u8
        }
    }
}

pub struct CC {
    channel: u8,
    cc: u8,
    value: u8
}

impl Into<RawMessage> for CC {
    fn into(self) -> RawMessage {
        RawMessage([self.channel, self.cc, self.value])
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
