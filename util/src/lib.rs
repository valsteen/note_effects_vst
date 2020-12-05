use vst::event::MidiEvent;
use vst::plugin::HostCallback;

pub mod constants;
pub mod debug;
pub mod parameter_value_conversion;
pub mod parameters;

#[derive(Default)]
pub struct HostCallbackLock {
    pub host: HostCallback,
}

pub fn make_midi_event(bytes: [u8; 3], delta_frames: i32) -> MidiEvent {
    MidiEvent {
        data: bytes,
        delta_frames,
        live: true,
        note_length: None,
        note_offset: None,
        detune: 0,
        note_off_velocity: 0,
    }
}
