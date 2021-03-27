use std::sync::Mutex;

use vst::plugin::HostCallback;
use vst::util::ParameterTransfer;

use util::debug::DebugSocket;
use util::parameter_value_conversion::{f32_to_bool, f32_to_byte};
use util::parameters::ParameterConversion;
use util::{duration_display, HostCallbackLock};
use util::delayed_message_consumer::MaxNotesParameter;

const PARAMETER_COUNT: usize = 3;

pub struct NoteOffDelayPluginParameters {
    pub host_mutex: Mutex<HostCallbackLock>,
    pub transfer: ParameterTransfer,
}

#[repr(i32)]
pub enum Parameter {
    Delay = 0,
    MaxNotes,
    MaxNotesAppliesToDelayedNotesOnly,
}

impl From<i32> for Parameter {
    fn from(i: i32) -> Self {
        match i {
            0 => Parameter::Delay,
            1 => Parameter::MaxNotes,
            2 => Parameter::MaxNotesAppliesToDelayedNotesOnly,
            _ => panic!("no such parameter {}", i),
        }
    }
}

impl Into<i32> for Parameter {
    fn into(self) -> i32 {
        self as i32
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
            i => MaxNotesParameter::Limited(i)
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

impl vst::plugin::PluginParameters for NoteOffDelayPluginParameters {
    fn get_parameter_text(&self, index: i32) -> String {
        match index.into() {
            Parameter::Delay => {
                let value = self.get_exponential_scale_parameter(Parameter::Delay, 10., 20.);

                if value > 0.0 {
                    duration_display(value)
                } else {
                    "Off".to_string()
                }
            }
            Parameter::MaxNotes => {
                if self.get_parameter(Parameter::MaxNotes as i32) == 0.0 {
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
        }
    }

    fn get_parameter_name(&self, index: i32) -> String {
        match index.into() {
            Parameter::Delay => "Delay",
            Parameter::MaxNotes => "Max Notes",
            Parameter::MaxNotesAppliesToDelayedNotesOnly => "Apply max notes to delayed notes only",
        }
        .to_string()
    }

    fn get_parameter(&self, index: i32) -> f32 {
        self.get_parameter_transfer().get_parameter(index as usize)
    }

    fn set_parameter(&self, index: i32, value: f32) {
        match index.into() {
            Parameter::Delay => {
                DebugSocket::send(&*format!("Parameter {} set to {}", index, value));
                let old_value = self.get_parameter(index);
                if (value - old_value).abs() > 0.0001 {
                    self.transfer.set_parameter(index as usize, value)
                }
            }
            Parameter::MaxNotes => {
                let old_value = self.get_max_notes();
                let byte_value = f32_to_byte(value) ;
                let max_notes = match byte_value / 4 {
                    0 => MaxNotesParameter::Infinite,
                    i => MaxNotesParameter::Limited(i)
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
