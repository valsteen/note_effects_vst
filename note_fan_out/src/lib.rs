mod parameters;

#[macro_use]
extern crate vst;

use std::collections::HashSet;
use std::hash::{Hash, Hasher};
use std::sync::Arc;
use vst::api;
use vst::buffer::{AudioBuffer, SendEventBuffer};
use vst::event::{Event, MidiEvent};
use vst::plugin::{CanDo, Category, HostCallback, Info, Plugin};

use parameters::ChannelDistribution;
use parameters::{NoteFanoutParameters, Parameter};
use util::messages::{ChannelMessage, GenericChannelMessage, NoteMessage, NoteOff, NoteOn};
use util::midi_message_type::MidiMessageType;
use util::parameters::ParameterConversion;
use util::raw_message::RawMessage;

plugin_main!(NoteFanOut);

#[derive(Default)]
pub struct NoteFanOut {
    events: Vec<MidiEvent>,
    current_playing_notes: HashSet<PlayingNote>,
    send_buffer: SendEventBuffer,
    parameters: Arc<NoteFanoutParameters>,
    current_step: u8,
    notes_counter: usize,
}

impl NoteFanOut {
    fn send_midi(&mut self) {
        if let Ok(mut host_callback_lock) = self.parameters.host.lock() {
            self.send_buffer.send_events(&self.events, &mut host_callback_lock.host);
        }
        self.events.clear();
    }
}

#[derive(Eq, Clone)]
struct PlayingNote {
    channel: u8,
    pitch: u8,
    mapped_channel: u8,
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
            parameters: 3,
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
            Other(_) => Maybe,
            _ => Maybe,
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
                let midi_message = MidiMessageType::from(&e.data);

                match midi_message {
                    MidiMessageType::NoteOnMessage(midi_message) => {
                        let target_channel =
                            match self.parameters.get_channel_distribution(Parameter::ChannelDistribute) {
                                ChannelDistribution::Channels(distribution) => {
                                    let target_channel = (self.notes_counter % (distribution as usize)) as u8 + 1;
                                    self.notes_counter += 1;
                                    target_channel
                                }
                                ChannelDistribution::Off => GenericChannelMessage::from(&e.data).get_channel(),
                            };

                        if steps == 0 || selection == self.current_step {
                            let raw_message: RawMessage = NoteOn {
                                channel: target_channel,
                                pitch: midi_message.pitch,
                                velocity: midi_message.velocity,
                            }
                            .into();

                            self.events.push(MidiEvent {
                                data: raw_message.into(),
                                delta_frames: e.delta_frames,
                                live: e.live,
                                note_length: e.note_length,
                                note_offset: e.note_offset,
                                detune: e.detune,
                                note_off_velocity: e.note_off_velocity,
                            });

                            self.current_playing_notes.insert(PlayingNote {
                                channel: midi_message.get_channel(),
                                pitch: midi_message.get_pitch(),
                                mapped_channel: target_channel,
                            });
                        }

                        if steps > 0 {
                            self.current_step = (self.current_step + 1) % steps;
                        }
                    }
                    MidiMessageType::NoteOffMessage(midi_message) => {
                        let lookup = PlayingNote {
                            channel: midi_message.get_channel(),
                            pitch: midi_message.get_pitch(),
                            mapped_channel: midi_message.get_channel(),
                        };

                        match self.current_playing_notes.take(&lookup) {
                            Some(note) => {
                                let raw_message: RawMessage = NoteOff {
                                    channel: note.mapped_channel,
                                    pitch: midi_message.pitch,
                                    velocity: midi_message.velocity,
                                }
                                .into();

                                self.events.push(MidiEvent {
                                    data: raw_message.into(),
                                    delta_frames: e.delta_frames,
                                    live: e.live,
                                    note_length: e.note_length,
                                    note_offset: e.note_offset,
                                    detune: e.detune,
                                    note_off_velocity: e.note_off_velocity,
                                });
                            }
                            None => {
                                self.events.push(e);
                            }
                        }
                    }
                    _ => {
                        self.events.push(e);
                    }
                }
            }
        }
    }

    fn get_parameter_object(&mut self) -> Arc<dyn vst::plugin::PluginParameters> {
        Arc::clone(&self.parameters) as Arc<dyn vst::plugin::PluginParameters>
    }
}
