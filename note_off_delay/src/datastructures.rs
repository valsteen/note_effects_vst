use std::collections::HashMap;
use std::fmt::{Display, Formatter};
use std::fmt;
use std::hash::{Hash, Hasher};

use util::absolute_time_midi_message::AbsoluteTimeMidiMessage;
use util::messages::{NoteOff, NoteOn, ChannelMessage, NoteMessage};
use util::midi_message_type::MidiMessageType;


#[derive(Eq, Clone, Copy)]
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

        Some(*oldest_note)
    }

    // TODO
    fn get(&self, index: &CurrentPlayingNotesIndex) -> Option<&AbsoluteTimeMidiMessage> {
        self.0.get(index)
    }

    fn add_message(&mut self, message: AbsoluteTimeMidiMessage, max_notes: u8) -> Option<AbsoluteTimeMidiMessage> {
        let play_time_in_samples = message.play_time_in_samples;

        match (&message).into() {
            MidiMessageType::NoteOnMessage(_) => {},
            _ => { return None }
        };

        self.0.insert((&message).into(), message);

        if max_notes > 0 && self.0.len() > max_notes as usize {
            let oldest = self.oldest() ;
            let oldest_note : NoteOn = match &oldest {
                None => return None,
                Some(m) => match m.into() {
                    MidiMessageType::NoteOnMessage(m) => m,
                    _ => return None
                }
            };

            // TODO don't remove the entry here. When sending the corresponding note off,
            // we remove the playing note from currentplayingnotes by using the ID
            self.0.remove_entry(&(CurrentPlayingNotesIndex::from(&oldest.unwrap())));

            // TODO here use the ID of the note on we will remove
            // then just skip sending it
            return Some(AbsoluteTimeMidiMessage::new(
                NoteOff::from(oldest_note).into(),
                play_time_in_samples,
            ));
        }
        None
    }

    pub fn update(&mut self, messages: &[AbsoluteTimeMidiMessage], max_notes: u8) -> Vec<AbsoluteTimeMidiMessage> {
        let mut notes_off: Vec<AbsoluteTimeMidiMessage> = Vec::new();

        for message in messages {
            match MidiMessageType::from(message) {
                MidiMessageType::NoteOffMessage(m) => {
                    // find corresponding note on, create this note off with the same ID, remove from
                    // playing notes when sending
                    self.0.remove(&CurrentPlayingNotesIndex([m.get_channel(), m.get_pitch()]));
                }
                MidiMessageType::NoteOnMessage(_) => {
                    // TODO since we're forcefully stopping a note, another redundant note off may come later,
                    // that might not even happened if the user didn't release the key yet
                    // we may want to stop redundant notes off to happen by checking if the corresponding note
                    // is anyway playing according to our internal state
                    if let Some(note_off) = self.add_message(*message, max_notes) {
                        notes_off.push(note_off);
                    }
                }
                _ => {}
            }
        }
        notes_off
    }
}
