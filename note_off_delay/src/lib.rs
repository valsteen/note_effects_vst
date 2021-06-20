mod parameters;
mod tests;

#[macro_use]
extern crate vst;

use log::info;
use std::cell::RefCell;
use std::sync::Arc;

use vst::api::Events;
use vst::buffer::{AudioBuffer, SendEventBuffer};
use vst::event::{Event, MidiEvent};
use vst::plugin::{CanDo, Category, HostCallback, Info, Plugin};

use crate::parameters::PARAMETER_COUNT;
use parameters::NoteOffDelayPluginParameters;
use parameters::Parameter;
use util::absolute_time_midi_message_vector::AbsoluteTimeMidiMessageVector;
use util::delayed_message_consumer::{process_scheduled_events, MessageReason};
use util::logging::logging_setup;
use util::messages::format_event;
use util::midi_message_type::MidiMessageType;
use util::parameters::ParameterConversion;
use util::absolute_time_midi_message::AbsoluteTimeMidiMessage;

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
    fn process_scheduled_events(&self, samples: usize) -> (AbsoluteTimeMidiMessageVector, Vec<MidiEvent>) {
        process_scheduled_events(
            samples,
            self.parameters.get_delay().is_active(),
            self.parameters.get_max_notes(),
            self.parameters
                .get_bool_parameter(Parameter::MaxNotesAppliesToDelayedNotesOnly),
            self.current_time_in_samples,
            &self.message_queue,
        )
    }

    fn send_events(&mut self, samples: usize) {
        let (queued_messages, events) = self.process_scheduled_events(samples);
        self.message_queue = queued_messages;

        if let Ok(mut host_callback_lock) = self.parameters.host_mutex.lock() {
            self.send_buffer
                .borrow_mut()
                .send_events(events, &mut host_callback_lock.host);
        }
    }

    #[allow(dead_code)]
    fn seconds_per_sample(&self) -> f32 {
        1.0 / self.sample_rate
    }

    #[allow(dead_code)]
    fn seconds_to_samples(&self, seconds: f32) -> usize {
        (seconds * self.sample_rate) as usize
    }

    fn debug_events_in(&mut self, events: &Events) {
        for e in events.events() {
            info!("{} current time={}", format_event(&e), self.current_time_in_samples);
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
            parameters: PARAMETER_COUNT as i32,
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
        logging_setup();
        info!(
            "{}",
            build_info::format!("{{{} v{} built with {} at {}}}", $.crate_info.name, $.crate_info.version, $.compiler, $.timestamp)
        );
        let parameters = NoteOffDelayPluginParameters::new(host);
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
                //     info!("{}", s) ;
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

        let midi_events = events.events().filter_map(
            |event| if let Event::Midi(midi_event) = event {
                Some(midi_event)
            } else {
                None
            }
        );

        for midi_event in midi_events {
            // TODO: minimum time, maximum time ( with delay )

            match MidiMessageType::from(&midi_event.data) {
                MidiMessageType::NoteOffMessage(note_off) => {
                    let note_off_play_time = midi_event.delta_frames as usize + self.current_time_in_samples;

                    self.message_queue
                        .insert_message(midi_event.data, note_off_play_time, MessageReason::Live);

                    let delay = self.parameters.get_delay();

                    if delay.is_active() {
                        let matching_note_on = self.message_queue.get_matching_note_on(note_off.channel, note_off.pitch);
                        if let Some(&AbsoluteTimeMidiMessage { play_time_in_samples,.. }) = matching_note_on {
                            let duration = note_off_play_time - play_time_in_samples;
                            let new_duration = delay.apply(duration, self.sample_rate).expect("delay is supposed to be active");

                            // send two times the note off, the live one will be only used to mark the note on as delayed
                            self.message_queue.insert_message(
                                midi_event.data,
                                play_time_in_samples + new_duration,
                                MessageReason::Delayed,
                            );
                        }
                    }
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
