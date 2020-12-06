mod messages;
mod parameters;

#[macro_use]
extern crate vst;

use std::sync::Arc;

use vst::api;
use vst::buffer::{AudioBuffer, SendEventBuffer};
use vst::plugin::{CanDo, Category, HostCallback, Info, Plugin};

use crate::messages::{MidiMessageType, AbsoluteTimeMidiMessage, NoteMessage, NoteOff, ChannelMessage, NoteOn};
use messages::{AbsoluteTimeMidiMessageVector, AbsoluteTimeMidiMessageVectorMethods};
use parameters::NoteOffDelayPluginParameters;
use std::collections::HashMap;
use util::debug::DebugSocket;
use std::fmt::Display;
use std::fmt;
use util::parameters::ParameterConversion;
use crate::parameters::Parameter;
use std::cell::RefCell;

plugin_main!(NoteOffDelayPlugin);

pub struct NoteOffDelayPlugin {
    send_buffer: RefCell<SendEventBuffer>,
    parameters: Arc<NoteOffDelayPluginParameters>,
    sample_rate: f32,
    current_time_in_samples: usize,
    message_queue: AbsoluteTimeMidiMessageVector,
    current_playing_notes: CurrentPlayingNotes,
}

impl Default for NoteOffDelayPlugin {
    fn default() -> Self {
        NoteOffDelayPlugin {
            send_buffer: Default::default(),
            parameters: Arc::new(Default::default()),
            sample_rate: 44100.0,
            current_time_in_samples: 0,
            message_queue: Vec::new(),
            current_playing_notes: Default::default(),
        }
    }
}

impl NoteOffDelayPlugin {
    fn send_events(&mut self, samples: usize) {
        if let Ok(mut host_callback_lock) = self.parameters.host_mutex.lock() {
            let message_consumer: DelayedMessageConsumer = DelayedMessageConsumer {
                samples_in_buffer: samples,
                messages: &mut self.message_queue,
                current_time_in_samples: self.current_time_in_samples,
            };

            let mut messages: Vec<AbsoluteTimeMidiMessage> = message_consumer.collect();
            let notes_off = self
                .current_playing_notes
                .update(&messages, self.parameters.get_max_notes());

            for note_off in notes_off {
                messages.push(note_off);
            }

            self.send_buffer.borrow_mut()
                .send_events(messages.iter().map(|e| e.new_midi_event(self.current_time_in_samples) ), &mut host_callback_lock.host);
        }
    }

    #[allow(dead_code)]
    fn seconds_per_sample(&self) -> f32 {
        1.0 / self.sample_rate
    }

    fn seconds_to_samples(&self, seconds: f32) -> usize {
        (seconds * self.sample_rate) as usize
    }

    fn debug_events_in(&mut self, events: &api::Events) {
        for e in events.events() {
            DebugSocket::send(
                &*(messages::format_event(&e)
                    + &*format!(" current time={}", self.current_time_in_samples)),
            );
        }
    }

    fn increase_time_in_samples(&mut self, samples: usize) {
        let new_time_in_samples = self.current_time_in_samples + samples;

        // tick every second in the debug socket
        // let old_time_in_seconds = self.seconds_per_sample() * self.current_time_in_samples as f32;
        // let new_time_in_seconds = self.seconds_per_sample() * new_time_in_samples as f32;
        //
        // if old_time_in_seconds.trunc() != new_time_in_seconds.trunc() {
        //     self.parameters.debug(&*format!("{}s", new_time_in_seconds));
        // }
        self.current_time_in_samples = new_time_in_samples;
    }
}

impl Plugin for NoteOffDelayPlugin {
    fn get_info(&self) -> Info {
        Info {
            name: "Note Off Delay".to_string(),
            vendor: "DJ Crontab".to_string(),
            unique_id: 234213173,
            parameters: 2,
            category: Category::Effect,
            initial_delay: 0,
            version: 1,
            inputs: 0,
            outputs: 0,
            midi_inputs: 1,
            f64_precision: false,
            presets: 1,
            midi_outputs: 1,
            preset_chunks: true,
            silent_when_stopped: true,
        }
    }

    fn new(host: HostCallback) -> Self {
        let parameters = NoteOffDelayPluginParameters::new(host);
        DebugSocket::send(
            build_info::format!("{{{} v{} built with {} at {}}}", $.crate_info.name, $.crate_info.version, $.compiler, $.timestamp),
        );
        NoteOffDelayPlugin {
            send_buffer: Default::default(),
            parameters: Arc::new(parameters),
            sample_rate: 44100.0,
            current_time_in_samples: 0,
            message_queue: Vec::new(),
            current_playing_notes: Default::default(),
        }
    }

    fn set_sample_rate(&mut self, rate: f32) {
        self.sample_rate = rate
    }

    fn can_do(&self, can_do: CanDo) -> vst::api::Supported {
        use vst::api::Supported::*;
        use vst::plugin::CanDo::*;

        match can_do {
            SendEvents
            | SendMidiEvent
            | ReceiveEvents
            | ReceiveMidiEvent
            | Offline
            | ReceiveTimeInfo
            | MidiKeyBasedInstrumentControl
            | Bypass => Yes,
            MidiProgramNames => No,
            ReceiveSysExEvent => Yes,
            MidiSingleNoteTuningChange => No,
            Other(s) => {
                if s == "MPE" {
                    Yes
                } else {
                    DebugSocket::send(&*s);
                    No
                }
            }
        }
    }

    fn process(&mut self, audio_buffer: &mut AudioBuffer<f32>) {
        self.send_events(audio_buffer.samples());
        self.increase_time_in_samples(audio_buffer.samples());
    }

    fn process_events(&mut self, events: &api::Events) {
        self.debug_events_in(events);

        let note_off_delay = match self
            .parameters
            .get_exponential_scale_parameter(Parameter::Delay)
        {
            Some(value) => self.seconds_to_samples(value),
            _ => 0,
        };

        let mut notes_off = AbsoluteTimeMidiMessageVector::new();

        for event in events.events() {
            // TODO: minimum time, maximum time ( with delay )

            if let Some(absolute_time_midi_message) = AbsoluteTimeMidiMessage::from_event(&event, self.current_time_in_samples) {
                let midi_message = MidiMessageType::from(&absolute_time_midi_message);
                match midi_message {
                    MidiMessageType::NoteOffMessage(_) => {
                        notes_off.insert_message(absolute_time_midi_message)
                    }
                    MidiMessageType::NoteOnMessage(_) => {
                        if let Some(delayed_note_off_position) = self.message_queue.iter().position(
                            |delayed_note_off| midi_message.is_same_note(&delayed_note_off.into())
                        ) {
                            let note_off = self.message_queue.remove(delayed_note_off_position);
                            DebugSocket::send(&*format!(
                                "removing delayed note off {}",
                                note_off
                            ));
                        }

                        self.message_queue.insert_message(absolute_time_midi_message)
                    }
                    MidiMessageType::Unsupported => {}
                    _ => {
                        self.message_queue.insert_message(absolute_time_midi_message)
                    }
                }
            }
        }

        self.message_queue.merge_notes_off(&mut notes_off, note_off_delay);
    }

    fn get_parameter_object(&mut self) -> Arc<dyn vst::plugin::PluginParameters> {
        Arc::clone(&self.parameters) as Arc<dyn vst::plugin::PluginParameters>
    }
}


#[derive(Default)]
struct CurrentPlayingNotes {
    inner: HashMap<[u8; 2], AbsoluteTimeMidiMessage>
}

impl Display for CurrentPlayingNotes {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.write_str(&*self.inner.keys().fold( String::new(), |acc, x| format!("{}, {} {}", acc, x[0], x[1].to_string())))
    }
}


impl CurrentPlayingNotes {
    fn oldest(&self) -> Option<AbsoluteTimeMidiMessage> {
        let oldest_note = match self.inner.values()
            .min_by( |a, b| a.play_time_in_samples.cmp(&b.play_time_in_samples) ) {
            None => return None,
            Some(n) => n
        };

        Some(oldest_note.clone())
    }

    fn add_message(&mut self, message: AbsoluteTimeMidiMessage, max_notes: u8) -> Option<AbsoluteTimeMidiMessage> {
        let play_time_in_samples = message.play_time_in_samples;
        let note_on : NoteOn = match MidiMessageType::from(&message) {
            MidiMessageType::NoteOnMessage(m) => m,
            _ => { return None }
        };
        self.inner.insert([note_on.get_channel(), note_on.get_pitch()], message);

        if max_notes > 0 && self.inner.len() > max_notes as usize {
            let oldest_note : NoteOn = match self.oldest() {
                None => return None,
                Some(m) => match MidiMessageType::from(&m) {
                    MidiMessageType::NoteOnMessage(m) => m,
                    _ => return None
                }
            };

            self.inner.remove_entry(&[oldest_note.get_channel(), oldest_note.get_pitch()]);

            return Some(AbsoluteTimeMidiMessage {
                data: NoteOff::from(oldest_note).into(),
                play_time_in_samples
            });
        }
        None
    }

    fn update(&mut self, messages: &[AbsoluteTimeMidiMessage], max_notes: u8) -> Vec<AbsoluteTimeMidiMessage> {
        let mut notes_off: Vec<AbsoluteTimeMidiMessage> = Vec::new();

        for message in messages {
            match MidiMessageType::from(message) {
                MidiMessageType::NoteOffMessage(m) => {
                    self.inner.remove(&[m.get_channel(), m.get_pitch()]);
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

struct DelayedMessageConsumer<'a> {
    samples_in_buffer: usize,
    messages: &'a mut AbsoluteTimeMidiMessageVector,
    current_time_in_samples: usize,
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
