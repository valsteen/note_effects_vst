#[macro_use]
extern crate vst;

use vst::api;
use vst::buffer::{AudioBuffer, SendEventBuffer};
use vst::event::{Event, MidiEvent};
use vst::plugin::{CanDo, HostCallback, Info, Plugin, Category, PluginParameters};
use std::sync::Arc;

pub mod util;

pub use util::parameters::FloatParameter;
use crate::util::parameters::{BoolParameter, ByteParameter};


plugin_main!(NoteGeneratorPlugin);


const PRESSURE: u8 = 0xD0;
const NOTE_OFF: u8 = 0x80;
const NOTE_ON: u8 = 0x90;
const C0: i8 = 0x18;
static NOTE_NAMES: &[&str; 12] = &["C", "C#", "D", "D#", "E", "F", "F#", "G", "G#", "A", "A#", "B"];


#[derive(Default)]
struct NoteGeneratorPlugin {
    host: HostCallback,
    events: Vec<MidiEvent>,
    send_buffer: SendEventBuffer,
    parameters: Arc<NoteGeneratorPluginParameters>,
}


struct NoteGeneratorPluginParameters {
    channel: ByteParameter,
    pitch: ByteParameter,
    velocity: ByteParameter,
    note_off_velocity: ByteParameter,
    pressure: ByteParameter,
    trigger: BoolParameter,
    pressure_is_modified: BoolParameter,
    trigger_is_modified: BoolParameter,
    triggered_note_channel: ByteParameter,
    triggered_note_pitch: ByteParameter,
}


impl PluginParameters for NoteGeneratorPluginParameters {
    fn get_parameter_text(&self, index: i32) -> String {
        match index {
            0 => format!("{}", self.channel.get() + 1),
            1 => format!("{}{}", NOTE_NAMES[self.pitch.get() as usize % 12], ((self.pitch.get() as i8) - C0) / 12),
            2 => format!("{}", self.velocity.get()),
            3 => format!("{}", self.note_off_velocity.get()),
            4 => format!("{}", self.pressure.get()),
            5 => format!("{}", self.trigger.get()),
            _ => "".to_string(),
        }
    }

    fn get_parameter_name(&self, index: i32) -> String {
        match index {
            0 => "Channel",
            1 => "Pitch",
            2 => "Velocity",
            3 => "Note off velocity",
            4 => "Pressure",
            5 => "Trigger generated note",
            _ => "",
        }
            .to_string()
    }

    fn set_parameter(&self, index: i32, val: f32) {
        match index {
            0 => self.channel.set_from_f32(val / 8.),
            1 => self.pitch.set_from_f32(val),
            2 => self.velocity.set_from_f32(val),
            3 => self.note_off_velocity.set_from_f32(val),
            4 => {
                self.pressure.set_from_f32(val);
                self.pressure_is_modified.set(true)
            }
            5 => {
                let old_value = self.trigger.get();
                if old_value != (val > 0.5) {
                    self.trigger.set_from_f32(val);
                    self.trigger_is_modified.set(true)
                }
            }
            _ => (),
        }
    }
}

impl Default for NoteGeneratorPluginParameters {
    fn default() -> Self {
        NoteGeneratorPluginParameters {
            channel: ByteParameter::new(0),
            pitch: Default::default(),
            velocity: Default::default(),
            note_off_velocity: Default::default(),
            pressure: ByteParameter::new(0),
            trigger: Default::default(),
            pressure_is_modified: Default::default(),
            trigger_is_modified: Default::default(),
            triggered_note_channel: Default::default(),
            triggered_note_pitch: Default::default(),
        }
    }
}

impl NoteGeneratorPlugin {
    fn make_midi_event(bytes: [u8; 3]) -> MidiEvent {
        MidiEvent {
            data: bytes,
            delta_frames: 0,
            live: true,
            note_length: None,
            note_offset: None,
            detune: 0,
            note_off_velocity: 0,
        }
    }

    fn push_midi_event(&mut self, bytes: [u8; 3]) {
        self.events.push(NoteGeneratorPlugin::make_midi_event(bytes))
    }

    fn send_midi(&mut self) {
        if self.parameters.pressure_is_modified.get() {
            self.parameters.pressure_is_modified.set(false);
            self.push_midi_event(
                [PRESSURE + self.parameters.channel.get(), self.parameters.pressure.get(), 0]
            );
        }
        if self.parameters.trigger_is_modified.get() {
            if self.parameters.trigger.get() {
                self.parameters.triggered_note_channel.set(self.parameters.channel.get());
                self.parameters.triggered_note_pitch.set(self.parameters.pitch.get());
                self.push_midi_event([
                    NOTE_ON + self.parameters.triggered_note_channel.get(),
                    self.parameters.triggered_note_pitch.get(),
                    self.parameters.velocity.get()
                ]);
            } else {
                self.push_midi_event([
                    NOTE_OFF + self.parameters.channel.get(),
                    self.parameters.pitch.get(),
                    self.parameters.note_off_velocity.get()
                ]);
            }
            self.parameters.trigger_is_modified.set(false);
        }
        self.send_buffer.send_events(&self.events, &mut self.host);
        self.events.clear();
    }
}

impl Plugin for NoteGeneratorPlugin {
    fn get_info(&self) -> Info {
        Info {
            name: "Note Generator".to_string(),
            vendor: "DJ Crontab".to_string(),
            unique_id: 234213172,
            parameters: 6,
            category: Category::Generator,
            version: 7,
            ..Default::default()
        }
    }

    fn new(host: HostCallback) -> Self {
        let mut p = NoteGeneratorPlugin::default();
        p.host = host;
        p
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
        for (input, output) in buffer.zip() {
            for (in_sample, out_sample) in input.iter().zip(output) {
                *out_sample = *in_sample;
            }
        }
        self.send_midi();
    }

    fn process_f64(&mut self, buffer: &mut AudioBuffer<f64>) {
        for (input, output) in buffer.zip() {
            for (in_sample, out_sample) in input.iter().zip(output) {
                *out_sample = *in_sample;
            }
        }
        self.send_midi();
    }

    fn process_events(&mut self, events: &api::Events) {
        for e in events.events() {
            if let Event::Midi(e) = e {
                self.events.push(e);
            }
        }
    }

    fn get_parameter_object(&mut self) -> Arc<dyn PluginParameters> {
        Arc::clone(&self.parameters) as Arc<dyn PluginParameters>
    }
}
