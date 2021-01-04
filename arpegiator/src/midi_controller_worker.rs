#[allow(unused_imports)]
use log::{error, info};

use util::raw_message::RawMessage;
use midir::{MidiOutput, MidiInput};
use midir::os::unix::{VirtualOutput, VirtualInput};
use util::constants::PRESSURE;
use async_channel::Receiver;


#[derive(Debug)]
pub enum ControllerCommand {
    RawMessage(RawMessage),
    Stop,
}

pub async fn midi_controller_worker(name: String, control_channel: Receiver<ControllerCommand>) {
    info!("Creating midi device {}", name);
    let midi_out = match MidiOutput::new(&*name) {
        Ok(midi_out) => midi_out,
        Err(err) => {
            error!("Could not create midi out port {}", err);
            return;
        }
    };
    let mut midi_out_connection = match midi_out.create_virtual(&*name) {
        Ok(midi_out_connection) => midi_out_connection,
        Err(err) => {
            error!("Could not get a midi out connection {}", err);
            return;
        }
    };

    let midi_in = match MidiInput::new(&*name) {
        Ok(midi_in) => midi_in,
        Err(err) => {
            error!("Could not create midi in port {}", err);
            return;
        }
    };

    // create input device just to ease setup. returned connection must not be dropped in order to keep the device alive
    let mut _midi_in_connection = match midi_in.create_virtual(&*name, |_ime, _data,_| {
        // noop for now
    }, ()) {
        Ok(midi_in_connection) => midi_in_connection,
        Err(err) => {
            error!("Could not get a midi in connection {}", err);
            return;
        }
    };


    loop {
        match control_channel.recv().await {
            Ok(command) => {
                match command {
                    ControllerCommand::RawMessage(raw_message) => {
                        let message = Into::<[u8; 3]>::into(raw_message);
                        // ugly hack originally with the intent of moving around a fixed amount of u8
                        let len = if message[0] & 0xF0 == PRESSURE { 2 } else { 3 };
                        if let Err(err) = midi_out_connection.send(&message[..len]) {
                            error!("Error while sending midi message: {}", err);
                            return;
                        }
                    }
                    ControllerCommand::Stop => {
                        info!("Stopping controller {}", name);
                        midi_out_connection.close();
                        return;
                    }
                }
            }
            Err(err) => {
                error!("Error while fetching a command from the channel: {}", err);
                return;
            }
        }
    }
}
