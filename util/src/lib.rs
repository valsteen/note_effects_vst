extern crate global_counter;

use vst::event::MidiEvent;
use vst::plugin::HostCallback;

pub mod constants;
pub mod debug;
pub mod parameter_value_conversion;
pub mod parameters;
pub mod messages;
pub mod absolute_time_midi_message;
pub mod midi_message_type;
pub mod raw_message;
pub mod absolute_time_midi_message_vector;
pub mod delayed_message_consumer;

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

pub fn duration_display(value: f32) -> String {
    let mut out = String::new();
    let mut _value = value;
    if _value >= 1.0 {
        out += &*format!("{:.0}s ", value);
        _value -= value.trunc();
    }
    if _value > 0.0 {
        out += &*format!("{:3.0}ms", value * 1000.0);
    }
    out
}
