use core::iter::Iterator;
use std::collections::hash_map::RandomState;
use std::collections::HashMap;
use std::ops::{Deref, DerefMut};
use vst::event::MidiEvent;

use super::absolute_time_midi_message::AbsoluteTimeMidiMessage;
use super::absolute_time_midi_message_vector::AbsoluteTimeMidiMessageVector;
use super::messages::NoteOff;
use super::midi_message_type::MidiMessageType;
use std::fmt;
use std::fmt::{Display, Formatter};

#[derive(Hash, Clone, Copy, PartialEq, Eq)]
struct PlayingNoteIndex {
    channel: u8,
    pitch: u8,
}

#[derive(Eq, PartialEq, Clone)]
pub enum MaxNotesParameter {
    Infinite,
    Limited(u8),
}

impl Display for MaxNotesParameter {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self {
            MaxNotesParameter::Infinite => "Infinite".to_string(),
            MaxNotesParameter::Limited(x) => x.to_string(),
        }
        .fmt(f)
    }
}

impl MaxNotesParameter {
    pub fn should_limit(&self, currently_playing: usize) -> bool {
        match self {
            MaxNotesParameter::Infinite => false,
            MaxNotesParameter::Limited(limit) => currently_playing >= *limit as usize,
        }
    }
}

#[derive(Clone, Copy, PartialEq)]
pub enum MessageReason {
    Live,
    Delayed, // the same event will exist live and delayed
    MaxNotes,
    Retrigger,
    PlayUnprocessed,
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
        self.iter().fold(None, |prev, (_, message)| {
            if prev.is_none()
                || (!(delayed_only && message.reason == MessageReason::Live) && prev.unwrap().id > message.id)
            {
                Some(message)
            } else {
                prev
            }
        })
    }
}

pub struct ScheduledEventsHelper {
    playing_notes: PlayingNotes,
    pub queued_messages: AbsoluteTimeMidiMessageVector,
    notes_on_to_requeue: HashMap<usize, AbsoluteTimeMidiMessage>,
    pub events: Vec<MidiEvent>,
    buffer_duration_in_samples: usize,
    delay_is_active: bool,
    max_notes: MaxNotesParameter,
    apply_max_notes_to_delayed_notes_only: bool,
    current_time_in_samples: usize,
}

impl ScheduledEventsHelper {
    pub fn new(
        buffer_duration_in_samples: usize,
        delay_is_active: bool,
        max_notes: MaxNotesParameter,
        apply_max_notes_to_delayed_notes_only: bool,
        current_time_in_samples: usize,
    ) -> Self {
        Self {
            playing_notes: Default::default(),
            queued_messages: Default::default(),
            notes_on_to_requeue: Default::default(),
            events: vec![],
            buffer_duration_in_samples,
            delay_is_active,
            max_notes,
            apply_max_notes_to_delayed_notes_only,
            current_time_in_samples,
        }
    }

    fn add_event(&mut self, event: AbsoluteTimeMidiMessage) {
        if event.play_time_in_samples < self.current_time_in_samples + self.buffer_duration_in_samples {
            // test if it belongs to that time window, as we don't want to replay notes on we put
            // back in the scheduled queue
            if event.play_time_in_samples >= self.current_time_in_samples {
                if let MidiMessageType::NoteOffMessage(_) = MidiMessageType::from(event) {
                    let note_off_event = event;

                    let note_on = match self.notes_on_to_requeue.get_mut(&note_off_event.id) {
                        None => {
                            // no such note running, skip
                            return;
                        }
                        Some(note_on) => note_on,
                    };

                    if note_off_event.reason == MessageReason::Live
                        && self.delay_is_active
                        && !self.max_notes.should_limit(self.playing_notes.len() - 1)
                    {
                        // mark the note on as delayed from now on, but don't sent the note off
                        note_on.reason = MessageReason::Delayed;
                        return;
                    }

                    if self.apply_max_notes_to_delayed_notes_only
                        && note_off_event.reason == MessageReason::MaxNotes
                        && note_on.reason == MessageReason::Live
                    {
                        // should be redundant, as MaxNotes messages are not generated for Live notes.
                        // keeping the logic to facilitate potential refactoring
                        return;
                    }

                    // stop this note, don't requeue
                    self.playing_notes.remove(&PlayingNoteIndex {
                        pitch: note_off_event.get_pitch(),
                        channel: note_off_event.get_channel(),
                    });
                    self.notes_on_to_requeue.remove(&note_off_event.id);
                }
                self.events.push(event.new_midi_event(self.current_time_in_samples));
            }

            if let MidiMessageType::NoteOnMessage(_) = MidiMessageType::from(event) {
                self.playing_notes.insert(
                    PlayingNoteIndex {
                        pitch: event.get_pitch(),
                        channel: event.get_channel(),
                    },
                    event,
                );
                self.notes_on_to_requeue.insert(event.id, event);
            }
        } else {
            self.queued_messages.push(event);
        }
    }

    pub fn process_scheduled_events(mut self, messages: &AbsoluteTimeMidiMessageVector) -> (AbsoluteTimeMidiMessageVector, Vec<MidiEvent>) {
        for mut message in messages.iter().copied() {
            if message.play_time_in_samples < self.current_time_in_samples {
                match MidiMessageType::from(message) {
                    MidiMessageType::NoteOnMessage(_) => {}
                    _ => {
                        panic!("Only pending note on are expected to be found in the past")
                    }
                }
            };

            match MidiMessageType::from(message) {
                MidiMessageType::NoteOnMessage(note_on) => {
                    // if still playing : generate note off at current sample, put note on with
                    // delta + 1 in the queue
                    let index = PlayingNoteIndex {
                        channel: note_on.channel,
                        pitch: note_on.pitch,
                    };

                    if let Some(&AbsoluteTimeMidiMessage { id, .. }) = self.playing_notes.get(&index) {
                        // we were still playing that note. generate a note off first.
                        self.add_event(AbsoluteTimeMidiMessage {
                            data: NoteOff {
                                channel: note_on.channel,
                                pitch: note_on.pitch,
                                velocity: 0,
                            }
                            .into(),
                            id,
                            reason: MessageReason::Retrigger,
                            play_time_in_samples: message.play_time_in_samples,
                        });

                        // move the note on to the next sample or the daw might be confused
                        message.play_time_in_samples += 1;
                    } else if self.max_notes.should_limit(self.playing_notes.len()) {
                        if let Some(oldest_playing_note) = self
                            .playing_notes
                            .oldest_playing_note(self.apply_max_notes_to_delayed_notes_only)
                        {
                            let oldest_playing_note = *oldest_playing_note; // drop the borrow

                            self.add_event(AbsoluteTimeMidiMessage {
                                data: NoteOff {
                                    channel: oldest_playing_note.get_channel(),
                                    pitch: oldest_playing_note.get_pitch(),
                                    velocity: 0,
                                }
                                .into(),
                                id: oldest_playing_note.id,
                                play_time_in_samples: message.play_time_in_samples,
                                reason: MessageReason::MaxNotes,
                            });
                        };
                    }

                    self.add_event(message);
                }

                MidiMessageType::NoteOffMessage(note_off) => {
                    let playing_note = PlayingNoteIndex {
                        channel: note_off.channel,
                        pitch: note_off.pitch,
                    };
                    match self.playing_notes.get(&playing_note) {
                        Some(currently_playing_note) => {
                            if currently_playing_note.id == message.id {
                                self.add_event(message);
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
                    self.add_event(message);
                }
            }
        }

        for (_, event) in self.notes_on_to_requeue.drain() {
            self.queued_messages.ordered_insert(event)
        };

        (self.queued_messages, self.events)
    }
}

pub fn raw_process_scheduled_events(
    samples: usize,
    current_time_in_samples: usize,
    messages: &AbsoluteTimeMidiMessageVector,
) -> (AbsoluteTimeMidiMessageVector, Vec<MidiEvent>) {
    let mut queued_messages = AbsoluteTimeMidiMessageVector::default();
    let mut events: Vec<MidiEvent> = vec![];

    for message in messages.iter().copied() {
        if message.play_time_in_samples < current_time_in_samples + samples {
            if message.play_time_in_samples >= current_time_in_samples {
                events.push(message.new_midi_event(current_time_in_samples));
            }
        } else {
            queued_messages.push(message);
        }
    }

    (queued_messages, events)
}
