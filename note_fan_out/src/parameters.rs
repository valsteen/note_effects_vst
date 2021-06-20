use std::sync::Mutex;
use util::parameter_value_conversion::f32_to_byte;
use util::parameters::ParameterConversion;
use util::HostCallbackLock;
use vst::plugin::{HostCallback, PluginParameters};
use vst::util::ParameterTransfer;

pub struct NoteFanoutParameters {
    pub host: Mutex<HostCallbackLock>,
    pub transfer: ParameterTransfer,
}

#[repr(i32)]
#[derive(Copy)]
pub enum Parameter {
    Steps = 0,
    Selection,
    ChannelDistribute,
}

impl Clone for Parameter {
    fn clone(&self) -> Self {
        let value = *self as i32;
        Parameter::from(value)
    }

    fn clone_from(&mut self, source: &Self) {
        let value = *source as i32;
        *self = Parameter::from(value)
    }
}

impl From<i32> for Parameter {
    fn from(i: i32) -> Self {
        match i {
            0 => Parameter::Steps,
            1 => Parameter::Selection,
            2 => Parameter::ChannelDistribute,
            _ => panic!("No such Parameter {}", i),
        }
    }
}

impl From<Parameter> for i32 {
    fn from(p: Parameter) -> Self {
        p as i32
    }
}

impl NoteFanoutParameters {
    pub fn get_channel_distribution(&self, parameter: Parameter) -> ChannelDistribution {
        ChannelDistribution::from(self.transfer.get_parameter(parameter as usize))
    }
}

impl ParameterConversion<Parameter> for NoteFanoutParameters {
    fn get_parameter_transfer(&self) -> &ParameterTransfer {
        &self.transfer
    }

    fn get_parameter_count() -> usize {
        3
    }
}

impl NoteFanoutParameters {
    pub fn new(host: HostCallback) -> Self {
        NoteFanoutParameters {
            host: Mutex::new(HostCallbackLock { host }),
            transfer: ParameterTransfer::new(3),
        }
    }
}

pub enum ChannelDistribution {
    Channels(u8),
    Off,
}

impl From<f32> for ChannelDistribution {
    fn from(i: f32) -> Self {
        let channels_value: u8 = ((i - 1. / 15.) / 14. * 15. * 13. + 2.0) as u8;

        if channels_value < 2 {
            ChannelDistribution::Off
        } else {
            ChannelDistribution::Channels(channels_value) // channel 0 ( displayed as 1 ) is reserved for MPE
        }
    }
}

impl From<ChannelDistribution> for f32 {
    fn from(cd: ChannelDistribution) -> f32 {
        match cd {
            ChannelDistribution::Channels(value) => {
                // normalize over 15 values, first range is off
                ((value as f32 - 2.) / 13.) * 14. / 15. + 1. / 15.
            }
            ChannelDistribution::Off => 0.0,
        }
    }
}

impl PluginParameters for NoteFanoutParameters {
    fn get_parameter_text(&self, index: i32) -> String {
        match Parameter::from(index) {
            Parameter::Steps => {
                let value = self.get_byte_parameter(Parameter::Steps) / 8;
                if value == 0 {
                    "off".to_string()
                } else {
                    format!("{}", value)
                }
            }
            Parameter::Selection => {
                format!("{}", self.get_byte_parameter(Parameter::Selection) / 8)
            }
            Parameter::ChannelDistribute => match self.get_channel_distribution(Parameter::ChannelDistribute) {
                ChannelDistribution::Channels(c) => format!("{}", c),
                ChannelDistribution::Off => "Off".to_string(),
            },
        }
    }

    fn get_parameter_name(&self, index: i32) -> String {
        match Parameter::from(index as i32) {
            Parameter::Steps => "Steps",
            Parameter::Selection => "Selection",
            Parameter::ChannelDistribute => "Channel distribution",
        }
        .to_string()
    }

    fn get_parameter(&self, index: i32) -> f32 {
        self.transfer.get_parameter(index as usize)
    }

    fn set_parameter(&self, index: i32, value: f32) {
        let parameter = Parameter::from(index);
        match parameter {
            Parameter::Steps | Parameter::Selection => {
                let new_value = f32_to_byte(value) / 8;
                let old_value = self.get_byte_parameter(parameter) / 8;

                if new_value != old_value {
                    self.transfer.set_parameter(index as usize, value)
                }
            }
            Parameter::ChannelDistribute => {
                self.transfer.set_parameter(index as usize, value);
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

impl Default for NoteFanoutParameters {
    fn default() -> Self {
        let parameters = NoteFanoutParameters {
            host: Default::default(),
            transfer: ParameterTransfer::new(3),
        };
        parameters.set_byte_parameter(Parameter::Steps, 0);
        parameters.set_byte_parameter(Parameter::Selection, 0);
        parameters.set_byte_parameter(Parameter::ChannelDistribute, 0);
        parameters
    }
}
