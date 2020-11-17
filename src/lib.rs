#[macro_use] extern crate vst;
#[macro_use] extern crate primitive_enum;

use std::sync::Arc;

use vst::api;
use vst::buffer::{AudioBuffer, SendEventBuffer};
use vst::event::{Event, MidiEvent};
use vst::plugin::{CanDo, HostCallback, Info, Plugin, Category, PluginParameters};
use vst::util::ParameterTransfer;


pub mod util;

use crate::util::parameters::{f32_to_byte, byte_to_f32, f32_to_bool};


plugin_main!(NoteGeneratorPlugin);

primitive_enum! { Parameter i32 ;
    Channel,
    Pitch,
    Velocity,
    NoteOffVelocity,
    Pressure,
    Trigger,
    TriggeredPitch,
    TriggeredChannel,
}


const PRESSURE: u8 = 0xD0;
const NOTE_OFF: u8 = 0x80;
const NOTE_ON: u8 = 0x90;
const C0: i8 = 0x18;
static NOTE_NAMES: &[&str; 12] = &["C", "C#", "D", "D#", "E", "F", "F#", "G", "G#", "A", "A#", "B"];


struct NoteGeneratorPluginParameters {
    host: HostCallback,
    transfer: ParameterTransfer,
}

#[derive(Default)]
struct NoteGeneratorPlugin {
    events: Vec<MidiEvent>,
    send_buffer: SendEventBuffer,
    parameters: Arc<NoteGeneratorPluginParameters>,
}


impl NoteGeneratorPluginParameters {
    #[inline]
    fn set_byte_parameter(&self, index: Parameter, value: u8) {
        self.transfer.set_parameter(index as usize, byte_to_f32(value))
    }

    #[inline]
    fn get_byte_parameter(&self, index: Parameter) -> u8 {
        f32_to_byte(self.transfer.get_parameter(index as usize))
    }

    #[inline]
    fn get_bool_parameter(&self, index: Parameter) -> bool {
        f32_to_bool(self.transfer.get_parameter(index as usize))
    }

    #[inline]
    fn get_displayable_channel(&self) -> u8 {
        // NOT the stored value, but the one used to show on the UI
        self.get_byte_parameter(Parameter::Channel) + 1
    }

    fn get_pitch_label(&self) -> String {
        format!("{}{}",
                NOTE_NAMES[self.get_byte_parameter(Parameter::Pitch) as usize % 12],
                ((self.get_byte_parameter(Parameter::Pitch) as i8) - C0) / 12)
    }

    #[inline]
    fn get_velocity(&self) -> u8 {
        self.get_byte_parameter(Parameter::Velocity)
    }

    #[inline]
    fn get_note_off_velocity(&self) -> u8 {
        self.get_byte_parameter(Parameter::NoteOffVelocity)
    }

    #[inline]
    fn get_pressure(&self) -> u8 {
        self.get_byte_parameter(Parameter::Pressure)
    }

    #[inline]
    fn get_trigger(&self) -> bool {
        self.get_bool_parameter(Parameter::Trigger)
    }

    #[inline]
    fn set_parameter_by_name(&self, index: Parameter, value: f32) {
        self.set_parameter(index as i32, value);
    }

    #[inline]
    fn get_parameter_by_name(&self, index: Parameter) -> f32 {
        self.get_parameter(index as i32)
    }
}


impl PluginParameters for NoteGeneratorPluginParameters {
    fn get_parameter_text(&self, index: i32) -> String {
        match Parameter::from(index) {
            Some(parameter) => match parameter {
                Parameter::Channel => format!("{}", self.get_displayable_channel()),
                Parameter::Pitch => self.get_pitch_label(),
                Parameter::Velocity => format!("{}", self.get_velocity()),
                Parameter::NoteOffVelocity => format!("{}", self.get_note_off_velocity()),
                Parameter::Pressure => format!("{}", self.get_pressure()),
                Parameter::Trigger => format!("{}", self.get_trigger()),
                _ => "".to_string()
            },
            _ => "".to_string(),
        }
    }

    fn get_parameter_name(&self, index: i32) -> String {
        match Parameter::from(index) {
            Some(parameter) => match parameter {
                Parameter::Channel => "Channel",
                Parameter::Pitch => "Pitch",
                Parameter::Velocity => "Velocity",
                Parameter::NoteOffVelocity => "Note off velocity",
                Parameter::Pressure => "Pressure",
                Parameter::Trigger => "Trigger generated note",
                _ => "",
            },
            _ => ""
        }
            .to_string()
    }

    #[inline]
    fn get_parameter(&self, index: i32) -> f32 {
        self.transfer.get_parameter(index as usize)
    }

    #[inline]
    fn set_parameter(&self, index: i32, value: f32) {
        self.transfer.set_parameter(index as usize, value);
    }

    fn string_to_parameter(&self, index: i32, text: String) -> bool {
        match Parameter::from(index) {
            Some(parameter) => match parameter {
                Parameter::Channel => match text.parse::<u8>() {
                    Ok(n) => {
                        if n > 0 && n <= 16 {
                            self.set_byte_parameter(Parameter::Channel, n);
                            true
                        } else {
                            false
                        }
                    }
                    Err(_) => false
                },
                Parameter::Velocity | Parameter::NoteOffVelocity | Parameter::Pressure => {
                    match text.parse::<u8>() {
                        Ok(n) => {
                            if n < 128 {
                                self.set_byte_parameter(parameter, n);
                                true
                            } else { false }
                        }
                        Err(_) => false
                    }
                }
                Parameter::Pitch => {
                    match NOTE_NAMES.iter().position(|&s| text.starts_with(s)) {
                        None => false,
                        Some(position) => {
                            match text[NOTE_NAMES[position].len()..text.len()].parse::<i8>() {
                                Ok(octave) => {
                                    if octave >= -2 && octave <= 8 {
                                        let pitch = octave as i16 * 12 + C0 as i16 + position as i16;
                                        if pitch < 128 {
                                            self.set_byte_parameter(Parameter::Pitch, pitch as u8);
                                            true
                                        } else { false }
                                    } else { false }
                                }
                                Err(_) => false
                            }
                        }
                    }
                }
                Parameter::Trigger => {
                    match text.to_ascii_lowercase().as_ref() {
                        "0" | "off" | "" => {
                            self.set_byte_parameter(Parameter::Trigger, 0);
                            true
                        }
                        "1" | "on" => {
                            self.set_byte_parameter(Parameter::Trigger, 1);
                            true
                        }
                        _ => false
                    }
                }
                _ => false
            },
            _ => false
        }
    }
}

impl Default for NoteGeneratorPluginParameters {
    fn default() -> Self {
        let parameters = NoteGeneratorPluginParameters {
            host: Default::default(),
            transfer: ParameterTransfer::new(8),
        };
        parameters.set_byte_parameter(Parameter::Pitch, C0 as u8);
        parameters.set_byte_parameter(Parameter::Velocity, 64);
        parameters
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

    fn get_current_note_on(&self) -> MidiEvent {
        self.parameters.set_parameter_by_name(
            Parameter::TriggeredChannel,
            self.parameters.get_parameter_by_name(Parameter::Channel));
        self.parameters.set_parameter_by_name(
            Parameter::TriggeredPitch,
            self.parameters.get_parameter_by_name(Parameter::TriggeredPitch));
        NoteGeneratorPlugin::make_midi_event([
            NOTE_ON + self.parameters.get_byte_parameter(Parameter::Channel),
            self.parameters.get_byte_parameter(Parameter::Pitch),
            self.parameters.get_byte_parameter(Parameter::Velocity)
        ])
    }

    fn get_current_note_off(&self) -> MidiEvent {
        NoteGeneratorPlugin::make_midi_event([
            NOTE_OFF + self.parameters.get_byte_parameter(Parameter::TriggeredChannel),
            self.parameters.get_byte_parameter(Parameter::TriggeredPitch),
            self.parameters.get_byte_parameter(Parameter::NoteOffVelocity)
        ])
    }

    fn get_current_pressure(&self) -> MidiEvent {
        NoteGeneratorPlugin::make_midi_event(
            [PRESSURE + self.parameters.get_byte_parameter(Parameter::Channel),
                self.parameters.get_byte_parameter(Parameter::Pressure), 0]
        )
    }

    fn send_midi(&mut self) {
        for (index, value) in self.parameters.transfer.iterate(true) {
            match Parameter::from(index as i32) {
                Some(parameter) => match parameter {
                    Parameter::Pressure => {
                        self.events.push(self.get_current_pressure());
                    },
                    Parameter::Trigger => {
                        if value > 0.0 {
                            self.events.push(self.get_current_note_on());
                        } else {
                            self.events.push(self.get_current_note_off());
                        }
                    },
                    _ => ()
                },
                _ => {}
            }
        }
        self.send_buffer.send_events(&self.events, &mut Arc::get_mut(&mut self.parameters).unwrap().host);
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
            initial_delay: 0,
            version: 7,
            inputs: 0,
            outputs: 0,
            midi_inputs: 0,
            f64_precision: false,
            presets: 0,
            midi_outputs: 0,
            preset_chunks: false,
            silent_when_stopped: true,
        }
    }

    fn new(host: HostCallback) -> Self {
        NoteGeneratorPlugin {
            events: Default::default(),
            send_buffer: Default::default(),
            parameters: Arc::new(NoteGeneratorPluginParameters {
                host,
                ..Default::default()
            }),
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

    fn get_parameter_object(&mut self) -> Arc<dyn PluginParameters> {
        Arc::clone(&self.parameters) as Arc<dyn PluginParameters>
    }
}
