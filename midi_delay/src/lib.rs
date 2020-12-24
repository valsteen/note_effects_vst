mod parameters;

#[macro_use]
extern crate vst;

use vst::api;
use vst::buffer::{AudioBuffer, SendEventBuffer};
use vst::plugin::{CanDo, Category, HostCallback, Info, Plugin};
use std::sync::Arc;
use std::cell::RefCell;

use parameters::{MidiDelayParameters, Parameter};
use util::parameters::ParameterConversion;
use util::midi_message_type::MidiMessageType;
use util::absolute_time_midi_message_vector::AbsoluteTimeMidiMessageVector;
use util::delayed_message_consumer::DelayedMessageConsumer;
use util::absolute_time_midi_message::AbsoluteTimeMidiMessage;


plugin_main!(MidiDelay);


pub struct MidiDelay {
    current_time_in_samples: usize,
    message_queue: AbsoluteTimeMidiMessageVector,
    parameters: Arc<MidiDelayParameters>,
    sample_rate: f32,
    send_buffer: RefCell<SendEventBuffer>,
}


impl Default for MidiDelay {
    fn default() -> Self {
        MidiDelay {
            current_time_in_samples: 0,
            message_queue: Default::default(),
            parameters: Arc::new(Default::default()),
            sample_rate: 44100.0,
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
    fn seconds_per_sample(&self) -> f32 {
        1.0 / self.sample_rate
    }

    fn seconds_to_samples(&self, seconds: f32) -> usize {
        (seconds * self.sample_rate) as usize
    }

    fn send_events(&mut self, samples: usize) {
        if let Ok(mut host_callback_lock) = self.parameters.host.lock() {
            let message_consumer: DelayedMessageConsumer = DelayedMessageConsumer {
                samples_in_buffer: samples,
                messages: &mut self.message_queue,
                current_time_in_samples: self.current_time_in_samples,
                drop_late_events: false
            };

            let messages: Vec<AbsoluteTimeMidiMessage> = message_consumer.collect();

            self.send_buffer.borrow_mut()
                .send_events(messages.iter().map(|e| e.new_midi_event(self.current_time_in_samples) ), &mut host_callback_lock.host);
        }
    }
}


impl Plugin for MidiDelay {
    fn get_info(&self) -> Info {
        Info {
            name: "Midi Delay".to_string(),
            vendor: "DJ Crontab".to_string(),
            unique_id: 133498,
            parameters: 1,
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
        let parameters = MidiDelayParameters::new(host);

        MidiDelay {
            current_time_in_samples: 0,
            message_queue: Default::default(),
            parameters: Arc::new(parameters),
            sample_rate: 44100.0,
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
            _ => Maybe
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

    fn process_events(&mut self, events: &api::Events) {
        let midi_delay = match self
            .parameters
            .get_exponential_scale_parameter(Parameter::Delay, 1., 80.)
        {
            Some(value) => self.seconds_to_samples(value),
            _ => 0,
        };

        for event in events.events() {
            if let Some(mut absolute_time_midi_message) = AbsoluteTimeMidiMessage::from_event(&event, self.current_time_in_samples) {
                let midi_message = MidiMessageType::from(&absolute_time_midi_message);
                match midi_message {
                    MidiMessageType::UnsupportedChannelMessage(_) | MidiMessageType::Unsupported => {}
                    _ => {
                        absolute_time_midi_message.play_time_in_samples += midi_delay;
                        self.message_queue.insert_message(absolute_time_midi_message);
                    }
                }
            }
        }
    }
}
