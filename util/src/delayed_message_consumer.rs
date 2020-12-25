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

struct Live(bool);

#[derive(Default)]
struct PlayingNotes(HashMap<PlayingNoteIndex, (Live, usize)>);

impl Deref for PlayingNotes {
    type Target = HashMap<PlayingNoteIndex, (Live, usize), RandomState>;

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
    fn oldest_playing_note(&self, exclude_live: bool) -> (PlayingNoteIndex, usize) {
        self.iter().fold(
            (PlayingNoteIndex { channel: 0, pitch: 0 }, usize::MAX),
            |(oldest_playing_note, oldest_id), (playing_note, (live, id))| {
                if (!live.0 || !exclude_live) && *id < oldest_id {
                    (*playing_note, *id)
                } else {
                    (oldest_playing_note, oldest_id)
                }
            },
        )
    }
}


pub fn process_scheduled_events(samples: usize, current_time_in_samples: usize,
                                messages: &AbsoluteTimeMidiMessageVector, max_notes: u8,
                                apply_max_notes_to_delayed_notes_only: bool
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
                events.push(event.new_midi_event(current_time_in_samples));

                if let MidiMessageType::NoteOffMessage(_) = MidiMessageType::from(event) {
                    // found a note off for this note on in this time window, so we won't requeue
                    notes_on_to_requeue.remove(&event.id);
                }
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
                let playing_note = PlayingNoteIndex { channel: note_on.channel, pitch: note_on.pitch };

                if let Some(playing_id) = playing_notes.get(&playing_note) {
                    let playing_id = *playing_id;

                    // replace the id of the playing note
                    playing_notes.insert(playing_note, message.id);

                    // we were still playing that note. generate a note off first.
                    add_event(AbsoluteTimeMidiMessage {
                        data: NoteOff {
                            channel: note_on.channel,
                            pitch: note_on.pitch,
                            velocity: 0,
                        }.into(),
                        id: playing_id,
                        play_time_in_samples: message.play_time_in_samples,
                    });

                    // move the note on to the next sample or the daw might be confused
                    message.play_time_in_samples += 1;
                    add_event(message);
                    continue;
                } else {
                    // handle note limit
                    if max_notes > 0 && playing_notes.len() == max_notes as usize {
                        let (oldest_playing_note, oldest_id) = playing_notes.oldest_playing_note();

                        playing_notes.remove(&oldest_playing_note);
                        playing_notes.insert(playing_note, message.id);

                        add_event(AbsoluteTimeMidiMessage {
                            data: NoteOff {
                                channel: oldest_playing_note.channel,
                                pitch: oldest_playing_note.pitch,
                                velocity: 0,
                            }.into(),
                            id: oldest_id,
                            play_time_in_samples: message.play_time_in_samples,
                        });

                        add_event(message);
                        continue;
                    } else {
                        playing_notes.insert(playing_note, message.id);
                        add_event(message);
                        continue;
                    }
                }
            }
            MidiMessageType::NoteOffMessage(note_off) => {
                let playing_note = PlayingNoteIndex { channel: note_off.channel, pitch: note_off.pitch };
                match playing_notes.get(&playing_note) {
                    Some(id) => {
                        if *id == message.id {
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
