use itertools::Itertools;
use log::{error, info};
use vst::api;
use vst::buffer::AudioBuffer;
use vst::event::{Event, MidiEvent};
use vst::plugin::{CanDo, Category, HostCallback, Info, Plugin};

use device_out::DeviceOut;
use util::logging::logging_setup;
use util::messages::{PitchBend, Timbre};

#[cfg(feature="use_channel_pressure")]
use util::messages::Pressure;

#[cfg(not(feature="use_channel_pressure"))]
use util::messages::AfterTouch;

use util::raw_message::RawMessage;

use crate::change::SourceChange;
use crate::device::{Device, Expression, DeviceChange};
use crate::pattern_device::{PatternDevice, PatternDeviceChange};
use crate::timed_event::TimedEvent;
use util::midi_message_with_delta::MidiMessageWithDelta;
use std::sync::Arc;
use crate::parameters::ArpegiatorParameters;
use crate::worker::{WorkerChannels, WorkerCommand, create_worker_thread};
use std::thread::JoinHandle;
use std::mem::take;
use std::os::raw::c_void;

pub mod pattern;
mod note;
mod device;
mod pattern_device;
mod timed_event;
mod change;
mod expressive_note;
mod device_out;
mod parameters;
mod worker;
mod midi_controller_worker;


#[macro_use]
extern crate vst;


plugin_main!(ArpegiatorPlugin);


pub struct ArpegiatorPlugin {
    events: Vec<MidiEvent>,
    _host: HostCallback,
    pattern_device_in: Device,
    notes_device_in: Device,
    pattern_device: PatternDevice,
    current_time: usize,
    device_out: DeviceOut,
    parameters: Arc<ArpegiatorParameters>,
    worker_channels: Option<WorkerChannels>,
    thread_handle: Option<JoinHandle<()>>,
}


impl ArpegiatorPlugin {
    fn close_socket(&mut self) {
        if let Some(worker_channels) = self.worker_channels.as_ref() {
            if let Err(e) = worker_channels.command_sender.try_send(WorkerCommand::Stop) {
                error!("Error while closing note receiver channel : {:?}", e)
            }
        }

        if let Some(thread_handle) = take(&mut self.thread_handle) {
            thread_handle.join().unwrap();
        }

        self.worker_channels = None; // so the channel is not dropped before the thread is joined
    }
}


impl Default for ArpegiatorPlugin {
    fn default() -> Self {
        ArpegiatorPlugin {
            events: vec![],
            _host: Default::default(),
            pattern_device_in: Default::default(),
            notes_device_in: Default::default(),
            pattern_device: PatternDevice::default(),
            current_time: 0,
            device_out: DeviceOut::default(),
            parameters: Arc::new(ArpegiatorParameters::new()),
            worker_channels: None,
            thread_handle: None,
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

    fn vendor_specific(&mut self, index: i32, value: isize, ptr: *mut c_void, opt: f32) -> isize {
        // according to MPE specifications a vendor specific call should occur in order to signal VST
        // support ( page 15 ). As it seems all bitwig does is setting "MPE support" to true by default
        // when CanDo replies to "MPE", and it's just sending pitchwheel/pressure.
        // https://d30pueezughrda.cloudfront.net/campaigns/mpe/mpespec.pdf
        info!("vendor_specific {:?} {:?} {:?} {:?}", index, value, ptr, opt);
        0
    }

    fn new(host: HostCallback) -> Self {
        logging_setup();
        info!("{} use_channel_pressure: {}",
              build_info::format!("{{{} v{} built with {} at {}}} ", $.crate_info.name, $.crate_info.version, $
              .compiler, $.timestamp), cfg!(feature = "use_channel_pressure"));

        ArpegiatorPlugin {
            events: vec![],
            _host: host,
            pattern_device_in: Default::default(),
            notes_device_in: Default::default(),
            pattern_device: Default::default(),
            current_time: 0,
            device_out: DeviceOut::default(),
            parameters: Arc::new(ArpegiatorParameters::new()),
            worker_channels: None,
            thread_handle: None,
        }
    }

    fn resume(&mut self) {
        self.close_socket();

        self.current_time = 0;

        let worker_channels = create_worker_thread();

        worker_channels.command_sender.try_send(WorkerCommand::SetPort(self.parameters.get_port())).unwrap();

        if let Ok(mut worker_commands) = self.parameters.worker_commands.lock() {
            *worker_commands = Some(worker_channels.command_sender.clone());
        }

        self.worker_channels = Some(worker_channels);
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
            SendEvents | SendMidiEvent | ReceiveEvents | ReceiveMidiEvent | Offline | MidiSingleNoteTuningChange | MidiKeyBasedInstrumentControl => Yes,
            Other(s) => {
                if s == "MPE" {
                    Yes
                } else {
                    info!("Cando : {}", s);
                    Maybe
                }
            }
            _ => No,
        }
    }

    fn process(&mut self, buffer: &mut AudioBuffer<f32>) {
        let messages = match self.worker_channels.as_ref() {
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
                SourceChange::NoteChange(change) => {
                    // TODO note changed. for now we don't change anything, it's only when a pattern starts or ends
                    // that we trigger anything. Forwarding CC optional
                    match change {
                        DeviceChange::AddNote { .. } => {}
                        DeviceChange::RemoveNote { .. } => {}
                        DeviceChange::NoteExpressionChange { .. } => {}
                        DeviceChange::ReplaceNote { .. } => {}

                        DeviceChange::CCChange { cc: _cc, time: _time } => {
                            #[cfg(feature="forward_note_cc")]
                            {
                                let message = MidiMessageWithDelta {
                                    delta_frames,
                                    data: Into::<RawMessage>::into(_cc).into()
                                };

                                let _ = self.device_out.update(message, current_time, None);
                            }
                        }
                        DeviceChange::None { .. } => {}
                    }
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
                            let raw_message: Option<RawMessage> = match expression {
                                Expression::Timbre => {
                                    Some(Timbre { channel: pattern.channel, value: pattern.timbre }.into())
                                }
                                Expression::PitchBend => {
                                    Some(PitchBend { channel: pattern.channel, millisemitones: pattern.pitchbend }.into())
                                }
                                Expression::Pressure | Expression::AfterTouch => {
                                    #[cfg(feature="use_channel_pressure")] {
                                        Some(Pressure { channel: pattern.channel, value: pattern.pressure }.into())
                                    }

                                    #[cfg(not(feature="use_channel_pressure"))] match self.notes_device_in.notes
                                        .values().sorted().nth(pattern.index as usize) {
                                        None => None,
                                        Some(note) => {
                                            if let Some(pitch) = pattern.transpose(note.pitch) {
                                                Some(AfterTouch {
                                                    channel: pattern.channel,
                                                    pitch,
                                                    value: pattern.pressure,
                                                }.into())
                                            } else {
                                                None
                                            }
                                        }
                                    }
                                }
                            };

                            if let Some(raw_message) = raw_message {
                                self.device_out.update(MidiMessageWithDelta { delta_frames, data: raw_message.into() },
                                                       current_time, None);
                            }
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
                        PatternDeviceChange::CC { cc: _cc, time: _time } => {
                            #[cfg(feature="forward_pattern_cc")]
                            {
                                let message = MidiMessageWithDelta {
                                    delta_frames,
                                    data: Into::<RawMessage>::into(_cc).into()
                                };

                                let _ = self.device_out.update(message, current_time, None);
                            }
                        }
                        PatternDeviceChange::None { .. } => {}
                    }
                }
            }

            if let Some(worker_channels) = self.worker_channels.as_ref() {
                self.device_out.flush_to(&worker_channels.command_sender)
            }
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
