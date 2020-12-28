#[macro_use]
extern crate vst;

use log::info;

use vst::api;
use vst::buffer::{AudioBuffer, SendEventBuffer};
use vst::event::{Event, MidiEvent};
use vst::plugin::{CanDo, Category, HostCallback, Info, Plugin};
use std::time::SystemTime;
use util::logging::logging_setup;
use util::transmute_buffer::{transmute_raw_buffer, transmute_raw_buffer_mut};



plugin_main!(AudioData);

pub struct AudioData {
    events: Vec<MidiEvent>,
    send_buffer: SendEventBuffer,
    host: HostCallback,
    last_was: u128,
    last_now: u128
}

impl Default for AudioData {
    fn default() -> Self {
        AudioData {
            events: vec![],
            send_buffer: Default::default(),
            host: Default::default(),
            last_was: 0,
            last_now: 0
        }
    }
}

impl AudioData {
    fn send_midi(&mut self) {
        self.send_buffer.send_events(&self.events, &mut self.host);
        self.events.clear();
    }
}


impl Plugin for AudioData {
    fn get_info(&self) -> Info {
        Info {
            name: "Audio data".to_string(),
            vendor: "DJ Crontab".to_string(),
            unique_id: 342131710,
            parameters: 0,
            category: Category::Synth,
            initial_delay: 0,
            version: 1,
            inputs: 2,
            outputs: 2,
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
        AudioData {
            events: vec![],
            send_buffer: Default::default(),
            host,
            last_was: 0,
            last_now: 0
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

    fn process(&mut self, buffer: &mut AudioBuffer<f32>) {
        self.send_midi();
        let (inputs, mut outputs) = buffer.split();
        let now = SystemTime::now().duration_since(SystemTime::UNIX_EPOCH).unwrap().as_micros();

        let input_buffer = transmute_raw_buffer(inputs.get(0));
        let was = input_buffer[0];

        if self.last_was != was {
            self.last_was = was;
            info!("difference = {} microseconds. Since last check: diff={} now={} was={}", now - was, now -
                self.last_now, now, was);
        }

        self.last_now = now;

        let output_buffer = transmute_raw_buffer_mut(outputs.get_mut(0));
        output_buffer[0] = now
    }

    fn process_events(&mut self, events: &api::Events) {
        for e in events.events() {
            if let Event::Midi(e) = e {
                self.events.push(e);
            }
        }
    }
}
