use vst::event::Event::Midi;
use vst::event::{Event, MidiEvent};
use util::constants::{NOTE_ON, NOTE_OFF, PRESSURE, PITCHBEND};
use std::fmt::Display;
use std::fmt;

pub type AbsoluteTimeMidiMessageVector = Vec<AbsoluteTimeMidiMessage>;

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

pub trait AbsoluteTimeMidiMessageVectorMethods {
    fn insert_message(&mut self, message: AbsoluteTimeMidiMessage);
    fn merge_notes_off(&mut self, notes_off: &mut AbsoluteTimeMidiMessageVector, note_off_delay: usize);
}

impl AbsoluteTimeMidiMessageVectorMethods for AbsoluteTimeMidiMessageVector {
    // called when receiving events ; caller takes care of not pushing note offs in a first phase
    fn insert_message(&mut self, message: AbsoluteTimeMidiMessage) {
        if let Some(insert_point) = self.iter().position(|message_at_position| {
            message.play_time_in_samples < message_at_position.play_time_in_samples
        }) {
            self.insert(insert_point, message);
        } else {
            self.push(message);
        }
    }

    // caller sends the notes off after inserting other events, so we know which notes are planned,
    // and insert notes off with the configured delay while making sure that between a note off
    // initial position and its final position, no note of same pitch and channel is triggered,
    // otherwise we will interrupt this second instance
    fn merge_notes_off(&mut self, notes_off: &mut AbsoluteTimeMidiMessageVector, note_off_delay: usize) {
        for mut note_off_message in notes_off {
            let mut iterator = self.iter();
            let mut position = 0;

            // find original position
            let mut current_message: Option<&AbsoluteTimeMidiMessage> = loop {
                match iterator.next() {
                    None => {
                        break None;
                    }
                    Some(message_at_position) => {
                        if note_off_message.play_time_in_samples
                            > message_at_position.play_time_in_samples
                        {
                            position += 1;
                            continue;
                        } else {
                            break Some(message_at_position);
                        }
                    }
                }
            };

            // add delay
            note_off_message.play_time_in_samples += note_off_delay;

            loop {
                match current_message {
                    None => {
                        self.push(note_off_message.clone());
                        break;
                    }
                    Some(message_at_position) => {
                        if message_at_position.play_time_in_samples
                            <= note_off_message.play_time_in_samples
                        {
                            if MidiMessageType::from(&*note_off_message).is_same_note(&message_at_position.into()) {
                                break;
                            }
                            position += 1;
                            current_message = iterator.next();
                            continue;
                        }

                        self.insert(position, note_off_message.clone());
                        break;
                    }
                }
            }
        }
    }
}


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


pub struct AbsoluteTimeMidiMessage {
    pub data: [u8;3],
    pub play_time_in_samples: usize,
}

impl AbsoluteTimeMidiMessage {
    pub fn from_event(event: &Event, current_time_in_samples: usize) -> Option<AbsoluteTimeMidiMessage> {
        match event {
            Event::Midi(e) => {
                Some(AbsoluteTimeMidiMessage {
                    data: e.data.clone(),
                    play_time_in_samples: current_time_in_samples + e.delta_frames as usize,
                })
            }
            Event::SysEx(_) => { None }
            Event::Deprecated(_) => { None }
        }
    }

    pub fn new_midi_event(&self, current_time_in_samples: usize) -> MidiEvent {
        MidiEvent {
            data: self.data,
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
        match m.data[0] & 0xF0 {
            0x80 => MidiMessageType::NoteOffMessage(NoteOff::from(m.data)),
            0x90 => MidiMessageType::NoteOnMessage(NoteOn::from(m.data)),
            0xB0 => MidiMessageType::CCMessage(CC::from(m.data)),
            0xD0 => MidiMessageType::PressureMessage(Pressure::from(m.data)),
            0xE0 => MidiMessageType::PitchBendMessage(PitchBend::from(m.data)),
            0xA0 | 0xC0 | 0xF0 => MidiMessageType::UnsupportedChannelMessage(GenericChannelMessage::from(m.data)),
            _ => MidiMessageType::Unsupported
        }
    }
}

pub type RawMessage = [u8; 3];

pub trait ChannelMessage {
    fn get_channel(&self) -> u8 ;
}

pub struct NoteOn {
    channel: u8,
    pitch: u8,
    velocity: u8
}

impl Into<RawMessage> for NoteOn {
    fn into(self) -> [u8; 3] {
        [NOTE_ON + self.channel, self.pitch, self.velocity]
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
    fn from(data: [u8; 3]) -> Self {
        NoteOn {
            channel: data[0] & 0x0F,
            pitch: data[1],
            velocity: data[2]
        }
    }
}


impl From<RawMessage> for NoteOff {
    fn from(data: [u8; 3]) -> Self {
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
    channel: u8,
    pitch: u8,
    velocity: u8
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
    fn into(self) -> [u8; 3] {
        [NOTE_OFF + self.channel, self.pitch, self.velocity]
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
    fn into(self) -> [u8; 3] {
        [PRESSURE + self.channel, self.value, 0]
    }
}

impl From<RawMessage> for Pressure {
    fn from(data: [u8; 3]) -> Self {
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
    fn into(self) -> [u8; 3] {
        // 96000 millisemitones are expressed over the possible values of 14 bits ( 16384 )
        // which never gets us an exact integer amount of semitones
        let millisemitones = (self.semitones as i32 * 1000) + self.millisemitones as i32 ;
        let value = ((millisemitones + 48000) * 16384) / 96000;
        let msb = value >> 7;
        let lsb = value & 0x7F;
        [self.channel + PITCHBEND, lsb as u8, msb as u8]
    }
}

impl From<RawMessage> for PitchBend {
    fn from(data: [u8; 3]) -> Self {
        let lsb : i32 = data[1] as i32;
        let msb : i32 = data[2] as i32;
        let value = lsb + (msb << 7);
        let millisemitones = (value * 96000 / 16384) - 48000;

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
    fn into(self) -> [u8; 3] {
        [self.channel, self.cc, self.value]
    }
}

impl From<RawMessage> for CC {
    fn from(data: [u8; 3]) -> Self {
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

pub struct GenericChannelMessage([u8; 3]);

impl From<RawMessage> for GenericChannelMessage {
    fn from(data: [u8; 3]) -> Self {
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