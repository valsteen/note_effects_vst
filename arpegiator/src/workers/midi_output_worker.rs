#[allow(unused_imports)]
use log::{error, info};

use async_channel::{Sender, unbounded};
use async_std::task;

#[cfg(target_os = "macos")]
use {
    coremidi::PacketBuffer
};


#[cfg(target_os = "linux")]
use {
    midir::MidiOutput,
    midir::os::unix::VirtualOutput
};
use midir::MidiInput;
use midir::os::unix::VirtualInput;

use util::midi_message_with_delta::MidiMessageWithDelta;


#[cfg(target_os = "macos")]
use crate::system::second_to_mach_timebase;
use std::error;
use util::system::Uuid;


#[derive(Debug)]
pub(crate) enum MidiOutputWorkerCommand {
    SendToController { reception_time: u64, messages: Vec<MidiMessageWithDelta> },
    Stop(Sender<()>, Uuid),
    SetSampleRate(f32),
    SetBlockSize(i64)
}


pub(crate) fn spawn_midi_output_worker(name: String) ->
    Result<Sender<MidiOutputWorkerCommand>, Box<dyn error::Error + Send + Sync>> {

    #[cfg(target_os = "macos")]
    let (second_to_mach, mut sample_to_mach, mut _block_size, mut block_duration) : (f64, u64, u64, u64) = {
        (second_to_mach_timebase(), 0, 0, 0)
    };

    #[cfg(target_os = "linux")]
    let mut midi_out_connection = {
        info!("Creating midi device {}", name);
        let midi_out = MidiOutput::new(&*name)?;
        midi_out.create_virtual(&*name).map_err(|e| format!("Cannot create midi out: {}", e))?
    };

    #[cfg(target_os = "macos")]
    let (client, source) = {
        let client = coremidi::Client::new(&*name)
            .map_err(
            |x| format!("os error: {:?}", x)
            )?;
        let source = client.virtual_source(&*name).map_err(
            |x|  format!("os error: {:?}", x)
        )?;
        info!("Created virtual port {}", name);
        (client, source)
    };

    let midi_in = MidiInput::new(&*name)?;

    // create input device just to ease setup. returned connection must not be dropped in order to keep the device alive
    let midi_in_connection = midi_in.create_virtual(&*name, |_time, _data, _| {
        // noop for now
    }, ()).map_err(
        |x| format!("os error: {:?}", x)
    )?;

    let (sender, receiver) = unbounded::<MidiOutputWorkerCommand>();

    task::spawn(async move {
        // move the client inside the task, otherwise it will be dropped and the device won't appear
        #[cfg(target_os = "macos")] let _client = client ;

        // move it here to keep the device visible. no use for now, just helps to have a consistent in/out setup
        let _midi_in_connection = midi_in_connection;

        while let Ok(command) = receiver.recv().await {
            match command {
                #[allow(unused_mut)]
                #[allow(unused_variables)]
                MidiOutputWorkerCommand::SendToController { reception_time, mut messages } => {
                    #[cfg(target_os = "linux")]
                    for message in messages {
                        // TODO timing is lost, we should actually wait until buffer_start_time and wait for the
                        // time corresponding to delta frame for each message
                        if let Err(err) = midi_out_connection.send(message.data.get_bytes()) {
                            error!("Error while sending midi message: {}", err);
                            break;
                        }
                    }

                    #[cfg(target_os = "macos")]
                    {
                        // we cannot accurately tell when those notes should be played, we just know when it was
                        // received and the earliest time being reception + time that represents a buffer
                        let play_time = reception_time + block_duration;
                        let message = messages.remove(0);
                        let mut buffer = PacketBuffer::new(
                            message.delta_frames as u64 * sample_to_mach + play_time,
                            message.data.get_bytes(),
                        );

                        for message in messages {
                            buffer.push_data(message.delta_frames as u64 * sample_to_mach + play_time,
                                             &message.data.get_bytes());
                        }

                        source.received(&buffer).unwrap();
                    }
                }
                MidiOutputWorkerCommand::Stop(sender, event_id) => {
                    match sender.send(()).await {
                        Ok(_) => { info!("[{}] Stopping controller {}", event_id, name); }
                        Err(err) => { info!("[{}] Error while quitting midi out {}: {}", event_id, name, err); }
                    }
                    return;
                }
                MidiOutputWorkerCommand::SetSampleRate(_rate) => {
                    #[cfg(target_os = "macos")]
                    {
                        let sample_to_second = 1.0 / _rate as f64;
                        sample_to_mach = (sample_to_second * second_to_mach) as u64;
                        block_duration = sample_to_mach * _block_size;
                    }
                }
                MidiOutputWorkerCommand::SetBlockSize(_size) => {
                    #[cfg(target_os = "macos")]
                    {
                        _block_size = _size as u64;
                        block_duration = sample_to_mach * _block_size;
                    }
                }
            }
        }
    });

    Ok(sender)
}
