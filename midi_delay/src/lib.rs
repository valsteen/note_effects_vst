mod parameters;

#[allow(unused_imports)]
use log::{error, info};

#[macro_use]
extern crate vst;

use std::cell::RefCell;
use std::sync::Arc;
use vst::api::Events;
use vst::buffer::{AudioBuffer, SendEventBuffer};
use vst::event::Event;
use vst::plugin::{CanDo, Category, HostCallback, Info, Plugin, PluginParameters};

use parameters::{MidiDelayParameters, Parameter};
use util::absolute_time_midi_message_vector::AbsoluteTimeMidiMessageVector;
use util::delayed_message_consumer::raw_process_scheduled_events;
use util::parameters::ParameterConversion;
use vst::host::Host;
use util::logging::logging_setup;
use util::SyncDuration;

plugin_main!(MidiDelay);

pub struct MidiDelay {
    current_time_in_samples: usize,
    message_queue: AbsoluteTimeMidiMessageVector,
    parameters: Arc<MidiDelayParameters>,
    sample_rate: f32,
    bpm: f64,
    send_buffer: RefCell<SendEventBuffer>,
}

impl Default for MidiDelay {
    fn default() -> Self {
        MidiDelay {
            current_time_in_samples: 0,
            message_queue: Default::default(),
            parameters: Arc::new(Default::default()),
            sample_rate: 44100.0,
            bpm: 0.0,
            send_buffer: Default::default(),
        }
    }
}

impl MidiDelay {
    fn increase_time_in_samples(&mut self, samples: usize) {
        let new_time_in_samples = self.current_time_in_samples + samples;
        self.current_time_in_samples = new_time_in_samples;
    }

    #[allow(dead_code)]
    fn samples_to_seconds(&self) -> f32 {
        1.0 / self.sample_rate
    }

    fn seconds_to_samples(&self, seconds: f32) -> usize {
        (seconds * self.sample_rate) as usize
    }

    fn send_events(&mut self, samples: usize) {
        if let Ok(mut host_callback_lock) = self.parameters.host.lock() {
            let (next_message_queue, events) = raw_process_scheduled_events(
                samples,
                self.current_time_in_samples,
                &self.message_queue,
            );

            self.message_queue = next_message_queue;
            self.send_buffer
                .borrow_mut()
                .send_events(events, &mut host_callback_lock.host);
        }
    }

    fn update_bpm(&mut self) {
        use vst::api::TimeInfoFlags;
        if let Ok(host_callback_lock) = self.parameters.host.lock() {
            match host_callback_lock.host.get_time_info(TimeInfoFlags::TEMPO_VALID.bits()) {
                None => (),
                Some(ti) => {
                    if ti.flags & TimeInfoFlags::TEMPO_VALID.bits() != 0 {
                        self.bpm = ti.tempo;
                        info!("{}", self.bpm);
                    }
                }
            }
        }
    }
}

impl Plugin for MidiDelay {
    fn get_info(&self) -> Info {
        Info {
            name: "Midi Delay".to_string(),
            vendor: "DJ Crontab".to_string(),
            unique_id: 133498,
            parameters: 2,
            category: Category::Effect,
            initial_delay: 0,
            version: 2,
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
        info!("{}", build_info::format!("{{{} v{} built with {} at {}}}", $.crate_info.name, $.crate_info.version, $.compiler, $.timestamp));
        let parameters = MidiDelayParameters::new(host);

        MidiDelay {
            current_time_in_samples: 0,
            message_queue: Default::default(),
            parameters: Arc::new(parameters),
            sample_rate: 44100.0,
            bpm: 0.0,
            send_buffer: Default::default(),
        }
    }

    fn can_do(&self, can_do: CanDo) -> vst::api::Supported {
        use vst::api::Supported::*;
        use vst::plugin::CanDo::*;

        match can_do {
            SendEvents | SendMidiEvent | ReceiveEvents | ReceiveMidiEvent | Offline | Bypass => Yes,
            MidiProgramNames | ReceiveSysExEvent | MidiSingleNoteTuningChange => No,
            Other(_) => Maybe,
            _ => Maybe,
        }
    }

    fn get_parameter_object(&mut self) -> Arc<dyn vst::plugin::PluginParameters> {
        Arc::clone(&self.parameters) as Arc<dyn vst::plugin::PluginParameters>
    }

    fn set_sample_rate(&mut self, rate: f32) {
        self.sample_rate = rate
    }

    fn process(&mut self, audio_buffer: &mut AudioBuffer<f32>) {
        self.send_events(audio_buffer.samples());
        self.increase_time_in_samples(audio_buffer.samples());
    }

    fn process_events(&mut self, events: &Events) {
        self.update_bpm();
        let mut midi_delay = self.seconds_to_samples(self.parameters.get_exponential_scale_parameter(
            Parameter::Delay,
            1.,
            80.,
        ));
        let sync_delay = SyncDuration::from(self.parameters.get_parameter(Parameter::SyncDelay.into()));
        match sync_delay.delay_to_samples(self.bpm, self.sample_rate) {
            None => {}
            Some(delay) => {
                midi_delay += delay
            }
        }
        for event in events.events() {
            let midi_event = if let Event::Midi(midi_event) = event {
                midi_event
            } else {
                continue;
            };

            self.message_queue.raw_insert(
                midi_event.data,
                midi_event.delta_frames as usize + self.current_time_in_samples + midi_delay,
            );
        }
    }
}
