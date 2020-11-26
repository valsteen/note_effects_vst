use std::sync::Mutex;

use vst::plugin::HostCallback;
use vst::util::ParameterTransfer;

use util::debug::DebugSocket;
use util::parameter_value_conversion::{byte_to_f32, f32_to_byte};
use util::HostCallbackLock;

const PARAMETER_COUNT: usize = 1;

pub struct NoteOffDelayPluginParameters {
    pub host_mutex: Mutex<HostCallbackLock>,
    pub transfer: ParameterTransfer,
    pub debug_socket: Mutex<DebugSocket>,
}

impl NoteOffDelayPluginParameters {
    pub const DELAY: i32 = 0;

    pub fn new(host: HostCallback) -> Self {
        return NoteOffDelayPluginParameters {
            host_mutex: Mutex::new(HostCallbackLock { host }),
            debug_socket: Mutex::new(DebugSocket::default()),
            ..Default::default()
        };
    }

    #[inline]
    pub fn get_byte_parameter(&self, index: i32) -> u8 {
        f32_to_byte(self.transfer.get_parameter(index as usize))
    }

    #[inline]
    fn set_byte_parameter(&self, index: i32, value: u8) {
        self.transfer
            .set_parameter(index as usize, byte_to_f32(value))
    }

    #[inline]
    pub fn debug(&self, s: &str) {
        if let Ok(mut debug) = self.debug_socket.lock() {
            debug.send(s);
        }
    }
}

impl Default for NoteOffDelayPluginParameters {
    fn default() -> Self {
        let parameters = NoteOffDelayPluginParameters {
            host_mutex: Default::default(),
            transfer: ParameterTransfer::new(PARAMETER_COUNT),
            debug_socket: Mutex::new(DebugSocket::default()),
        };
        parameters
    }
}

impl vst::plugin::PluginParameters for NoteOffDelayPluginParameters {
    fn get_parameter_text(&self, index: i32) -> String {
        match index {
            Self::DELAY => format!("{}", self.get_parameter(index)),
            _ => "".to_string(),
        }
    }

    fn get_parameter_name(&self, index: i32) -> String {
        match index {
            Self::DELAY => "Delay",
            _ => "",
        }
        .to_string()
    }

    fn get_parameter(&self, index: i32) -> f32 {
        self.transfer.get_parameter(index as usize)
    }

    fn set_parameter(&self, index: i32, value: f32) {
        match index {
            Self::DELAY => {
                self.debug(&*format!("Parameter {} set to {}", index, value));
                let old_value = self.get_parameter(index);
                if value != old_value {
                    self.transfer.set_parameter(index as usize, value)
                }
            }
            _ => {}
        }
    }

    fn get_preset_data(&self) -> Vec<u8> {
        (0..PARAMETER_COUNT)
            .map(|i| self.get_byte_parameter(i as i32))
            .collect()
    }

    fn get_bank_data(&self) -> Vec<u8> {
        (0..PARAMETER_COUNT)
            .map(|i| self.get_byte_parameter(i as i32))
            .collect()
    }

    fn load_preset_data(&self, data: &[u8]) {
        for (i, item) in data.iter().enumerate() {
            self.set_byte_parameter(i as i32, *item);
        }
    }

    fn load_bank_data(&self, data: &[u8]) {
        for (i, item) in data.iter().enumerate() {
            self.set_byte_parameter(i as i32, *item);
        }
    }
}
