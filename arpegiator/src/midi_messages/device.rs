use log::info;
use std::cmp::Ordering;
use std::collections::HashMap;

use util::constants::TIMBRECC;
use util::messages::CC;
use util::midi_message_type::MidiMessageType;
use util::midi_message_with_delta::MidiMessageWithDelta;

use crate::midi_messages::note::{NoteIndex, Note, CCIndex};
use crate::midi_messages::timed_event::TimedEvent;


pub struct Device {
    pub _name: String,
    pub notes: HashMap<NoteIndex, Note>,
    pub cc: HashMap<CCIndex, u8>,
    pub channels: [Channel; 16],
    pub note_index: usize,
}


impl Device {
    pub fn new (name: String) -> Self {
        Device {
            _name: name,
            notes: Default::default(),
            cc: Default::default(),
            channels: [Channel {
                pressure: 0,
                pitchbend: 0,
                timbre: 0,
            }; 16],
            note_index: 0,
        }
    }
}


#[derive(Copy, Clone, Debug)]
pub struct Channel {
    pub pressure: u8,
    // in millisemitones
    pub pitchbend: i32,
    pub timbre: u8,
}

pub enum Expression {
    Timbre,
    Pressure,
    PitchBend,
    AfterTouch
}

pub enum DeviceChange {
    AddNote { time: usize, note: Note },
    RemoveNote { time: usize, note: Note },
    NoteExpressionChange { time: usize, expression: Expression, note: Note },
    ReplaceNote { time: usize, old_note: Note, new_note: Note },
    CCChange { time: usize, cc: CC },
    None { time: usize },
}


impl TimedEvent for DeviceChange {
    fn timestamp(&self) -> usize {
        match self {
            DeviceChange::AddNote { time, .. } => *time,
            DeviceChange::RemoveNote { time, .. } => *time,
            DeviceChange::NoteExpressionChange { time, .. } => *time,
            DeviceChange::ReplaceNote { time, .. } => *time,
            DeviceChange::CCChange { time, .. } => *time,
            DeviceChange::None { time, .. } => *time
        }
    }

    fn id(&self) -> usize {
        // used to order events that happen at the same time. Doesn't matter on CCs, in any case they'll be sorted
        // by time already
        match self {
            DeviceChange::AddNote { note, .. } => note.id,
            DeviceChange::RemoveNote { note, .. } => note.id,
            DeviceChange::NoteExpressionChange { note, .. } => note.id,
            DeviceChange::ReplaceNote { new_note: note, .. } => note.id,
            DeviceChange::CCChange { .. } => 0,
            DeviceChange::None { .. } => 0
        }
    }
}

impl PartialOrd for DeviceChange {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        let timestamp_cmp = self.timestamp().cmp(&other.timestamp());
        if timestamp_cmp == Ordering::Equal {
            Some(self.id().cmp(&other.id()))
        } else {
            Some(timestamp_cmp)
        }
    }
}

impl PartialEq for DeviceChange {
    fn eq(&self, other: &Self) -> bool {
        self.id() == other.id()
    }
}


impl Device {
    pub fn update(&mut self,
                  midi_message: MidiMessageWithDelta, current_time: usize, id: Option<usize>) -> DeviceChange {
        #[cfg(feature="device_debug")]
        info!("[{}] Got event: {:?} {:?} {:02X?}", self._name, id, current_time, midi_message);

        let time = current_time + midi_message.delta_frames as usize;

        match MidiMessageType::from(&midi_message.data.into()) {
            MidiMessageType::NoteOnMessage(note) => {
                let note_id = match id {
                    None => {
                        let note_id = self.note_index;
                        self.note_index += 1;
                        note_id
                    }
                    Some(id) => id
                };
                let index = NoteIndex { channel: note.channel, pitch: note.pitch };
                let new_note = Note {
                    id: note_id,
                    pressed_at: time,
                    released_at: 0,
                    channel: note.channel,
                    pitch: note.pitch,
                    velocity: note.velocity,
                    velocity_off: 0,
                    pressure: self.channels[note.channel as usize].pressure,
                    timbre: self.channels[note.channel as usize].timbre,
                    pitchbend: self.channels[note.channel as usize].pitchbend,
                };

                match self.notes.insert(index, new_note) {
                    None => {
                        DeviceChange::AddNote { time, note: new_note }
                    }
                    Some(old_note) => {
                        DeviceChange::ReplaceNote { time, old_note, new_note }
                    }
                }
            }
            MidiMessageType::NoteOffMessage(note) => {
                let index = NoteIndex { channel: note.channel, pitch: note.pitch };

                match self.notes.remove(&index) {
                    None => {
                        info!("Attempt to remove note, but it was not found {:02X?}", index);
                        DeviceChange::None { time }
                    }
                    Some(mut old_note) => {
                        //info!("Removed note {:02X?}", index);
                        old_note.released_at = time;
                        old_note.velocity_off = note.velocity;
                        DeviceChange::RemoveNote { time, note: old_note }
                    }
                }
            }
            MidiMessageType::CCMessage(cc) => {
                self.cc.insert(CCIndex { channel: cc.channel, index: cc.cc }, cc.value);
                if cc.cc == TIMBRECC {
                    self.channels[cc.channel as usize].timbre = cc.value;
                    for (_, note) in self.notes.iter_mut() {
                        if note.channel == cc.channel {
                            note.timbre = cc.value;
                            // note: per design simplification, having several notes running on the same channel
                            // is not supported. only the first note found on the channel is updated
                            return DeviceChange::NoteExpressionChange {
                                time,
                                expression: Expression::Timbre,
                                note: *note,
                            };
                        }
                    }
                }
                DeviceChange::CCChange { time, cc }
            }
            MidiMessageType::PressureMessage(message) => {
                self.channels[message.channel as usize].pressure = message.value;
                for (_, note) in self.notes.iter_mut() {
                    if note.channel == message.channel {
                        // note: per design simplification, having several notes running on the same channel
                        // is not supported. only the first note found on the channel is updated
                        note.pressure = message.value;
                        return DeviceChange::NoteExpressionChange {
                            time,
                            expression: Expression::Pressure,
                            note: *note,
                        };
                    }
                }
                DeviceChange::None { time }
            },
            MidiMessageType::AfterTouchMessage(message) => {
                // redundant with pressure, but that's the message that bitwig will properly handle for by-note
                // expressions
                for (_, note) in self.notes.iter_mut() {
                    if note.channel == message.channel && note.pitch == message.pitch {
                        note.pressure = message.value;
                        // since aftertouch is assigned by pitch and channel, contrary to channel pressure
                        // we are sure it's only affecting one note
                        return DeviceChange::NoteExpressionChange {
                            time,
                            expression: Expression::Pressure,
                            note: *note,
                        };
                    }
                }
                DeviceChange::None { time }
            },
            MidiMessageType::PitchBendMessage(message) => {
                self.channels[message.channel as usize].pitchbend = message.millisemitones;
                for (_, note) in self.notes.iter_mut() {
                    if note.channel == message.channel {
                        // note: per design simplification, having several notes running on the same channel
                        // is not supported. only the first note found on the channel is updated
                        note.pitchbend = message.millisemitones;
                        return DeviceChange::NoteExpressionChange {
                            time,
                            expression: Expression::PitchBend,
                            note: *note,
                        };
                    }
                }
                DeviceChange::None { time }
            }
            MidiMessageType::UnsupportedChannelMessage(_) => DeviceChange::None { time },
            MidiMessageType::Unsupported => DeviceChange::None { time },

        }
    }
}
