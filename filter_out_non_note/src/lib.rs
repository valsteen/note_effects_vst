#[macro_use]
extern crate vst;

use vst::api;
use vst::buffer::{AudioBuffer, SendEventBuffer};
use vst::event::{Event, MidiEvent};
use vst::plugin::{CanDo, Category, HostCallback, Info, Plugin};

plugin_main!(FilterOutNonNote);

pub struct FilterOutNonNote {
    events: Vec<MidiEvent>,
    send_buffer: SendEventBuffer,
    host: HostCallback,
}

impl Default for FilterOutNonNote {
    fn default() -> Self {
        FilterOutNonNote {
            events: vec![],
            send_buffer: Default::default(),
            host: Default::default(),
        }
    }
}

impl FilterOutNonNote {
    fn send_midi(&mut self) {
        self.send_buffer.send_events(&self.events, &mut self.host);
        self.events.clear();
    }
}

impl Plugin for FilterOutNonNote {
    fn get_info(&self) -> Info {
        Info {
            name: "Filter out non-note".to_string(),
            vendor: "DJ Crontab".to_string(),
            unique_id: 342131710,
            parameters: 0,
            category: Category::Effect,
            initial_delay: 0,
            version: 7,
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
        FilterOutNonNote {
            events: vec![],
            send_buffer: Default::default(),
            host,
        }
    }

    fn can_do(&self, can_do: CanDo) -> vst::api::Supported {
        use vst::api::Supported::*;
        use vst::plugin::CanDo::*;

        match can_do {
            SendEvents | SendMidiEvent | ReceiveEvents | ReceiveMidiEvent => Yes,
            _ => No,
        }
    }

    fn process(&mut self, _: &mut AudioBuffer<f32>) {
        self.send_midi();
    }

    fn process_events(&mut self, events: &api::Events) {
        for e in events.events() {
            if let Event::Midi(e) = e {
                if e.data[0] >= 0x80 && e.data[0] <= 0x9F {
                    self.events.push(e);
                }
            }
        }
    }
}
