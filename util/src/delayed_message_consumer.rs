use core::iter::Iterator;
use core::option::Option::{None, Some};
use core::option::Option;

use super::absolute_time_midi_message::AbsoluteTimeMidiMessage;
use crate::absolute_time_midi_message_vector::AbsoluteTimeMidiMessageVector;
use super::debug::DebugSocket;


pub struct DelayedMessageConsumer<'a> {
    pub samples_in_buffer: usize,
    pub messages: &'a mut AbsoluteTimeMidiMessageVector,
    pub current_time_in_samples: usize,
    pub drop_late_events: bool
}

impl<'a> Iterator for DelayedMessageConsumer<'a> {
    type Item = AbsoluteTimeMidiMessage;

    fn next(&mut self) -> Option<Self::Item> {
        loop {
            if self.messages.is_empty() {
                return None;
            }

            let delayed_message = &self.messages[0];

            if delayed_message.play_time_in_samples < self.current_time_in_samples {
                // can happen if the delay time is modulated
                if self.drop_late_events {
                    DebugSocket::send(&*format!(
                        "too late for {} ( current buffer: {} - {}, removing",
                        delayed_message,
                        self.current_time_in_samples,
                        self.current_time_in_samples + self.samples_in_buffer
                    ));
                    self.messages.remove(0);
                    continue;
                } else {
                    // immediately send it
                    let mut delayed_message = self.messages.remove(0);
                    delayed_message.play_time_in_samples = self.current_time_in_samples;
                    return Some(delayed_message);
                }
            };

            if delayed_message.play_time_in_samples > self.current_time_in_samples + self.samples_in_buffer {
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
