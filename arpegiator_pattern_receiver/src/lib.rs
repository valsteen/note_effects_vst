use std::sync::mpsc::Sender;
use std::thread::JoinHandle;

use log::{info, error};
use vst::api;
use vst::buffer::AudioBuffer;
use vst::event::Event;
use vst::plugin::{CanDo, Category, HostCallback, Info, Plugin};

use util::logging::logging_setup;
use util::midi_message_with_delta::MidiMessageWithDelta;
use crate::socket::{SenderSocketCommand, create_socket_thread};
use std::mem::take;
use crate::parameters::ArpegiatorPatternReceiverParameters;
use std::sync::Arc;
use util::pattern_payload::PatternPayload;

mod parameters;
mod socket;

#[macro_use]
extern crate vst;

plugin_main!(ArpegiatorPatternReceiver);


struct ArpegiatorPatternReceiver {
    host: HostCallback,
    socket_thread_handle: Option<JoinHandle<()>>,
    socket_channel_sender: Option<Sender<SenderSocketCommand>>,
    messages: Vec<MidiMessageWithDelta>,
    current_time: usize,
    parameters: Arc<ArpegiatorPatternReceiverParameters>
}


impl Default for ArpegiatorPatternReceiver {
    fn default() -> Self {
        ArpegiatorPatternReceiver {
            host: Default::default(),
            socket_thread_handle: None,
            socket_channel_sender: None,
            messages: vec![],
            current_time: 0,
            parameters: Arc::new(ArpegiatorPatternReceiverParameters::new())
        }
    }
}

impl ArpegiatorPatternReceiver {
    fn close_socket(&mut self) {
        if let Some(sender) = take(&mut self.socket_channel_sender) {
            if let Err(e) = sender.send(SenderSocketCommand::Stop) {
                error!("Error while closing sender channel : {:?} {}", e, e)
            }
        }

        if let Some(thread_handle) = take(&mut self.socket_thread_handle) {
            thread_handle.join().unwrap();
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

        let (join_handle, sender) = create_socket_thread();
        self.socket_thread_handle = Some(join_handle) ;
        self.socket_channel_sender = Some(sender.clone());
        sender.send(SenderSocketCommand::SetPort(self.parameters.get_port())).unwrap();

        if let Ok(mut socket_command) = self.parameters.socket_command.lock() {
            *socket_command = Some(sender);
        }
    }

    fn suspend(&mut self) {
        self.close_socket()
    }

    fn new(host: HostCallback) -> Self {
        logging_setup();
        info!("{}", build_info::format!("{{{} v{} built with {} at {}}}", $.crate_info.name, $.crate_info.version,
        $.compiler, $.timestamp));

        ArpegiatorPatternReceiver {
            host,
            socket_thread_handle: None,
            socket_channel_sender: None,
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
            if let Some(sender) = &self.socket_channel_sender {
                let payload = PatternPayload {
                    time: self.current_time,
                    messages: take(&mut self.messages)
                } ;
                sender.send(SenderSocketCommand::Send(payload)).unwrap()
            }
            self.messages.clear();
        }

        self.current_time += buffer.samples()
    }

    fn process_events(&mut self, events: &api::Events) {
        if self.socket_channel_sender.is_some() {
            for e in events.events() {
                if let Event::Midi(e) = e {
                    self.messages.push( MidiMessageWithDelta {
                        delta_frames: e.delta_frames as u16,
                        data: e.data
                    });
                }
            }
        }
    }

    fn get_parameter_object(&mut self) -> Arc<dyn vst::plugin::PluginParameters> {
        Arc::clone(&self.parameters) as Arc<dyn vst::plugin::PluginParameters>
    }
}

impl Drop for ArpegiatorPatternReceiver {
    fn drop(&mut self) {
        self.close_socket();
    }
}
