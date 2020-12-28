use itertools::Itertools;
use log::{error, info};
use vst::api;
use vst::buffer::{AudioBuffer, SendEventBuffer};
use vst::event::{Event, MidiEvent};
use vst::plugin::{CanDo, Category, HostCallback, Info, Plugin};

use device_out::DeviceOut;
use util::logging::logging_setup;
use util::messages::{PitchBend, Pressure, Timbre};
use util::raw_message::RawMessage;

use crate::change::SourceChange;
use crate::device::{Device, Expression};
use crate::pattern_device::{PatternDevice, PatternDeviceChange};
use crate::timed_event::TimedEvent;
use util::midi_message_with_delta::MidiMessageWithDelta;
use std::sync::Arc;
use crate::parameters::ArpegiatorParameters;
use crate::socket::{SocketChannels, SocketCommand, create_socket_thread};
use std::thread::JoinHandle;
use std::mem::take;

pub mod pattern;
mod note;
mod device;
mod pattern_device;
mod timed_event;
mod change;
mod expressive_note;
mod device_out;
mod parameters;
mod socket;


#[macro_use]
extern crate vst;


plugin_main!(ArpegiatorPlugin);


pub struct ArpegiatorPlugin {
    events: Vec<MidiEvent>,
    send_buffer: SendEventBuffer,
    host: HostCallback,
    pattern_device_in: Device,
    notes_device_in: Device,
    pattern_device: PatternDevice,
    current_time: usize,
    device_out: DeviceOut,
    parameters: Arc<ArpegiatorParameters>,
    socket_channels: Option<SocketChannels>,
    thread_handle: Option<JoinHandle<()>>
}


impl ArpegiatorPlugin {
    fn close_socket(&mut self) {
        if let Some(socket_channels) = self.socket_channels.as_ref() {
            if let Err(e) = socket_channels.command_sender.try_send(SocketCommand::Stop) {
                error!("Error while closing note receiver channel : {:?}", e)
            }
        }

        if let Some(thread_handle) = take(&mut self.thread_handle) {
            thread_handle.join().unwrap();
        }

        self.socket_channels = None; // so the channel is not dropped before the thread is joined
    }
}


impl Default for ArpegiatorPlugin {
    fn default() -> Self {
        ArpegiatorPlugin {
            events: vec![],
            send_buffer: Default::default(),
            host: Default::default(),
            pattern_device_in: Default::default(),
            notes_device_in: Default::default(),
            pattern_device: PatternDevice::default(),
            current_time: 0,
            device_out: DeviceOut::default(),
            parameters: Arc::new(ArpegiatorParameters::new()),
            socket_channels: None,
            thread_handle: None
        }
    }
}


impl Plugin for ArpegiatorPlugin {
    fn get_info(&self) -> Info {
        Info {
            name: "Arpegiator".to_string(),
            vendor: "DJ Crontab".to_string(),
            unique_id: 342111721,
            parameters: 1,
            category: Category::Synth,
            initial_delay: 0,
            version: 1,
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
        logging_setup();
        info!("{}", build_info::format!("{{{} v{} built with {} at {}}}", $.crate_info.name, $.crate_info.version,
        $.compiler, $.timestamp));
        ArpegiatorPlugin {
            events: vec![],
            send_buffer: Default::default(),
            host,
            pattern_device_in: Default::default(),
            notes_device_in: Default::default(),
            pattern_device: Default::default(),
            current_time: 0,
            device_out: DeviceOut::default(),
            parameters: Arc::new(ArpegiatorParameters::new()),
            socket_channels: None,
            thread_handle: None
        }
    }

    fn resume(&mut self) {
        self.close_socket();

        self.current_time = 0 ;

        let (join_handle, socket_channels) = create_socket_thread();
        self.thread_handle = Some(join_handle) ;

        socket_channels.command_sender.try_send(SocketCommand::SetPort(self.parameters.get_port())).unwrap();

        if let Ok(mut socket_command) = self.parameters.socket_command.lock() {
            *socket_command = Some(socket_channels.command_sender.clone());
        }

        self.socket_channels = Some(socket_channels);
    }

    fn suspend(&mut self) {
        self.close_socket()
    }

    fn get_parameter_object(&mut self) -> Arc<dyn vst::plugin::PluginParameters> {
        Arc::clone(&self.parameters) as Arc<dyn vst::plugin::PluginParameters>
    }

    fn can_do(&self, can_do: CanDo) -> vst::api::Supported {
        use vst::api::Supported::*;
        use vst::plugin::CanDo::*;

        match can_do {
            SendEvents | SendMidiEvent | ReceiveEvents | ReceiveMidiEvent | Offline => Yes,
            Other(s) => {
                if s == "MPE" {
                    Yes
                } else {

                    Maybe
                }
            }
            _ => No,
        }
    }

    fn process(&mut self, buffer: &mut AudioBuffer<f32>) {
        let messages = match self.socket_channels.as_ref() {
            None => vec![],
            Some(socket_channels) => {
                match socket_channels.notes_receiver.try_recv() {
                    Ok(payload) => {
                        //info!("[{}] received patterns : {:02X?}", self.current_time, payload);
                        payload.messages
                    }
                    Err(_) => vec![]
                }
            }
        };

        // this avoids borrowing self
        let current_time = self.current_time;
        let pattern_device_in = &mut self.pattern_device_in;
        let pattern_device = &mut self.pattern_device;
        let notes_device_in = &mut self.notes_device_in;

        let pattern_changes = messages.into_iter().map(|message| {
            let change = pattern_device_in.update(message, current_time, None);
            let change = pattern_device.update(change);
            SourceChange::PatternChange(change)
        });

        let note_changes = self.events.iter().map(|event| {
            let midi_message_with_delta = MidiMessageWithDelta {
                delta_frames: event.delta_frames as u16,
                data: event.data,
            };

            let change = notes_device_in.update(midi_message_with_delta, current_time, None);
            SourceChange::NoteChange(change)
        });

        // merge() gets a sorted output. If a note is triggered at the same time as a pattern, note comes first in order
        // to set the pitch
        for change in pattern_changes.sorted().merge(note_changes.sorted()) {
            let delta_frames = (change.timestamp() - self.current_time) as u16;

            match change {
                SourceChange::NoteChange(_) => {
                    // TODO note changed. for now we don't change anything, it's only when a pattern starts or ends
                    // that we trigger anything
                }
                SourceChange::PatternChange(change) => {
                    match change {
                        PatternDeviceChange::AddPattern { pattern, .. } => {
                            match self.notes_device_in.notes.values().sorted().nth(pattern.index as usize) {
                                None => {}
                                Some(note) => self.device_out.push_note_on(&pattern, &note, current_time)
                            }
                        }

                        PatternDeviceChange::PatternExpressionChange { expression, pattern, .. } => {
                            let raw_message: RawMessage = match expression {
                                Expression::Timbre => {
                                    Timbre { channel: pattern.channel, value: pattern.timbre }.into()
                                }
                                Expression::Pressure => {
                                    Pressure { channel: pattern.channel, value: pattern.pressure }.into()
                                }
                                Expression::PitchBend => {
                                    PitchBend { channel: pattern.channel, millisemitones: pattern.pitchbend }.into()
                                }
                            };

                            self.device_out.update(MidiMessageWithDelta { delta_frames, data: raw_message.into() },
                                                   current_time, None);
                        }
                        PatternDeviceChange::RemovePattern { pattern, .. } => {
                            self.device_out.push_note_off(pattern.id, pattern.velocity_off,
                                                          delta_frames, current_time);
                        }
                        PatternDeviceChange::ReplacePattern { old_pattern, new_pattern, .. } => {
                            self.device_out.push_note_off(old_pattern.id, old_pattern.velocity_off,
                                                          delta_frames, current_time);

                            let note = match self.notes_device_in.notes.values().sorted().nth(new_pattern.index as usize) {
                                None => { continue; }
                                Some(note) => note
                            };

                            self.device_out.push_note_on(&new_pattern, note, current_time);
                        }
                        PatternDeviceChange::None { .. } => {}
                    }
                }
            }

            self.device_out.flush_to(&mut self.send_buffer, &mut self.host)
        }

        self.events.clear();

        self.current_time += buffer.samples()
    }

    fn process_events(&mut self, events: &api::Events) {
        for e in events.events() {
            if let Event::Midi(e) = e {
                self.events.push(e);
            }
        }
    }
}

impl Drop for ArpegiatorPlugin {
    fn drop(&mut self) {
        self.close_socket();
    }
}
