#[macro_use]
extern crate vst;

use std::sync::Arc;

use vst::api;
use vst::buffer::{AudioBuffer, SendEventBuffer};
use vst::event::{Event, MidiEvent};
use vst::plugin::{CanDo, Category, HostCallback, Info, Plugin};

mod parameters;

use crate::parameters::NoteGeneratorPluginParameters;
use util::make_midi_event;
use util::parameter_value_conversion::f32_to_bool;

plugin_main!(NoteGeneratorPlugin);

const PRESSURE: u8 = 0xD0;
const PITCHWHEEL: u8 = 0xE0;
const ZEROVALUE: u8 = 0x40;
const CC: u8 = 0xB0;
const TIMBRECC: u8 = 0x4A;
const NOTE_OFF: u8 = 0x80;
const NOTE_ON: u8 = 0x90;

#[derive(Default)]
pub struct NoteGeneratorPlugin {
    events: Vec<MidiEvent>,
    send_buffer: SendEventBuffer,
    parameters: Arc<parameters::NoteGeneratorPluginParameters>,
}

impl NoteGeneratorPlugin {
    fn get_midi_channel_event(
        &self,
        event_type: u8,
        channel_parameter: i32,
        pitch_parameter: i32,
        velocity_parameter: i32,
        delta: i32,
    ) -> MidiEvent {
        make_midi_event(
            [
                event_type + self.parameters.get_byte_parameter(channel_parameter) / 8,
                self.parameters.get_byte_parameter(pitch_parameter),
                self.parameters.get_byte_parameter(velocity_parameter),
            ],
            delta,
        )
    }

    fn get_current_note_on(&self, delta: i32) -> MidiEvent {
        self.get_midi_channel_event(
            NOTE_ON,
            NoteGeneratorPluginParameters::CHANNEL,
            NoteGeneratorPluginParameters::PITCH,
            NoteGeneratorPluginParameters::VELOCITY,
            delta,
        )
    }

    fn get_current_note_off(&self, delta: i32) -> MidiEvent {
        self.get_midi_channel_event(
            NOTE_OFF,
            NoteGeneratorPluginParameters::TRIGGERED_CHANNEL,
            NoteGeneratorPluginParameters::TRIGGERED_PITCH,
            NoteGeneratorPluginParameters::NOTE_OFF_VELOCITY,
            delta,
        )
    }

    fn get_current_pressure(&self, delta: i32) -> MidiEvent {
        make_midi_event(
            [
                PRESSURE
                    + self
                        .parameters
                        .get_byte_parameter(NoteGeneratorPluginParameters::CHANNEL)
                        / 8,
                self.parameters
                    .get_byte_parameter(NoteGeneratorPluginParameters::PRESSURE),
                0,
            ],
            delta,
        )
    }

    fn get_current_timber(&self, delta: i32) -> MidiEvent {
        make_midi_event(
            [
                CC + self
                    .parameters
                    .get_byte_parameter(NoteGeneratorPluginParameters::CHANNEL)
                    / 8,
                TIMBRECC,
                ZEROVALUE,
            ],
            delta,
        )
    }

    fn get_current_pitchwheel(&self, delta: i32) -> MidiEvent {
        make_midi_event(
            [
                PITCHWHEEL
                    + self
                        .parameters
                        .get_byte_parameter(NoteGeneratorPluginParameters::CHANNEL)
                        / 8,
                0,
                ZEROVALUE,
            ],
            delta,
        )
    }

    fn send_midi(&mut self) {
        for (index, value) in self.parameters.transfer.iterate(true) {
            match index as i32 {
                NoteGeneratorPluginParameters::PRESSURE => {
                    self.events.push(self.get_current_pressure(0));
                }

                NoteGeneratorPluginParameters::TRIGGER => {
                    if f32_to_bool(value) {
                        self.parameters.copy_parameter(
                            NoteGeneratorPluginParameters::CHANNEL,
                            NoteGeneratorPluginParameters::TRIGGERED_CHANNEL,
                        );
                        self.parameters.copy_parameter(
                            NoteGeneratorPluginParameters::PITCH,
                            NoteGeneratorPluginParameters::TRIGGERED_PITCH,
                        );
                        self.events.push(self.get_current_note_on(0));

                        // delay those expressions with delta frames, seem to do the trick
                        // even though bitwig always inserts zero values for those before the
                        // note, so it always need to be sent right after to obtain the
                        // desired state
                        self.events.push(self.get_current_pitchwheel(1));
                        self.events.push(self.get_current_timber(1));
                        self.events.push(self.get_current_pressure(1));
                    } else {
                        self.events.push(self.get_current_note_off(0));
                    }
                }
                _ => (),
            }
        }

        if let Ok(mut host_callback_lock) = self.parameters.host.lock() {
            self.send_buffer
                .send_events(&self.events, &mut host_callback_lock.host);
        }
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
        NoteGeneratorPlugin {
            parameters: Arc::new(NoteGeneratorPluginParameters::new(host)),
            ..Default::default()
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
                self.events.push(e);
            }
        }
    }

    fn get_parameter_object(&mut self) -> Arc<dyn vst::plugin::PluginParameters> {
        Arc::clone(&self.parameters) as Arc<dyn vst::plugin::PluginParameters>
    }
}
