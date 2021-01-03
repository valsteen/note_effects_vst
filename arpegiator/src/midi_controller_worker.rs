#[allow(unused_imports)]
use log::{error, info};

use util::raw_message::RawMessage;
use midir::MidiOutput;
use midir::os::unix::VirtualOutput;
use util::constants::PRESSURE;
use smol::channel::Receiver;

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
            error!("Could not create midi port {}", err);
            return
        }
    };
    let mut midi_connection = match midi_out.create_virtual(&*name) {
        Ok(midi_connection) => midi_connection,
        Err(err) => {
            error!("Could not get a midi connection {}", err);
            return
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
                        if let Err(err) = midi_connection.send(&message[..len]) {
                            error!("Error while sending midi message: {}", err);
                            return
                        }
                    }
                    ControllerCommand::Stop => {
                        midi_connection.close();
                        return
                    }
                }
            }
            Err(err) => {
                error!("Error while fetching a command from the channel: {}", err);
                return
            }
        }
    }
}
