#[allow(unused_imports)]
use {
    async_channel::Sender,
    log::{error, info},
    std::mem::take,
};

use util::messages::{NoteOff, PitchBend};
use util::midi_message_with_delta::MidiMessageWithDelta;
use util::raw_message::RawMessage;

use crate::midi_messages::device::Device;
use crate::midi_messages::expressive_note::ExpressiveNote;
use crate::midi_messages::note::Note;
use crate::midi_messages::pattern::Pattern;
#[cfg(not(feature = "midi_hack_transmission"))]
use crate::workers::main_worker::WorkerCommand;
use crate::parameters::VelocitySource;

pub(crate) struct DeviceOut {
    device: Device,
    pub output_queue: Vec<MidiMessageWithDelta>,
}

impl DeviceOut {
    pub(crate) fn legato(&mut self, old_id: usize, new_id: usize) {
        if let Some(note) = self.device.notes.values_mut().find(|note| note.id == old_id) {
            note.id = new_id;
        }
    }

    pub fn new(name: String) -> Self {
        Self {
            device: Device::new(name),
            output_queue: vec![],
        }
    }

    pub fn update(&mut self, midi_message: MidiMessageWithDelta, current_time: usize, id: Option<usize>) {
        self.output_queue.push(midi_message);
        self.device.push(midi_message, current_time, id);
        if !self.device.notes.is_empty() {
            #[cfg(feature = "device_debug")]
            info!("Device out state after update: {:2X?}", self.device.notes)
        }
    }

    #[cfg(not(feature = "midi_hack_transmission"))]
    pub fn flush_to(&mut self, reception_time: u64, midi_output_sender: &Sender<WorkerCommand>) {
        if self.output_queue.is_empty() {
            return;
        }

        {
            midi_output_sender
                .try_send(WorkerCommand::SendToMidiOutput {
                    reception_time,
                    messages: take(&mut self.output_queue),
                })
                .unwrap_or_else(|err| error!("Could not send to the controller worker {}", err));
        }
    }

    pub fn find_by_note_id(&self, note_id: usize) -> Option<&Note> {
        self.device.notes.values().find(|note| note.id == note_id)
    }

    pub fn update_pitch(&mut self, note_id: usize, increment: i32, delta_frames: u16, current_time: usize) {
        match self.find_by_note_id(note_id) {
            None => {
                info!(
                    "Cannot find note to pitchbend. Required note_id: {}. Current notes: {:02X?}",
                    note_id,
                    self.device.notes.values()
                )
            }
            Some(note) => {
                let raw_message: RawMessage = PitchBend {
                    channel: note.channel,
                    millisemitones: increment
                }.into();

                self.update(
                    MidiMessageWithDelta {
                        delta_frames,
                        data: raw_message,
                    },
                    current_time,
                    None,
                );
            }
        }
    }

    pub fn push_note_off(&mut self, note_id: usize, velocity_off: u8, delta_frames: u16, current_time: usize) {
        let note = match self.device.notes.values().find(|note| note.id == note_id) {
            None => {
                #[cfg(feature = "device_debug")]
                info!("Cannot find note to stop: {:02x?}", note_id);
                return;
            }
            Some(note) => note,
        };

        let raw_message: RawMessage = NoteOff {
            channel: note.channel,
            pitch: note.pitch,
            velocity: velocity_off,
        }
        .into();

        self.update(
            MidiMessageWithDelta {
                delta_frames,
                data: raw_message,
            },
            current_time,
            None,
        );
    }

    pub fn push_note_on(&mut self, pattern: &Pattern, note: &Note, current_time: usize, velocity_source: VelocitySource) {
        let pitch = match pattern.transpose(note.pitch) {
            None => {
                return;
            }
            Some(pitch) => pitch,
        };

        let velocity = match velocity_source {
            VelocitySource::Pattern => pattern.velocity,
            VelocitySource::Mixed(x) => (pattern.velocity as f32 * (1. - x) + note.velocity as f32 * x) as u8,
            VelocitySource::Notes => note.velocity
        };

        for raw_message in (ExpressiveNote {
            channel: pattern.channel,
            pitch,
            velocity,
            pressure: pattern.pressure,
            timbre: pattern.timbre,
            pitchbend: pattern.pitchbend,
        }).into_rawmessages() {
            self.update(
                MidiMessageWithDelta {
                    delta_frames: (pattern.pressed_at - current_time) as u16,
                    data: raw_message,
                },
                current_time,
                Some(pattern.id),
            );
        }
    }
}
