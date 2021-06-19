extern crate global_counter;

use vst::event::MidiEvent;
use vst::plugin::HostCallback;
use std::fmt::{Display, Formatter};
use std::fmt;
use crate::parameters::get_exponential_scale_value;

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

impl Display for Duration {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Duration::Off => "off".to_string(),
            Duration::Duration(seconds) => {
                duration_display(*seconds)
            }
        }.fmt(f)
    }
}

pub enum Duration {
    Off,
    Duration(f32),
}


impl From<f32> for Duration {
    fn from(parameter_value: f32) -> Self {
        match get_exponential_scale_value(parameter_value, 10., 20.) {
            x if x == 0.0 => Duration::Off,
            value => Duration::Duration(value)
        }
    }
}

pub enum SyncDuration {
    Off,
    Subdivision(u8, u8),
}


impl From<f32> for SyncDuration {
    fn from(value: f32) -> Self {
        let value = (value * 10.0) as u8;

        match value {
            0 => SyncDuration::Off,
            1 => SyncDuration::Subdivision(1, 128),
            2 => SyncDuration::Subdivision(1, 64),
            3 => SyncDuration::Subdivision(1, 32),
            4 => SyncDuration::Subdivision(1, 16),
            5 => SyncDuration::Subdivision(1, 8),
            6 => SyncDuration::Subdivision(3, 16),
            7 => SyncDuration::Subdivision(1, 4),
            8 => SyncDuration::Subdivision(1, 2),
            9 => SyncDuration::Subdivision(3, 4),
            _ => SyncDuration::Subdivision(1, 1),
        }
    }
}

impl Display for SyncDuration {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self {
            SyncDuration::Off => "off".to_string(),
            SyncDuration::Subdivision(numerator, denominator) => {
                match (numerator, denominator) {
                    (1, 1) => "1".to_string(),
                    (n, d) => format!("{}/{}", n, d)
                }
            }
        }.fmt(f)
    }
}


impl SyncDuration {
    // this considers a 4/4 signature ( 4 beats per bar )

    pub fn delay_to_samples(&self, bpm: f64, sample_rate: f32) -> Option<usize> {
        match self {
            SyncDuration::Off => None,
            SyncDuration::Subdivision(numerator, denominator) =>
                Some((((*numerator as f64 / *denominator as f64 * 4.0) / bpm * 60.) as f32 * sample_rate) as usize),
        }
    }
}