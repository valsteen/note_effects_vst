use std::ops::{Deref, DerefMut};

use global_counter::primitive::exact::CounterUsize;

use super::absolute_time_midi_message::AbsoluteTimeMidiMessage;
use super::debug::DebugSocket;
use super::delayed_message_consumer::MessageReason;
use super::midi_message_type::MidiMessageType;
use super::raw_message::RawMessage;

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

// plot twist: this vector is for all events, not just note on/note off.
// would still be OK to add that metadata to any event after all ? no
// as a weak reference should be ok
// then logically

static NOTE_SEQUENCE_ID: CounterUsize = CounterUsize::new(0);

impl AbsoluteTimeMidiMessageVector {
    pub fn insert_message(&mut self, data: [u8; 3], play_time_in_samples: usize, reason: MessageReason) {
        // we generate unique identifier per event. this is in order to match note on/note off pairs
        let channel_pitch_lookup = match MidiMessageType::from(RawMessage::from(data)) {
            MidiMessageType::NoteOffMessage(midi_message) => Some((midi_message.channel, midi_message.pitch)),
            _ => None,
        };

        let mut last_note_on_match = None;

        let insert_point = self.iter().position(|message_at_position| {
            if let Some((channel, pitch)) = channel_pitch_lookup {
                if let MidiMessageType::NoteOnMessage(midi_message) = MidiMessageType::from(*message_at_position) {
                    if channel == midi_message.channel && pitch == midi_message.pitch {
                        last_note_on_match = Some(message_at_position);
                    }
                }
            }

            // we use '<' and not '<=', because this method is called in the same order as events
            // come, and find their position starting from the beginning of the vector. By moving
            // past equally timed elements, we keep the original order.
            play_time_in_samples < message_at_position.play_time_in_samples
        });

        let id = if let Some(note_message) = last_note_on_match {
            note_message.id
        } else {
            NOTE_SEQUENCE_ID.inc()
        };

        let message = AbsoluteTimeMidiMessage {
            data: data.into(),
            id,
            play_time_in_samples,
            reason,
        };

        DebugSocket::send(&*format!("Inserting {}", message));
        if let Some(insert_point) = insert_point {
            self.insert(insert_point, message);
        } else {
            self.push(message);
        }
    }

    pub fn ordered_insert(&mut self, message: AbsoluteTimeMidiMessage) {
        let position = self
            .iter()
            .position(|message_at_position| message.play_time_in_samples < message_at_position.play_time_in_samples);

        match position {
            Some(position) => self.insert(position, message),
            None => self.push(message),
        }
    }
}
