extern crate global_counter;

use vst::event::MidiEvent;
use vst::plugin::HostCallback;

pub mod absolute_time_midi_message;
pub mod absolute_time_midi_message_vector;
pub mod constants;
pub mod delayed_message_consumer;
pub mod ipc_payload;
pub mod logging;
pub mod messages;
pub mod midi_message_type;
pub mod midi_message_with_delta;
pub mod parameter_value_conversion;
pub mod parameters;
pub mod raw_message;
pub mod system;
pub mod transmute_buffer;

#[derive(Default)]
pub struct HostCallbackLock {
    pub host: HostCallback,
}

pub fn make_midi_message(bytes: [u8; 3], delta_frames: i32) -> MidiEvent {
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

pub fn duration_display(seconds: f32) -> String {
    let mut out = String::new();
    let mut _value = seconds;
    if _value >= 1.0 {
        out += &*format!("{:.0}s ", seconds);
        _value -= seconds.trunc();
    }
    if _value > 0.0 {
        out += &*format!("{:3.0}ms", seconds * 1000.0);
    }
    out
}
