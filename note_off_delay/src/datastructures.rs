use std::collections::HashMap;
use crate::messages::{AbsoluteTimeMidiMessage, NoteOn, MidiMessageType, NoteOff, ChannelMessage, NoteMessage};
use std::fmt::{Display, Formatter};
use std::fmt;
use util::debug::DebugSocket;
use std::hash::{Hash, Hasher};
use std::ops::{Deref, DerefMut};


#[derive(Eq)]
pub struct CurrentPlayingNotesIndex([u8; 2]);

impl From<&AbsoluteTimeMidiMessage> for CurrentPlayingNotesIndex {
    fn from(m: &AbsoluteTimeMidiMessage) -> Self {
        match MidiMessageType::from(m) {
            MidiMessageType::NoteOnMessage(m) => CurrentPlayingNotesIndex([m.get_channel(), m.get_pitch()]),
            MidiMessageType::NoteOffMessage(m) => CurrentPlayingNotesIndex([m.get_channel(), m.get_pitch()]),
            _ => panic!("only note messages are allowed")
        }
    }
}

impl PartialEq for CurrentPlayingNotesIndex {
    fn eq(&self, other: &Self) -> bool {
        self.0[0] == other.0[0] && self.0[1] == other.0[1]
    }
}

impl Hash for CurrentPlayingNotesIndex {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.0[0].hash(state);
        self.0[1].hash(state);
    }
}


#[derive(Default)]
pub struct CurrentPlayingNotes(HashMap<CurrentPlayingNotesIndex, AbsoluteTimeMidiMessage>);

impl Display for CurrentPlayingNotes {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.write_str(&*self.0.keys().fold( String::new(), |acc, x| format!("{}, {}", acc, x)))
    }
}

impl Display for CurrentPlayingNotesIndex {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        f.write_str(&*format!("[{:#04X} {:#04X}]", self.0[0] as u8, self.0[1] as u8))
    }
}

impl CurrentPlayingNotes {
    fn oldest(&self) -> Option<AbsoluteTimeMidiMessage> {
        let oldest_note = match self.0.values()
            .min_by( |a, b| a.play_time_in_samples.cmp(&b.play_time_in_samples) ) {
            None => return None,
            Some(n) => n
        };

        Some(oldest_note.clone())
    }

    fn add_message(&mut self, message: AbsoluteTimeMidiMessage, max_notes: u8) -> Option<AbsoluteTimeMidiMessage> {
        let play_time_in_samples = message.play_time_in_samples;

        match MidiMessageType::from(&message) {
            MidiMessageType::NoteOnMessage(_) => {},
            _ => { return None }
        };

        self.0.insert(CurrentPlayingNotesIndex::from(&message), message);

        if max_notes > 0 && self.0.len() > max_notes as usize {
            let oldest = self.oldest() ;
            let oldest_note : NoteOn = match &oldest {
                None => return None,
                Some(m) => match MidiMessageType::from(m) {
                    MidiMessageType::NoteOnMessage(m) => m,
                    _ => return None
                }
            };

            self.0.remove_entry(&(CurrentPlayingNotesIndex::from(&oldest.unwrap())));

            return Some(AbsoluteTimeMidiMessage {
                data: NoteOff::from(oldest_note).into(),
                play_time_in_samples
            });
        }
        None
    }

    pub fn update(&mut self, messages: &[AbsoluteTimeMidiMessage], max_notes: u8) -> Vec<AbsoluteTimeMidiMessage> {
        let mut notes_off: Vec<AbsoluteTimeMidiMessage> = Vec::new();

        for message in messages {
            match MidiMessageType::from(message) {
                MidiMessageType::NoteOffMessage(m) => {
                    self.0.remove(&CurrentPlayingNotesIndex([m.get_channel(), m.get_pitch()]));
                }
                MidiMessageType::NoteOnMessage(_) => {
                    // TODO since we're forcefully stopping a note, another redundant note off may come later,
                    // that might not even happened if the user didn't release the key yet
                    // we may want to stop redundant notes off to happen by checking if the corresponding note
                    // is anyway playing according to our internal state
                    if let Some(note_off) = self.add_message(message.clone(), max_notes) {
                        notes_off.push(note_off);
                    }
                }
                _ => {}
            }
        }
        notes_off
    }
}

pub struct DelayedMessageConsumer<'a> {
    pub samples_in_buffer: usize,
    pub messages: &'a mut AbsoluteTimeMidiMessageVector,
    pub current_time_in_samples: usize,
}

impl<'a> Iterator for DelayedMessageConsumer<'a> {
    type Item = AbsoluteTimeMidiMessage;

    fn next(&mut self) -> Option<Self::Item> {
        loop {
            if self.messages.is_empty() {
                return None;
            }

            let delayed_message = &self.messages[0];
            let play_time_in_samples = delayed_message.play_time_in_samples;

            if play_time_in_samples < self.current_time_in_samples {
                DebugSocket::send(&*format!(
                    "too late for {} ( current buffer: {} - {}, removing",
                    delayed_message,
                    self.current_time_in_samples,
                    self.current_time_in_samples + self.samples_in_buffer
                ));
                self.messages.remove(0);
                continue;
            };

            if play_time_in_samples > self.current_time_in_samples + self.samples_in_buffer {
                // DebugSocket::send(&*format!(
                //     "too soon for {} ( planned: {} , current buffer: {} - {}",
                //     &delayed_event.event,
                //     delayed_event.play_time_in_samples,
                //     self.current_time_in_samples,
                //     self.current_time_in_samples + self.samples_in_buffer
                // ));
                return None;
            }

            let delayed_message: AbsoluteTimeMidiMessage = self.messages.remove(0);

            DebugSocket::send(&*format!(
                "will do {} ( current_time_in_samples={}, play_time_in_samples={} )",
                delayed_message,
                self.current_time_in_samples,
                delayed_message.play_time_in_samples
            ));

            return Some(delayed_message);
        }
    }
}

pub struct AbsoluteTimeMidiMessageVector(Vec<AbsoluteTimeMidiMessage>) ;

impl Default for AbsoluteTimeMidiMessageVector {
    fn default() -> Self {
        AbsoluteTimeMidiMessageVector(Default::default())
    }
}


impl AbsoluteTimeMidiMessageVector {
    // called when receiving events ; caller takes care of not pushing note offs in a first phase
    pub fn insert_message(&mut self, message: AbsoluteTimeMidiMessage) {
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
    pub fn merge_notes_off(&mut self, notes_off: &mut AbsoluteTimeMidiMessageVector, note_off_delay: usize) {
        for mut note_off_message in notes_off.iter().copied() {
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
                        self.push(note_off_message);
                        break;
                    }
                    Some(message_at_position) => {
                        if message_at_position.play_time_in_samples
                            <= note_off_message.play_time_in_samples
                        {
                            if MidiMessageType::from(&note_off_message).is_same_note(&MidiMessageType::from(message_at_position)) {
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

impl Deref for AbsoluteTimeMidiMessageVector {
    type Target = Vec<AbsoluteTimeMidiMessage>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl DerefMut for AbsoluteTimeMidiMessageVector {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}
