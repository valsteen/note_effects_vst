use std::mem::take;
use std::sync::Arc;
use log::{info, error};

use async_channel::Sender;

use vst::api;
use vst::buffer::AudioBuffer;
use vst::event::Event;
use vst::plugin::{CanDo, Category, HostCallback, Info, Plugin};

use util::logging::logging_setup;
use util::midi_message_with_delta::MidiMessageWithDelta;
use util::ipc_payload::PatternPayload;

use crate::ipc_worker::{IPCWorkerCommand, spawn_ipc_worker};
use crate::parameters::ArpegiatorPatternReceiverParameters;

mod parameters;
mod ipc_worker;

#[macro_use]
extern crate vst;

plugin_main!(ArpegiatorPatternReceiver);


struct ArpegiatorPatternReceiver {
    #[allow(dead_code)]
    host: HostCallback,
    ipc_worker_sender: Option<Sender<IPCWorkerCommand>>,
    messages: Vec<MidiMessageWithDelta>,
    current_time: usize,
    parameters: Arc<ArpegiatorPatternReceiverParameters>
}


impl Default for ArpegiatorPatternReceiver {
    fn default() -> Self {
        ArpegiatorPatternReceiver {
            host: Default::default(),
            ipc_worker_sender: None,
            messages: vec![],
            current_time: 0,
            parameters: Arc::new(ArpegiatorPatternReceiverParameters::new())
        }
    }
}

impl ArpegiatorPatternReceiver {
    fn stop_worker(&mut self) {
        if let Some(sender) = take(&mut self.ipc_worker_sender) {
            sender.try_send(IPCWorkerCommand::Stop).unwrap_or_else(|err| {
                error!("Error while closing sender channel : {}", err)
            });
        }
    }
}


impl Plugin for ArpegiatorPatternReceiver {
    fn get_info(&self) -> Info {
        Info {
            name: "Arpegiator Pattern Receiver".to_string(),
            vendor: "DJ Crontab".to_string(),
            unique_id: 342112720,
            parameters: 1,
            category: Category::Synth,
            initial_delay: 0,
            version: 2,
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

    fn resume(&mut self) {
        self.current_time = 0 ;

        let sender= spawn_ipc_worker();
        self.ipc_worker_sender = Some(sender.clone());
        sender.try_send(IPCWorkerCommand::SetPort(self.parameters.get_port())).unwrap();

        if let Ok(mut socket_command) = self.parameters.ipc_worker_sender.lock() {
            *socket_command = Some(sender);
        }
    }

    fn suspend(&mut self) {
        self.stop_worker()
    }

    fn new(host: HostCallback) -> Self {
        logging_setup();
        info!("{}", build_info::format!("{{{} v{} built with {} at {}}}", $.crate_info.name, $.crate_info.version,
        $.compiler, $.timestamp));

        ArpegiatorPatternReceiver {
            host,
            ipc_worker_sender: None,
            messages: vec![],
            current_time: 0,
            parameters: Arc::new(ArpegiatorPatternReceiverParameters::new())
        }
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
        if !self.messages.is_empty() {
            if let Some(ipc_worker_sender) = &self.ipc_worker_sender {
                let payload = PatternPayload {
                    #[cfg(target_os = "macos")]
                    time: unsafe { mach::mach_time::mach_absolute_time() },
                    messages: take(&mut self.messages)
                } ;
                ipc_worker_sender.try_send(IPCWorkerCommand::Send(payload)).unwrap()
            } else {
                self.messages.clear();
            }
        }

        self.current_time += buffer.samples()
    }

    fn process_events(&mut self, events: &api::Events) {
        if self.ipc_worker_sender.is_some() {
            self.messages.extend(events.events().map(|event| match event {
                Event::Midi(event) => Ok(MidiMessageWithDelta {
                    delta_frames: event.delta_frames as u16,
                    data: event.data.into()
                }),
                Event::SysEx(_) => Err(()),
                Event::Deprecated(_) => Err(())
            }).filter(|item| item.is_ok()).map(|item| item.unwrap()));

            //|midi_event| midi_event.unwrap())
        }
    }

    fn get_parameter_object(&mut self) -> Arc<dyn vst::plugin::PluginParameters> {
        Arc::clone(&self.parameters) as Arc<dyn vst::plugin::PluginParameters>
    }
}

impl Drop for ArpegiatorPatternReceiver {
    fn drop(&mut self) {
        self.stop_worker();
    }
}
