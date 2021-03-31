mod parameters;

#[macro_use]
extern crate vst;

use std::cell::RefCell;
use std::sync::Arc;

use vst::api::Events;
use vst::buffer::{AudioBuffer, SendEventBuffer};
use vst::event::Event;
use vst::plugin::{CanDo, Category, HostCallback, Info, Plugin, PluginParameters};

use parameters::NoteOffDelayPluginParameters;
use parameters::Parameter;
use util::absolute_time_midi_message_vector::AbsoluteTimeMidiMessageVector;
use util::debug::DebugSocket;
use util::delayed_message_consumer::{process_scheduled_events, MessageReason};
use util::messages::format_event;
use util::midi_message_type::MidiMessageType;
use util::parameters::ParameterConversion;
use crate::parameters::Delay;

plugin_main!(NoteOffDelayPlugin);

pub struct NoteOffDelayPlugin {
    current_time_in_samples: usize,
    message_queue: AbsoluteTimeMidiMessageVector,
    parameters: Arc<NoteOffDelayPluginParameters>,
    sample_rate: f32,
    send_buffer: RefCell<SendEventBuffer>,
}

impl Default for NoteOffDelayPlugin {
    fn default() -> Self {
        NoteOffDelayPlugin {
            send_buffer: Default::default(),
            parameters: Arc::new(Default::default()),
            sample_rate: 44100.0,
            current_time_in_samples: 0,
            message_queue: Default::default(),
        }
    }
}

impl NoteOffDelayPlugin {
    fn send_events(&mut self, samples: usize) {
        if let Ok(mut host_callback_lock) = self.parameters.host_mutex.lock() {
            let (next_message_queue, events) = process_scheduled_events(
                samples,
                self.current_time_in_samples,
                &self.message_queue,
                self.parameters.get_max_notes(),
                self.parameters
                    .get_bool_parameter(Parameter::MaxNotesAppliesToDelayedNotesOnly),
                self.parameters.get_delay().is_active(),
            );

            self.message_queue = next_message_queue;
            self.send_buffer
                .borrow_mut()
                .send_events(events, &mut host_callback_lock.host);
        }
    }

    #[allow(dead_code)]
    fn seconds_per_sample(&self) -> f32 {
        1.0 / self.sample_rate
    }

    fn seconds_to_samples(&self, seconds: f32) -> usize {
        (seconds * self.sample_rate) as usize
    }

    fn debug_events_in(&mut self, events: &Events) {
        for e in events.events() {
            DebugSocket::send(&*(format_event(&e) + &*format!(" current time={}", self.current_time_in_samples)));
        }
    }

    fn increase_time_in_samples(&mut self, samples: usize) {
        self.current_time_in_samples += samples;
    }
}

impl Plugin for NoteOffDelayPlugin {
    fn get_info(&self) -> Info {
        Info {
            name: "Note Off Delay".to_string(),
            vendor: "DJ Crontab".to_string(),
            unique_id: 234213173,
            parameters: 3,
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
            current_time_in_samples: 0,
            message_queue: Default::default(),
            parameters: Arc::new(parameters),
            sample_rate: 44100.0,
            send_buffer: Default::default(),
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
            Other(_) => {
                // Bitwig will mark it as "MPE" by default if 'Yes', but somehow either there is a
                // bug here, or bitwig ends up being confused about midi events coming out of VSTs,
                // and some notes end up still running. As it's not really useful in this context,
                // let the feature off.
                // if s == "MPE" {
                //     Yes
                // } else {
                //     DebugSocket::send(&*s);
                //     No
                // }
                Maybe
            }
        }
    }

    fn process(&mut self, audio_buffer: &mut AudioBuffer<f32>) {
        self.send_events(audio_buffer.samples());
        self.increase_time_in_samples(audio_buffer.samples());
    }

    fn process_events(&mut self, events: &Events) {
        self.debug_events_in(events);

        for event in events.events() {
            let midi_event = if let Event::Midi(midi_event) = event {
                midi_event
            } else {
                continue;
            };

            // TODO: minimum time, maximum time ( with delay )

            match MidiMessageType::from(&midi_event.data) {
                MidiMessageType::NoteOffMessage(_) => {
                    self.message_queue.insert_message(
                        midi_event.data,
                        midi_event.delta_frames as usize + self.current_time_in_samples,
                        MessageReason::Live,
                    );

                    if let Delay::Duration(seconds) = self.parameters.get_delay() {
                        let delay_in_samples = self.seconds_to_samples(seconds);
                        // send two times the note off, the live one will be only used to mark the note on as delayed
                        self.message_queue.insert_message(
                            midi_event.data,
                            delay_in_samples + midi_event.delta_frames as usize + self.current_time_in_samples,
                            MessageReason::Delayed,
                        );
                    } ;
                }
                MidiMessageType::Unsupported => {
                    continue;
                }
                _ => {
                    self.message_queue.insert_message(
                        midi_event.data,
                        midi_event.delta_frames as usize + self.current_time_in_samples,
                        MessageReason::Live,
                    );
                }
            };
        }
    }

    fn get_parameter_object(&mut self) -> Arc<dyn vst::plugin::PluginParameters> {
        Arc::clone(&self.parameters) as Arc<dyn vst::plugin::PluginParameters>
    }
}
