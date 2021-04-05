use std::ops::{Deref, DerefMut};

use global_counter::primitive::exact::CounterUsize;

use super::absolute_time_midi_message::AbsoluteTimeMidiMessage;
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
    pub fn get_matching_note_on(&self, channel: u8, pitch: u8) -> Option<&AbsoluteTimeMidiMessage> {
        self.iter().filter(|message|
            match MidiMessageType::from(**message) {
                MidiMessageType::NoteOnMessage(midi_message) => channel == midi_message.channel && pitch == midi_message.pitch,
                _ => false
            }
        ).last()
    }

    pub fn raw_insert(&mut self, data: [u8; 3], play_time_in_samples: usize) {
        // insert in vector, disregarding matching note on/off
        let message = AbsoluteTimeMidiMessage {
            data: data.into(),
            id: NOTE_SEQUENCE_ID.inc(),
            play_time_in_samples,
            reason: MessageReason::PlayUnprocessed
        };

        self.ordered_insert(message);
    }

    pub fn insert_message(&mut self, data: [u8; 3], play_time_in_samples: usize, reason: MessageReason) {
        let raw_message = RawMessage::from(data) ;

        let id = match MidiMessageType::from(raw_message) {
            MidiMessageType::NoteOffMessage(midi_message) => {
                match self.get_matching_note_on(midi_message.channel, midi_message.pitch) {
                    Some(note_on_message) => note_on_message.id,
                    None => NOTE_SEQUENCE_ID.inc()
                }
            },
            _ => NOTE_SEQUENCE_ID.inc(),
        };

        let message = AbsoluteTimeMidiMessage {
            data: raw_message,
            id,
            play_time_in_samples,
            reason,
        };

        self.ordered_insert(message);
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
