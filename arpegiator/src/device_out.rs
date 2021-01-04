#[allow(unused_imports)]
use log::{error, info};

use util::messages::NoteOff;
use util::raw_message::RawMessage;

use crate::device::Device;
use crate::expressive_note::ExpressiveNote;
use crate::note::Note;
use crate::pattern::Pattern;
use util::midi_message_with_delta::MidiMessageWithDelta;
use crate::midi_controller_worker::ControllerCommand;
use crate::worker::WorkerCommand;
use async_channel::Sender;


#[derive(Default)]
pub struct DeviceOut {
    pub device: Device,
    queue: Vec<MidiMessageWithDelta>,
}


impl DeviceOut {
    pub fn update(&mut self, midi_message: MidiMessageWithDelta, current_time: usize, id: Option<usize>) {
        self.queue.push(midi_message);
        self.device.update(midi_message, current_time, id);
    }

    pub fn flush_to(&mut self, midi_controller_sender: &Sender<WorkerCommand>) {
        for message in self.queue.drain(..) {
            if let Err(err) = midi_controller_sender.try_send(
                WorkerCommand::SendToController(ControllerCommand::RawMessage(RawMessage::from(message.data)))) {
                error!("Could not send to the controller worker {}", err)
            }
        }
    }

    pub fn push_note_off(&mut self, note_id: usize, velocity_off: u8, delta_frames: u16, current_time: usize) {
        let note = match self.device.notes.values().find(|note| note.id == note_id) {
            None => {
                // info!("Cannot find note to stop: {:02x?}", note_id);
                return;
            }
            Some(note) => note
        };

        let raw_message: RawMessage = NoteOff {
            channel: note.channel,
            pitch: note.pitch,
            velocity: velocity_off,
        }.into();

        self.update(MidiMessageWithDelta {
            delta_frames,
            data: raw_message.into(),
        }, current_time, None);
    }

    pub fn push_note_on(&mut self, pattern: &Pattern, note: &Note, current_time: usize) {
        let pitch = match pattern.transpose(note.pitch) {
            None => { return; }
            Some(pitch) => pitch
        };

        for raw_message in (ExpressiveNote {
            channel: pattern.channel,
            pitch,
            velocity: pattern.velocity,
            pressure: pattern.pressure,
            timbre: pattern.timbre,
            pitchbend: pattern.pitchbend,
        }).into_rawmessages() {
            self.update(
                MidiMessageWithDelta {
                    delta_frames: (pattern.pressed_at - current_time) as u16,
                    data: raw_message.into(),
                },
                current_time,
                Some(pattern.id),
            );
        }
    }
}
