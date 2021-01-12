#[allow(unused_imports)]
use {
    std::mem::take,
    log::{info, error},
    async_channel::Sender,
    vst::event::Event,
    vst::buffer::SendEventBuffer,
    vst::api::MidiEvent
};
use std::sync::Arc;

use vst::api;
use vst::buffer::{AudioBuffer};
use vst::plugin::{CanDo, Category, HostCallback, Info, Plugin};

use util::logging::logging_setup;

#[cfg(not(feature="midi_hack_transmission"))]
use {
    util::ipc_payload::PatternPayload,
    crate::ipc_worker::{IPCWorkerCommand, spawn_ipc_worker},
    util::midi_message_with_delta::MidiMessageWithDelta
};

use crate::parameters::{ArpegiatorPatternReceiverParameters, PARAMETER_COUNT};

mod parameters;
#[cfg(not(feature="midi_hack_transmission"))] mod ipc_worker;

#[macro_use]
extern crate vst;

plugin_main!(ArpegiatorPatternReceiver);


struct ArpegiatorPatternReceiver {
    #[allow(dead_code)]
    host: HostCallback,
    #[cfg(feature = "midi_hack_transmission")]
    send_buffer: SendEventBuffer,
    #[cfg(not(feature="midi_hack_transmission"))]
    ipc_worker_sender: Option<Sender<IPCWorkerCommand>>,
    #[cfg(not(feature="midi_hack_transmission"))]
    messages: Vec<MidiMessageWithDelta>,
    #[cfg(feature="midi_hack_transmission")]
    messages: Vec<vst::event::MidiEvent>,
    current_time: usize,
    resumed: bool,
    parameters: Arc<ArpegiatorPatternReceiverParameters>
}


impl Default for ArpegiatorPatternReceiver {
    fn default() -> Self {
        ArpegiatorPatternReceiver {
            host: Default::default(),
            #[cfg(not(feature="midi_hack_transmission"))]
            ipc_worker_sender: None,
            messages: vec![],
            current_time: 0,
            resumed: false,
            parameters: Arc::new(ArpegiatorPatternReceiverParameters::new()),
            #[cfg(feature="midi_hack_transmission")]
            send_buffer: Default::default()
        }
    }
}

impl ArpegiatorPatternReceiver {
    #[cfg(not(feature="midi_hack_transmission"))]
    fn stop_worker(&mut self) {
        if let Some(sender) = take(&mut self.ipc_worker_sender) {
            sender.try_send(IPCWorkerCommand::Stop).unwrap_or_else(|err| {
                error!("Error while closing sender channel : {}", err)
            });
            sender.close();
        }
    }
}


impl Plugin for ArpegiatorPatternReceiver {
    fn get_info(&self) -> Info {
        Info {
            name: "Arpegiator Pattern Receiver".to_string(),
            vendor: "DJ Crontab".to_string(),
            unique_id: 342112720,
            parameters: PARAMETER_COUNT as i32,
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

    fn new(host: HostCallback) -> Self {
        logging_setup();
        info!("{} midi_hack_transmission={}",
              build_info::format!("{{{} v{} built with {} at {}}}", $.crate_info.name,
                $.crate_info.version,
                $.compiler, $.timestamp),
              cfg!(feature = "midi_hack_transmission"));

        ArpegiatorPatternReceiver {
            host,
            #[cfg(not(feature="midi_hack_transmission"))]
            ipc_worker_sender: None,
            messages: vec![],
            current_time: 0,
            resumed: false,
            parameters: Arc::new(ArpegiatorPatternReceiverParameters::new()),
            #[cfg(feature="midi_hack_transmission")]
            send_buffer: Default::default()
        }
    }

    fn resume(&mut self) {
        if self.resumed {
            return;
        }
        self.resumed = true;

        self.current_time = 0 ;

        #[cfg(not(feature="midi_hack_transmission"))]
        {
            self.stop_worker();

            let sender= spawn_ipc_worker();

            self.ipc_worker_sender = Some(sender.clone());
            sender.try_send(IPCWorkerCommand::SetPort(self.parameters.get_port())).unwrap();

            if let Ok(mut socket_command) = self.parameters.ipc_worker_sender.lock() {
                *socket_command = Some(sender);
            }
        }
    }

    fn suspend(&mut self) {
        if !self.resumed {
            return;
        }
        self.resumed = false;

        #[cfg(not(feature="midi_hack_transmission"))]
        {
            self.stop_worker()
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
            #[cfg(not(feature="midi_hack_transmission"))]
            if let Some(ipc_worker_sender) = &self.ipc_worker_sender {
                let payload = PatternPayload {
                    time: {
                        #[cfg(target_os = "macos")] unsafe { mach::mach_time::mach_absolute_time() }
                        #[cfg(target_os = "linux")] 0
                    },
                    messages: take(&mut self.messages)
                } ;
                ipc_worker_sender.try_send(IPCWorkerCommand::Send(payload)).unwrap()
            } else {
                self.messages.clear();
            }

            #[cfg(feature="midi_hack_transmission")]
            {
                self.send_buffer.send_events(&self.messages, &mut self.host);
                self.messages.clear()
            }
        }

        self.current_time += buffer.samples()
    }

    fn process_events(&mut self, events: &api::Events) {
        #[cfg(not(feature="midi_hack_transmission"))]
        if self.ipc_worker_sender.is_none() {
            return;
        }

        self.messages.extend(events.events().map(|event| match event {
            #[allow(unused_mut)]
            Event::Midi(mut event) => {
                #[cfg(not(feature = "midi_hack_transmission"))] {
                    Ok(MidiMessageWithDelta {
                        delta_frames: event.delta_frames as u16,
                        data: event.data.into(),
                    })
                }
                #[cfg(feature = "midi_hack_transmission")] {
                    event.data[0] -= 0x80;
                    Ok(event)
                }
            },
            Event::SysEx(_) => Err(()),
            Event::Deprecated(_) => Err(())
        }).filter(|item| item.is_ok()).map(|item| item.unwrap()));
    }

    fn get_parameter_object(&mut self) -> Arc<dyn vst::plugin::PluginParameters> {
        Arc::clone(&self.parameters) as Arc<dyn vst::plugin::PluginParameters>
    }
}

impl Drop for ArpegiatorPatternReceiver {
    fn drop(&mut self) {
        #[cfg(not(feature="midi_hack_transmission"))]
        {
            self.stop_worker();
        }
    }
}
