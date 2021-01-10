#[allow(unused_imports)]
use {
    std::mem::take,
    log::{error, info}
};

use std::os::raw::c_void;
use std::sync::Arc;

use itertools::Itertools;
use vst::api;
use vst::buffer::{AudioBuffer, SendEventBuffer};
use vst::event::{Event, MidiEvent};
use vst::plugin::{CanDo, Category, HostCallback, Info, Plugin};

use midi_messages::device::{Device, DeviceChange, Expression};
use midi_messages::device_out::DeviceOut;
use util::logging::logging_setup;
use util::messages::{PitchBend, Timbre};
#[cfg(not(feature = "use_channel_pressure"))]
use util::messages::AfterTouch;
#[cfg(feature = "use_channel_pressure")]
use util::messages::Pressure;
use util::midi_message_with_delta::MidiMessageWithDelta;
use util::raw_message::RawMessage;
#[cfg(not(feature="midi_hack_transmission"))]
use {
    workers::main_worker::{create_worker_thread, WorkerChannels, WorkerCommand},
};

#[cfg(not(feature="midi_hack_transmission"))]
#[cfg(target_os = "macos")]
use {
    mach::mach_time::mach_absolute_time
};

use crate::parameters::{ArpegiatorParameters, PARAMETER_COUNT};
use crate::midi_messages::change::SourceChange;
use crate::midi_messages::pattern_device::{PatternDevice, PatternDeviceChange};
use util::system::Uuid;
use crate::midi_messages::timed_event::TimedEvent;

#[cfg(not(feature="midi_hack_transmission"))] mod workers;
#[cfg(not(feature="midi_hack_transmission"))] mod system;

mod midi_messages;
mod parameters;


#[macro_use]
extern crate vst;


plugin_main!(ArpegiatorPlugin);


pub struct ArpegiatorPlugin {
    events: Vec<MidiEvent>,
    _host: HostCallback,
    #[cfg(feature = "midi_hack_transmission")]
    send_buffer: SendEventBuffer,
    pattern_device_in: Device,
    notes_device_in: Device,
    pattern_device: PatternDevice,
    current_time_in_samples: usize,
    sample_rate: f32,
    block_size: i64,
    device_out: DeviceOut,
    parameters: Arc<ArpegiatorParameters>,
    #[cfg(not(feature = "midi_hack_transmission"))]
    worker_channels: Option<WorkerChannels>,
    resumed: bool,
}


impl ArpegiatorPlugin {
    #[cfg(not(feature = "midi_hack_transmission"))]
    fn close_worker(&mut self, event_id: Uuid) {
        if let Some(worker_channels) = take(&mut self.worker_channels) {
            #[cfg(feature = "worker_debug")] info!("[{}] stopping workers", event_id);
            if let Err(e) = worker_channels.command_sender.try_send(WorkerCommand::Stop(event_id)) {
                error!("[{}] Error while closing worker channel : {:?}", event_id, e)
            }
            if let Err(err) = worker_channels.worker.join() {
                error!("[{}] Error while waiting for worker thread to finish {:?}", event_id, err)
            }
        }
    }
}


impl Default for ArpegiatorPlugin {
    fn default() -> Self {
        ArpegiatorPlugin {
            events: vec![],
            _host: Default::default(),
            #[cfg(feature = "midi_hack_transmission")]
            send_buffer: Default::default(),
            pattern_device_in: Device::new("Patterns".to_string()),
            notes_device_in: Device::new("Notes".to_string()),
            pattern_device: PatternDevice::default(),
            current_time_in_samples: 0,
            sample_rate: 44100.0,
            block_size: 64,
            device_out: DeviceOut::new("Out".to_string()),
            parameters: Arc::new(ArpegiatorParameters::new()),
            #[cfg(not(feature = "midi_hack_transmission"))]
            worker_channels: None,
            resumed: false,
        }
    }
}


impl Plugin for ArpegiatorPlugin {
    fn get_info(&self) -> Info {
        Info {
            name: "Arpegiator".to_string(),
            vendor: "DJ Crontab".to_string(),
            unique_id: 342111721,
            parameters: PARAMETER_COUNT as i32,
            category: Category::Synth,
            // this device must start slightly later than the pattern receiver, so it receives patterns that
            // apply to the same buffer
            // one sample would be 0.2ms down to 0.05ms. increase amount of samples if the delay between plugins
            // is greater than 0.05ms
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
        info!("{} use_channel_pressure: {}",
              build_info::format!("{{{} v{} built with {} at {}}} ", $.crate_info.name, $.crate_info.version, $
              .compiler, $.timestamp), cfg!(feature = "use_channel_pressure"));

        ArpegiatorPlugin {
            events: vec![],
            _host: host,
            #[cfg(feature = "midi_hack_transmission")]
            send_buffer: Default::default(),
            pattern_device_in: Device::new("Pattern".to_string()),
            notes_device_in: Device::new("Notes".to_string()),
            pattern_device: Default::default(),
            current_time_in_samples: 0,
            sample_rate: 44100.,
            block_size: 64,
            device_out: DeviceOut::new("Out".to_string()),
            parameters: Arc::new(ArpegiatorParameters::new()),
            #[cfg(not(feature = "midi_hack_transmission"))]
            worker_channels: None,
            resumed: false,
        }
    }

    fn set_sample_rate(&mut self, rate: f32) {
        #[cfg(not(feature="midi_hack_transmission"))]
        if let Some(workers_channel) = &self.worker_channels {
            workers_channel.command_sender.try_send(WorkerCommand::SetSampleRate(rate)).unwrap()
        };
        self.sample_rate = rate
    }

    fn set_block_size(&mut self, size: i64) {
        self.block_size = size;

        #[cfg(not(feature="midi_hack_transmission"))]
        if let Some(workers_channel) = &self.worker_channels {
            workers_channel.command_sender.try_send(WorkerCommand::SetBlockSize(size)).unwrap()
        };
    }

    fn resume(&mut self) {
        if self.resumed {
            info!("Already resumed");
            return;
        }
        self.resumed = true;

        let event_id = Uuid::new_v4();

        #[cfg(feature = "worker_debug")] info!("[{}] resume: enter", event_id);

        self.current_time_in_samples = 0;

        #[cfg(not(feature = "midi_hack_transmission"))]
        {
            self.close_worker(event_id);
            let worker_channels = create_worker_thread();
            worker_channels.command_sender.try_send(WorkerCommand::SetPort(self.parameters.get_port(), event_id)).unwrap();
            worker_channels.command_sender.try_send(WorkerCommand::SetSampleRate(self.sample_rate)).unwrap();

            self.worker_channels = match self.parameters.worker_commands.lock() {
                Ok(mut worker_commands) => {
                    *worker_commands = Some(worker_channels.command_sender.clone());
                    Some(worker_channels)
                }
                Err(err) => {
                    error!("[{}] Could not get parameters lock: {}", event_id, err);
                    None
                }
            };
        }

        #[cfg(feature = "worker_debug")] info!("[{}] resume: exit", event_id);
    }

    fn suspend(&mut self) {
        if !self.resumed {
            info!("Already suspended");
            return;
        }
        let event_id = Uuid::new_v4();

        self.resumed = false;

        #[cfg(not(feature = "midi_hack_transmission"))]
        {
            #[cfg(feature = "worker_debug")] info!("[{}] suspend enter", event_id);
            if let Ok(mut worker_commands) = self.parameters.worker_commands.lock() {
                *worker_commands = None
            }

            self.close_worker(event_id);
        }
        #[cfg(feature = "worker_debug")] info!("[{}] suspend exit", event_id);
    }

    fn vendor_specific(&mut self, index: i32, value: isize, ptr: *mut c_void, opt: f32) -> isize {
        // according to MPE specifications a vendor specific call should occur in order to signal VST
        // support ( page 15 ). As it seems all bitwig does is setting "MPE support" to true by default
        // when CanDo replies to "MPE", and it's just sending pitchwheel/pressure.
        // https://d30pueezughrda.cloudfront.net/campaigns/mpe/mpespec.pdf
        info!("vendor_specific {:?} {:?} {:?} {:?}", index, value, ptr, opt);
        0
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
        #[cfg(not(feature = "midi_hack_transmission"))]
        let local_time = {
            #[cfg(target_os = "macos")] unsafe { mach_absolute_time() }
            #[cfg(target_os = "linux")] 0
        };

        #[cfg(not(feature = "midi_hack_transmission"))]
        let pattern_messages = {
            match self.worker_channels.as_ref() {
                None => vec![],
                Some(socket_channels) => {
                    match socket_channels.pattern_receiver.try_recv() {
                        Ok(payload) => {
                            #[cfg(feature = "device_debug")]
                            info!("[{}] received patterns : {:02X?}", self.current_time_in_samples, payload);

                            payload.messages
                        }
                        Err(_) => vec![]
                    }
                }
            }
        };
        #[cfg(not(feature = "midi_hack_transmission"))]
        let events = &self.events;


        // from here we cannot accurately tell when the buffer we're building will actually play
        // so our best guest will be the earliest :
        // current_time + (delta_frame + buffer_size) * sample_to_second

        // this avoids borrowing self
        let current_time_in_samples = self.current_time_in_samples;
        let pattern_device_in = &mut self.pattern_device_in;
        let pattern_device = &mut self.pattern_device;
        let notes_device_in = &mut self.notes_device_in;

        #[cfg(feature = "midi_hack_transmission")]
        let (events, pattern_messages) : (Vec<MidiEvent>, Vec<MidiMessageWithDelta>) = {
            let (mut events, mut patterns) : (Vec<MidiEvent>, Vec<MidiEvent>) = self.events.drain(..).partition(
                |item| item.data[0] < 0x80);
            events.sort_by_key(|x| x.delta_frames);
            patterns.sort_by_key(|x| x.delta_frames);
            let patterns = patterns.iter().map(|event| MidiMessageWithDelta {
                delta_frames: event.delta_frames as u16,
                data: RawMessage::from([event.data[0] + 0x80, event.data[1], event.data[2]])
            }).collect_vec();
            (events, patterns)
        };

        let pattern_changes = pattern_messages.into_iter().map(|message| {
            let change = pattern_device_in.update(message, current_time_in_samples, None);
            let change = pattern_device.update(change);
            SourceChange::PatternChange(change)
        });

        let note_changes = events.iter().map(|event| {
            let midi_message_with_delta = MidiMessageWithDelta {
                delta_frames: event.delta_frames as u16,
                data: event.data.into(),
            };

            let change = notes_device_in.update(midi_message_with_delta, current_time_in_samples, None);
            SourceChange::NoteChange(change)
        });

        // merge() gets a sorted output. If a note is triggered at the same time as a pattern, note comes first in order
        // to set the pitch
        for change in pattern_changes.sorted().merge(note_changes.sorted()) {
            let delta_frames = (change.timestamp() - self.current_time_in_samples) as u16;

            match change {
                SourceChange::NoteChange(change) => {
                    // TODO note change should trigger pitchbend events
                    match change {
                        DeviceChange::AddNote { .. } => {}
                        DeviceChange::RemoveNote { .. } => {}
                        DeviceChange::NoteExpressionChange { .. } => {}
                        DeviceChange::ReplaceNote { .. } => {}

                        DeviceChange::CCChange { cc: _cc, time: _time } => {
                            #[cfg(feature = "forward_note_cc")] {
                                let message = MidiMessageWithDelta {
                                    delta_frames,
                                    data: Into::<RawMessage>::into(_cc).into(),
                                };

                                let _ = self.device_out.update(message, current_time_in_samples, None);
                            }
                        }
                        DeviceChange::None { .. } => {}
                    }
                }
                SourceChange::PatternChange(change) => {
                    match change {
                        PatternDeviceChange::AddPattern { pattern, .. } => {
                            // TODO "hold notes" logic
                            match self.notes_device_in.notes.values().sorted().nth(pattern.index as usize) {
                                None => {}
                                Some(note) => self.device_out.push_note_on(&pattern, &note, current_time_in_samples)
                            }
                        }

                        PatternDeviceChange::PatternExpressionChange { expression, pattern, .. } => {
                            let raw_message: Option<RawMessage> = match expression {
                                Expression::Timbre => {
                                    Some(Timbre { channel: pattern.channel, value: pattern.timbre }.into())
                                }
                                Expression::PitchBend => {
                                    // TODO should change the pitch as is, meaning it's the pitchbend is just added to
                                    // the result, independently from the notes we're supposed to match
                                    // the result should be: target note + pattern pitchbend
                                    Some(PitchBend { channel: pattern.channel, millisemitones: pattern.pitchbend }.into())
                                }
                                Expression::Pressure | Expression::AfterTouch => {
                                    #[cfg(feature = "use_channel_pressure")] {
                                        Some(Pressure { channel: pattern.channel, value: pattern.pressure }.into())
                                    }

                                    #[cfg(not(feature = "use_channel_pressure"))] match self.notes_device_in.notes.values().sorted().nth(pattern.index as usize) {
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
                                self.device_out.update(MidiMessageWithDelta { delta_frames, data: raw_message },
                                                       current_time_in_samples, None);
                            }
                        }
                        PatternDeviceChange::RemovePattern { pattern, .. } => {
                            self.device_out.push_note_off(pattern.id, pattern.velocity_off,
                                                          delta_frames, current_time_in_samples);
                        }
                        PatternDeviceChange::ReplacePattern { old_pattern, new_pattern, .. } => {
                            self.device_out.push_note_off(old_pattern.id, old_pattern.velocity_off,
                                                          delta_frames, current_time_in_samples);

                            let note = match self.notes_device_in.notes.values().sorted().nth(new_pattern.index as usize) {
                                None => { continue; }
                                Some(note) => note
                            };

                            self.device_out.push_note_on(&new_pattern, note, current_time_in_samples);
                        }
                        PatternDeviceChange::CC { cc: _cc, time: _time } => {
                            #[cfg(feature = "forward_pattern_cc")] {
                                let message = MidiMessageWithDelta {
                                    delta_frames,
                                    data: _cc.into(),
                                };

                                let _ = self.device_out.update(message, current_time_in_samples, None);
                            }
                        }
                        PatternDeviceChange::None { .. } => {}
                    }
                }
            }
        };

        #[cfg(not(feature = "midi_hack_transmission"))]
        if let Some(worker_channels) = self.worker_channels.as_ref() {
            self.device_out.flush_to(local_time, &worker_channels.command_sender)
        }

        // TODO
        //self.send_buffer.send_events(events, &mut self._host);

        self.events.clear();

        self.current_time_in_samples += buffer.samples()
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

#[cfg(not(feature="midi_hack_transmission"))]
impl Drop for ArpegiatorPlugin {
    fn drop(&mut self) {
        let event_id = Uuid::new_v4();
        info!("[{}] Dropping plugin", event_id);
        self.close_worker(event_id);
    }
}
