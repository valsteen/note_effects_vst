use std::sync::Mutex;

use vst::plugin::{HostCallback, PluginParameters};
use vst::util::ParameterTransfer;

use std::fmt;
use std::fmt::{Display, Formatter};
use util::delayed_message_consumer::MaxNotesParameter;
use util::parameter_value_conversion::{f32_to_bool, f32_to_byte};
use util::parameters::{get_exponential_scale_value, ParameterConversion};
use util::{Duration, HostCallbackLock};

pub const PARAMETER_COUNT: usize = 5;

pub struct NoteOffDelayPluginParameters {
    pub host_mutex: Mutex<HostCallbackLock>,
    pub transfer: ParameterTransfer,
}

#[repr(i32)]
pub enum Parameter {
    DelayOffset = 0,
    MaxNotes,
    MaxNotesAppliesToDelayedNotesOnly,
    MultiplyLength,
    Threshold,
}

impl From<i32> for Parameter {
    fn from(i: i32) -> Self {
        match i {
            0 => Parameter::DelayOffset,
            1 => Parameter::MaxNotes,
            2 => Parameter::MaxNotesAppliesToDelayedNotesOnly,
            3 => Parameter::MultiplyLength,
            4 => Parameter::Threshold,
            _ => panic!("no such parameter {}", i),
        }
    }
}

impl From<Parameter> for i32 {
    fn from(p: Parameter) -> Self {
        p as i32
    }
}

impl ParameterConversion<Parameter> for NoteOffDelayPluginParameters {
    fn get_parameter_transfer(&self) -> &ParameterTransfer {
        &self.transfer
    }

    fn get_parameter_count() -> usize {
        PARAMETER_COUNT
    }
}

impl NoteOffDelayPluginParameters {
    pub fn new(host: HostCallback) -> Self {
        NoteOffDelayPluginParameters {
            host_mutex: Mutex::new(HostCallbackLock { host }),
            ..Default::default()
        }
    }

    pub fn get_max_notes(&self) -> MaxNotesParameter {
        match self.get_byte_parameter(Parameter::MaxNotes) / 4 {
            0 => MaxNotesParameter::Infinite,
            i => MaxNotesParameter::Limited(i),
        }
    }

    pub fn get_delay(&self) -> Delay {
        Delay {
            offset: Duration::from(self.get_parameter(Parameter::DelayOffset.into())),
            multiplier: DelayMultiplier::from(self.get_parameter(Parameter::MultiplyLength.into())),
            threshold: Duration::from(self.get_parameter(Parameter::Threshold.into())),
        }
    }
}

impl Default for NoteOffDelayPluginParameters {
    fn default() -> Self {
        NoteOffDelayPluginParameters {
            host_mutex: Default::default(),
            transfer: ParameterTransfer::new(PARAMETER_COUNT),
        }
    }
}

pub struct Delay {
    pub offset: Duration,
    pub multiplier: DelayMultiplier,
    pub threshold: Duration,
}

pub enum DelayMultiplier {
    Off,
    Multiplier(f32),
}

impl Delay {
    pub fn is_active(&self) -> bool {
        !matches!((&self.offset, &self.multiplier), (Duration::Off, DelayMultiplier::Off))
    }

    pub fn apply(&self, duration_in_samples: usize, sample_rate: f32) -> Option<usize> {
        if self.is_active() {
            if let Duration::Duration(threshold) = self.threshold {
                let threshold_in_samples = (threshold * sample_rate) as usize;
                if duration_in_samples < threshold_in_samples {
                    return Some(duration_in_samples);
                }
            }

            let duration_in_samples = match self.multiplier {
                DelayMultiplier::Off => duration_in_samples as f32,
                DelayMultiplier::Multiplier(x) => x * duration_in_samples as f32,
            };

            Some(match self.offset {
                Duration::Off => duration_in_samples as usize,
                Duration::Duration(x) => (duration_in_samples + x * sample_rate) as usize,
            })
        } else {
            None
        }
    }
}

impl Display for DelayMultiplier {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self {
            DelayMultiplier::Off => "off".to_string(),
            DelayMultiplier::Multiplier(multiplier) => {
                format!("{:.3}x", multiplier)
            }
        }
        .fmt(f)
    }
}

impl From<f32> for DelayMultiplier {
    fn from(parameter_value: f32) -> Self {
        match get_exponential_scale_value(parameter_value, 19., 20.) {
            x if x == 0.0 => DelayMultiplier::Off,
            value => DelayMultiplier::Multiplier(1.0 + value),
        }
    }
}

impl vst::plugin::PluginParameters for NoteOffDelayPluginParameters {
    fn get_parameter_text(&self, index: i32) -> String {
        let value = self.get_parameter(index);
        match index.into() {
            Parameter::DelayOffset | Parameter::Threshold => Duration::from(value).to_string(),

            Parameter::MaxNotes => {
                if value == 0.0 {
                    "Off".to_string()
                } else {
                    format!("{}", self.get_max_notes())
                }
            }

            Parameter::MaxNotesAppliesToDelayedNotesOnly => {
                if self.get_bool_parameter(Parameter::MaxNotesAppliesToDelayedNotesOnly) {
                    "On"
                } else {
                    "Off"
                }
                .to_string()
            }
            Parameter::MultiplyLength => DelayMultiplier::from(value).to_string(),
        }
    }

    fn get_parameter_name(&self, index: i32) -> String {
        match index.into() {
            Parameter::DelayOffset => "Delay",
            Parameter::MaxNotes => "Max Notes",
            Parameter::MaxNotesAppliesToDelayedNotesOnly => "Apply max notes to delayed notes only",
            Parameter::MultiplyLength => "Length multiplier",
            Parameter::Threshold => "Threshold",
        }
        .to_string()
    }

    fn get_parameter(&self, index: i32) -> f32 {
        self.get_parameter_transfer().get_parameter(index as usize)
    }

    fn set_parameter(&self, index: i32, value: f32) {
        match index.into() {
            Parameter::DelayOffset | Parameter::MultiplyLength | Parameter::Threshold => {
                let old_value = self.get_parameter(index);
                if (value - old_value).abs() > 0.0001 || (value == 0.0 && old_value != 0.0) {
                    self.transfer.set_parameter(index as usize, value)
                }
            }
            Parameter::MaxNotes => {
                let old_value = self.get_max_notes();
                let byte_value = f32_to_byte(value);
                let max_notes = match byte_value / 4 {
                    0 => MaxNotesParameter::Infinite,
                    i => MaxNotesParameter::Limited(i),
                };
                if max_notes != old_value {
                    self.set_byte_parameter(Parameter::MaxNotes, byte_value);
                }
            }
            Parameter::MaxNotesAppliesToDelayedNotesOnly => {
                self.set_bool_parameter(Parameter::MaxNotesAppliesToDelayedNotesOnly, f32_to_bool(value))
            }
        }
    }

    fn get_preset_data(&self) -> Vec<u8> {
        self.serialize_state()
    }

    fn get_bank_data(&self) -> Vec<u8> {
        self.serialize_state()
    }

    fn load_preset_data(&self, data: &[u8]) {
        self.deserialize_state(data)
    }

    fn load_bank_data(&self, data: &[u8]) {
        self.deserialize_state(data)
    }
}
