#[macro_use]
extern crate vst;
#[macro_use]
extern crate primitive_enum;

use std::sync::Arc;

use vst::api;
use vst::buffer::{AudioBuffer, SendEventBuffer};
use vst::event::{Event, MidiEvent};
use vst::plugin::{CanDo, HostCallback, Info, Plugin, Category, PluginParameters};

mod util;
mod parameters;

use crate::parameters::{Parameter, NoteGeneratorPluginParameters};
use crate::util::parameter_value_conversion::f32_to_bool;


plugin_main!(NoteGeneratorPlugin);

const PRESSURE: u8 = 0xD0;
const PITCHWHEEL: u8 = 0xE0;
const ZEROVALUE: u8 = 0x40;
const CC: u8 = 0xB0;
const TIMBRECC: u8 = 0x4A;
const NOTE_OFF: u8 = 0x80;
const NOTE_ON: u8 = 0x90;


#[derive(Default)]
pub struct HostCallbackLock {
    host: HostCallback
}

#[derive(Default)]
pub struct NoteGeneratorPlugin {
    events: Vec<MidiEvent>,
    // next_events are sent at next process call, in order to adjust expressive attributes
    // ( pitch wheel, pressure, timber ) after the note on event. otherwise bitwig may ignore
    // those events and override them if they happen from the same process() call.
    next_events: Vec<MidiEvent>,
    send_buffer: SendEventBuffer,
    parameters: Arc<parameters::NoteGeneratorPluginParameters>,
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

    fn get_midi_channel_event(&self, event_type: u8, channel_parameter: Parameter,
                              pitch_parameter: Parameter, velocity_parameter: Parameter) -> MidiEvent {
        NoteGeneratorPlugin::make_midi_event([
            event_type + self.parameters.get_byte_parameter(channel_parameter) / 8,
            self.parameters.get_byte_parameter(pitch_parameter),
            self.parameters.get_byte_parameter(velocity_parameter)
        ])
    }

    fn get_current_note_on(&self) -> MidiEvent {
        self.get_midi_channel_event(NOTE_ON,
                                    Parameter::Channel,
                                    Parameter::Pitch,
                                    Parameter::Velocity)
    }

    fn get_current_note_off(&self) -> MidiEvent {
        self.get_midi_channel_event(NOTE_OFF,
                                    Parameter::TriggeredChannel,
                                    Parameter::TriggeredPitch,
                                    Parameter::NoteOffVelocity)
    }

    fn get_current_pressure(&self) -> MidiEvent {
        NoteGeneratorPlugin::make_midi_event(
            [PRESSURE + self.parameters.get_byte_parameter(Parameter::Channel) / 8,
                self.parameters.get_byte_parameter(Parameter::Pressure), 0]
        )
    }

    fn get_current_timber(&self) -> MidiEvent {
        NoteGeneratorPlugin::make_midi_event([
            CC + self.parameters.get_byte_parameter(Parameter::Channel) / 8,
            TIMBRECC, ZEROVALUE])
    }

    fn get_current_pitchwheel(&self) -> MidiEvent {
        NoteGeneratorPlugin::make_midi_event([
            PITCHWHEEL + self.parameters.get_byte_parameter(Parameter::Channel) / 8,
            0, ZEROVALUE]
        )
    }

    fn send_midi(&mut self) {
        for (index, value) in self.parameters.transfer.iterate(true) {
            match Parameter::from(index as i32) {
                Some(parameter) => match parameter {
                    Parameter::Pressure => {
                        self.events.push(self.get_current_pressure());
                    }

                    Parameter::Trigger => {
                        if f32_to_bool(value) {
                            /*
                            At note on, don't send pressure/timber/pitch wheel yet, just the
                            Note On ; upon receiving the Note On bitwig will send those
                            messages with zero values right before the Note On message anyway
                             */
                            self.parameters.copy_parameter(Parameter::Channel,
                                                           Parameter::TriggeredChannel);
                            self.parameters.copy_parameter(Parameter::Pitch,
                                                           Parameter::TriggeredPitch);
                            self.events.push(self.get_current_note_on());
                            /*
                            Sending those events at the next process() call makes sure they are
                            actually sent to the instrument in order to apply note expression.

                            Delaying those events is the behaviour of a ROLI seaboard talking
                            to Bitwig.
                             */
                            self.next_events.push(self.get_current_pitchwheel());
                            self.next_events.push(self.get_current_timber());
                            self.next_events.push(self.get_current_pressure());
                        } else {
                            self.events.push(self.get_current_note_off());
                        }
                    }
                    _ => ()
                },
                _ => {}
            }
        }

        if let Ok(mut host_callback_lock) = self.parameters.host.lock() {
            self.send_buffer.send_events(&self.events, &mut host_callback_lock.host);
        }
        self.events.clear();

        // plan those events for the next process() call. This is in order to make sure note
        // expressions are taken into account, if sending in the same call as Note On it seems
        // bitwig cancels them out with its own Note On messages
        for next_event in self.next_events.drain(..) {
            self.events.push(next_event)
        }
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
            presets: 1,
            midi_outputs: 0,
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

    fn get_parameter_object(&mut self) -> Arc<dyn PluginParameters> {
        Arc::clone(&self.parameters) as Arc<dyn PluginParameters>
    }
}
