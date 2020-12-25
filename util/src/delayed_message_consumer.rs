use core::iter::Iterator;
use std::collections::HashMap;
use std::collections::hash_map::RandomState;
use std::ops::{DerefMut, Deref};
use vst::event::MidiEvent;

use super::absolute_time_midi_message::AbsoluteTimeMidiMessage;
use super::absolute_time_midi_message_vector::AbsoluteTimeMidiMessageVector;
use super::midi_message_type::MidiMessageType;
use super::messages::NoteOff;


#[derive(Hash, Clone, Copy, PartialEq, Eq)]
struct PlayingNoteIndex {
    channel: u8,
    pitch: u8,
}

#[derive(Clone, Copy, PartialEq)]
pub enum MessageReason {
    Live,
    Delayed,   // the same event will exist live and delayed
    MaxNotes,
    Retrigger
}

#[derive(Default)]
struct PlayingNotes(HashMap<PlayingNoteIndex, AbsoluteTimeMidiMessage>);

impl Deref for PlayingNotes {
    type Target = HashMap<PlayingNoteIndex, AbsoluteTimeMidiMessage, RandomState>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl DerefMut for PlayingNotes {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

impl PlayingNotes {
    fn oldest_playing_note(&self, delayed_only: bool) -> Option<&AbsoluteTimeMidiMessage> {
        self.iter().fold(
            None, |prev, (_, message)| {
                if prev.is_none()
                    || (!(delayed_only && message.reason == MessageReason::Live) && prev.unwrap().id > message.id) {
                    Some(message)
                } else {
                    prev
                }
            }
        )
    }
}


pub fn process_scheduled_events(samples: usize, current_time_in_samples: usize,
                                messages: &AbsoluteTimeMidiMessageVector, max_notes: u8,
                                apply_max_notes_to_delayed_notes_only: bool, delay_is_active: bool
) -> (AbsoluteTimeMidiMessageVector, Vec<MidiEvent>) {
    let mut playing_notes: PlayingNotes = PlayingNotes::default();
    let mut queued_messages = AbsoluteTimeMidiMessageVector::default();
    let mut notes_on_to_requeue : HashMap<usize, AbsoluteTimeMidiMessage> = HashMap::new();
    let mut events: Vec<MidiEvent> = vec![];

    let mut add_event = |event: AbsoluteTimeMidiMessage| {
        if event.play_time_in_samples < current_time_in_samples + samples {
            // test if it belongs to that time window, as we don't want to replay notes on we put
            // back in the scheduled queue
            if event.play_time_in_samples >= current_time_in_samples {
                if let MidiMessageType::NoteOffMessage(_) = MidiMessageType::from(event) {
                    let note_on = notes_on_to_requeue.get_mut( & event.id);
                    if note_on.is_none() { return }  // no such note running, skip
                    let note_on = note_on.unwrap();

                    if event.reason == MessageReason::Live && delay_is_active {
                        // mark the note on as delayed from now on, but don't sent the note off
                        note_on.reason = MessageReason::Delayed;
                        return
                    }

                    if apply_max_notes_to_delayed_notes_only &&
                        event.reason == MessageReason::MaxNotes &&
                        note_on.reason == MessageReason::Live {
                        // should be redundant, as MaxNotes messages are not generated for Live notes.
                        // keeping the logic to facilitate potential refactoring
                        return
                    }

                    // stop this note, don't requeue
                    notes_on_to_requeue.remove(&event.id);
                }
                events.push(event.new_midi_event(current_time_in_samples));
            }

            if let MidiMessageType::NoteOnMessage(_) = MidiMessageType::from(event) {
                notes_on_to_requeue.insert(event.id, event);
            }
        } else {
            queued_messages.push(event);
        }
    };

    for mut message in messages.iter().copied() {
        if message.play_time_in_samples < current_time_in_samples {
            match MidiMessageType::from(message) {
                MidiMessageType::NoteOnMessage(_) => {}
                _ => { panic!("Only pending note on are expected to be found in the past") }
            }
        };

        match MidiMessageType::from(message) {
            MidiMessageType::NoteOnMessage(note_on) => {
                // if still playing : generate note off at current sample, put note on with
                // delta + 1 in the queue
                let index = PlayingNoteIndex { channel: note_on.channel, pitch: note_on.pitch };

                if let Some(already_playing_note) = playing_notes.get(&index) {
                    // we were still playing that note. generate a note off first.
                    add_event(AbsoluteTimeMidiMessage {
                        data: NoteOff {
                            channel: note_on.channel,
                            pitch: note_on.pitch,
                            velocity: 0,
                        }.into(),
                        id: already_playing_note.id,
                        reason: MessageReason::Retrigger,
                        play_time_in_samples: message.play_time_in_samples,
                    });

                    // move the note on to the next sample or the daw might be confused
                    message.play_time_in_samples += 1;
                } else if max_notes > 0 && playing_notes.len() >= max_notes as usize {
                    if let Some(oldest_playing_note)
                        = playing_notes.oldest_playing_note(apply_max_notes_to_delayed_notes_only) {

                        let oldest_playing_note = *oldest_playing_note; // drop the borrow

                        playing_notes.remove(&PlayingNoteIndex {
                            channel: oldest_playing_note.get_channel(),
                            pitch: oldest_playing_note.get_pitch(),
                        });

                        add_event(AbsoluteTimeMidiMessage {
                            data: NoteOff {
                                channel: oldest_playing_note.get_channel(),
                                pitch: oldest_playing_note.get_pitch(),
                                velocity: 0,
                            }.into(),
                            id: oldest_playing_note.id,
                            play_time_in_samples: message.play_time_in_samples,
                            reason: MessageReason::MaxNotes,
                        });
                    };
                }

                playing_notes.insert(index, message);
                add_event(message);
            }

            MidiMessageType::NoteOffMessage(note_off) => {
                let playing_note = PlayingNoteIndex { channel: note_off.channel, pitch: note_off.pitch };
                match playing_notes.get(&playing_note) {
                    Some(currently_playing_note) => {
                        if currently_playing_note.id == message.id {
                            playing_notes.remove(&playing_note);
                            add_event(message);
                            continue;
                        } else {
                            // this note was interrupted earlier already, don't send that
                            // note off or we may interrupt a new note with that delayed note
                            // off
                            continue;
                        }
                    }
                    None => {
                        // was not playing at all, skip
                        continue;
                    }
                };
            }
            _ => {
                add_event(message);
            }
        }
    }

    for (_, event) in notes_on_to_requeue {
        queued_messages.ordered_insert(event)
    }

    (queued_messages, events)
}
