mod parameters;

#[macro_use]
extern crate vst;

use vst::api;
use vst::buffer::{AudioBuffer, SendEventBuffer};
use vst::event::{Event, MidiEvent};
use vst::plugin::{CanDo, Category, HostCallback, Info, Plugin};
use std::sync::Arc;
use util::parameters::ParameterConversion;
use util::messages::{MidiMessageType, RawMessage, ChannelMessage, NoteMessage};
use parameters::{NoteFanoutParameters, Parameter};
use std::collections::HashSet;
use std::hash::{Hasher, Hash};


plugin_main!(NoteFanOut);


#[derive(Default)]
pub struct NoteFanOut {
    events: Vec<MidiEvent>,
    current_playing_notes: HashSet<PlayingNote>,
    send_buffer: SendEventBuffer,
    parameters: Arc<NoteFanoutParameters>,
    current_step: u8
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

#[derive(Eq)]
struct PlayingNote {
    channel: u8,
    pitch: u8
}

impl PartialEq for PlayingNote {
    fn eq(&self, other: &Self) -> bool {
        self.channel == other.channel && self.pitch == other.pitch
    }
}

impl Hash for PlayingNote {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.channel.hash(state);
        self.pitch.hash(state);
    }
}

impl Plugin for NoteFanOut {
    fn get_info(&self) -> Info {
        Info {
            name: "Note fan-out".to_string(),
            vendor: "DJ Crontab".to_string(),
            unique_id: 123458,
            parameters: 2,
            category: Category::Effect,
            initial_delay: 0,
            version: 5,
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
            parameters: Arc::new(NoteFanoutParameters::new(host)),
            ..Default::default()
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

        for e in events.events() {
            if let Event::Midi(e) = e {
                if steps > 0 {
                    let raw_message = RawMessage::from(e.data);
                    let midi_message = MidiMessageType::from(raw_message);

                    match midi_message {
                        MidiMessageType::NoteOnMessage(midi_message) => {
                            if selection == self.current_step {
                                self.events.push(e);
                                self.current_playing_notes.insert(PlayingNote {
                                    channel: midi_message.get_channel(),
                                    pitch: midi_message.get_pitch()
                                });
                            }
                            self.current_step = (self.current_step + 1) % steps ;
                        }
                        MidiMessageType::NoteOffMessage(midi_message) => {
                            let lookup = PlayingNote {
                                channel: midi_message.get_channel(),
                                pitch: midi_message.get_pitch(),
                            };
                            if self.current_playing_notes.contains(&lookup) {
                                self.current_playing_notes.remove(&lookup);
                                self.events.push(e);
                            }
                        }
                        _ => {
                            self.events.push(e);
                        }
                    }
                } else {
                    self.events.push(e);
                }
            }
        }
    }

    fn get_parameter_object(&mut self) -> Arc<dyn vst::plugin::PluginParameters> {
        Arc::clone(&self.parameters) as Arc<dyn vst::plugin::PluginParameters>
    }
}
