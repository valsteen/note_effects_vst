mod parameters;

#[macro_use]
extern crate vst;

use vst::api;
use vst::buffer::{AudioBuffer, SendEventBuffer};
use vst::event::{Event, MidiEvent};
use vst::plugin::{CanDo, Category, HostCallback, Info, Plugin};
use std::sync::Arc;
use util::parameters::ParameterConversion;

use parameters::{NoteFanoutParameters, Parameter};


plugin_main!(NoteFanOut);

pub struct NoteFanOut {
    events: Vec<MidiEvent>,
    send_buffer: SendEventBuffer,
    parameters: Arc<NoteFanoutParameters>,
}


impl Default for NoteFanOut {
    fn default() -> Self {
        NoteFanOut {
            events: vec![],
            send_buffer: Default::default(),
            parameters: Arc::new(Default::default()),
        }
    }
}

impl NoteFanOut {
    fn send_midi(&mut self) {
        if let Ok(mut host_callback_lock) = self.parameters.host.lock() {
            self.send_buffer
                .send_events(&self.events, &mut host_callback_lock.host);
        }
        self.events.clear();
    }
}

impl Plugin for NoteFanOut {
    fn get_info(&self) -> Info {
        Info {
            name: "Note fan-out".to_string(),
            vendor: "DJ Crontab".to_string(),
            unique_id: 342111711,
            parameters: 2,
            category: Category::Effect,
            initial_delay: 0,
            version: 3,
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
        NoteFanOut {
            events: vec![],
            send_buffer: Default::default(),
            parameters: Arc::new(NoteFanoutParameters::new(host)),
        }
    }

    fn can_do(&self, can_do: CanDo) -> vst::api::Supported {
        use vst::api::Supported::*;
        use vst::plugin::CanDo::*;

        match can_do {
            SendEvents | SendMidiEvent | ReceiveEvents | ReceiveMidiEvent | Offline | Bypass => Yes,
            MidiProgramNames | ReceiveSysExEvent | MidiSingleNoteTuningChange => No,
            Other(s) => {
                if s == "MPE" {
                    Yes
                } else {
                    Maybe
                }
            }
            _ => Maybe
        }
    }

    fn process(&mut self, _: &mut AudioBuffer<f32>) {
        self.send_midi();
    }

    fn process_events(&mut self, events: &api::Events) {
        let steps = self.parameters.get_byte_parameter(Parameter::Steps) / 8;
        let selection = self.parameters.get_byte_parameter(Parameter::Selection) / 8;
        let mut current_step = self.parameters.get_byte_parameter(Parameter::CurrentStep) ;

        for e in events.events() {
            if let Event::Midi(e) = e {
                if e.data[0] >= 0x80 && e.data[0] <= 0x9F {
                    if selection == current_step {
                        self.events.push(e);
                    }

                    current_step = (current_step + 1) % steps ;
                } else {
                    self.events.push(e);
                }
            }
        }

        self.parameters.set_byte_parameter(Parameter::CurrentStep, current_step);
    }

    fn get_parameter_object(&mut self) -> Arc<dyn vst::plugin::PluginParameters> {
        Arc::clone(&self.parameters) as Arc<dyn vst::plugin::PluginParameters>
    }
}
