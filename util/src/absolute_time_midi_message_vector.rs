use std::ops::{Deref, DerefMut};

use super::midi_message_type::MidiMessageType;
use super::absolute_time_midi_message::AbsoluteTimeMidiMessage;

pub struct AbsoluteTimeMidiMessageVector(Vec<AbsoluteTimeMidiMessage>);

impl Default for AbsoluteTimeMidiMessageVector {
    fn default() -> Self {
        AbsoluteTimeMidiMessageVector(Default::default())
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


impl AbsoluteTimeMidiMessageVector {
    // called when receiving events ; caller takes care of not pushing note offs in a first phase
    pub fn insert_message(&mut self, message: AbsoluteTimeMidiMessage) {
        if let Some(insert_point) = self.iter().position(|message_at_position| {
            // since we insert in the same order as originally found, new messages should get after
            // those already present. Note off being moved after the same note on may occur otherwise
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

                        self.insert(position, note_off_message);
                        break;
                    }
                }
            }
        }
    }
}
