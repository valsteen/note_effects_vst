#[allow(unused_imports)]
use {
    async_channel::Sender,
    log::{error, info},
    std::sync::Mutex,
    util::parameter_value_conversion::{f32_to_bool, f32_to_byte},
};

use vst::plugin::PluginParameters;
use vst::util::ParameterTransfer;

#[cfg(not(feature = "midi_hack_transmission"))]
use crate::workers::main_worker::WorkerCommand;
use util::duration_display;
use util::parameters::{ParameterConversion, get_exponential_scale_value};
use util::system::Uuid;

#[cfg(not(feature = "midi_hack_transmission"))]
pub const PARAMETER_COUNT: usize = 5;

#[cfg(feature = "midi_hack_transmission")]
pub const PARAMETER_COUNT: usize = 4;

#[cfg(not(feature = "midi_hack_transmission"))]
const BASE_PORT: u16 = 6000;

pub(crate) enum PitchBendValues {
    Off, // no pitchbend, means same pitch until pattern ends
    DurationToReachTarget(f32),
    Immediate,
}

impl From<f32> for PitchBendValues {
    fn from(x: f32) -> Self {
        match x {
            x if x <= 0. => PitchBendValues::Immediate,
            x if x >= 1. => PitchBendValues::Off,
            _ => {
                let value = get_exponential_scale_value(x, 5., 80.);
                PitchBendValues::DurationToReachTarget(value)
            }
        }
    }
}

// TODO trait to string, change parameter to string in parameters trait

impl ToString for PitchBendValues {
    fn to_string(&self) -> String {
        match self {
            PitchBendValues::Off => "Off".to_string(),
            PitchBendValues::DurationToReachTarget(x) => duration_display(*x),
            PitchBendValues::Immediate => "Immediate".to_string()
        }
    }
}

pub(crate) enum VelocitySource {
    Pattern,
    Mixed(f32),
    Notes
}


impl ToString for VelocitySource {
    fn to_string(&self) -> String {
        match self {
            VelocitySource::Pattern => "Pattern".to_string(),
            VelocitySource::Mixed(x) => format!("{}%", (x * 100.) as u8),
            VelocitySource::Notes => "Notes".to_string()
        }
    }
}

impl From<f32> for VelocitySource {
    fn from(x: f32) -> Self {
        match x {
            x if x <= 0. => VelocitySource::Pattern,
            x if x >= 1. => VelocitySource::Notes,
            x => {
                VelocitySource::Mixed(x)
            }
        }
    }
}


pub(crate) struct ArpegiatorParameters {
    pub transfer: ParameterTransfer,
    #[cfg(not(feature = "midi_hack_transmission"))]
    pub worker_commands: Mutex<Option<Sender<WorkerCommand>>>,
}

impl ArpegiatorParameters {
    #[cfg(not(feature = "midi_hack_transmission"))]
    pub fn get_port(&self) -> u16 {
        BASE_PORT + self.get_byte_parameter(Parameter::PortIndex) as u16
    }

    #[cfg(not(feature = "midi_hack_transmission"))]
    pub fn update_port(&self, event_id: Uuid) {
        let port = self.get_byte_parameter(Parameter::PortIndex) as u16 + BASE_PORT;
        info!("Applying parameter change: port={}", port);
        if let Err(error) = self
            .worker_commands
            .lock()
            .unwrap()
            .as_ref()
            .unwrap()
            .try_send(WorkerCommand::SetPort(port, event_id))
        {
            info!(
                "[{}] main worker is shutdown - ignoring port change ({})",
                event_id, error
            );
        }
    }
}

#[repr(i32)]
pub enum Parameter {
    HoldNotes = 0, // a started pattern will find a note to play, even if no note is playing for that index
    PatternLegato, // pattern is not restarted if start/end match, and note is thus held
    Pitchbend,
    VelocitySource,
    #[cfg(not(feature = "midi_hack_transmission"))]
    PortIndex,
}

impl From<i32> for Parameter {
    fn from(i: i32) -> Self {
        match i {
            0 => Parameter::HoldNotes,
            1 => Parameter::PatternLegato,
            2 => Parameter::Pitchbend,
            3 => Parameter::VelocitySource,
            #[cfg(not(feature = "midi_hack_transmission"))]
            4 => Parameter::PortIndex,
            _ => panic!("no such parameter {}", i),
        }
    }
}

impl Into<i32> for Parameter {
    fn into(self) -> i32 {
        self as i32
    }
}

impl ParameterConversion<Parameter> for ArpegiatorParameters {
    fn get_parameter_transfer(&self) -> &ParameterTransfer {
        &self.transfer
    }

    fn get_parameter_count() -> usize {
        PARAMETER_COUNT
    }
}

impl ArpegiatorParameters {
    pub fn new() -> Self {
        let parameters = ArpegiatorParameters {
            transfer: ParameterTransfer::new(PARAMETER_COUNT),
            #[cfg(not(feature = "midi_hack_transmission"))]
            worker_commands: Mutex::new(None),
        };
        parameters.set_parameter(Parameter::PatternLegato.into(), 1.);
        parameters
    }

    pub fn get_pitchbend(&self) -> PitchBendValues {
        PitchBendValues::from(self.get_parameter(Parameter::Pitchbend.into()))
    }

    pub fn get_velocitysource(&self) -> VelocitySource {
        VelocitySource::from(self.get_parameter(Parameter::VelocitySource.into()))
    }
}

impl PluginParameters for ArpegiatorParameters {
    fn get_parameter_text(&self, index: i32) -> String {
        let parameter = index.into();
        match parameter {
            #[cfg(not(feature = "midi_hack_transmission"))]
            Parameter::PortIndex => self.get_port().to_string(),
            Parameter::HoldNotes | Parameter::PatternLegato => match self.get_bool_parameter(parameter) {
                true => "On",
                false => "Off",
            }
            .to_string(),
            Parameter::Pitchbend => self.get_pitchbend().to_string(),
            Parameter::VelocitySource => self.get_velocitysource().to_string()
        }
    }

    fn get_parameter_name(&self, index: i32) -> String {
        match index.into() {
            #[cfg(not(feature = "midi_hack_transmission"))]
            Parameter::PortIndex => "Port",
            Parameter::HoldNotes => "Hold notes",
            Parameter::PatternLegato => "Pattern Legato",
            Parameter::Pitchbend => "Use pitchbend",
            Parameter::VelocitySource => "Velocity",
        }
        .to_string()
    }

    fn get_parameter(&self, index: i32) -> f32 {
        self.get_parameter_transfer().get_parameter(index as usize)
    }

    fn set_parameter(&self, index: i32, value: f32) {
        let parameter = index.into();
        match parameter {
            #[cfg(not(feature = "midi_hack_transmission"))]
            Parameter::PortIndex => {
                let new_value = f32_to_byte(value);
                let old_value = self.get_byte_parameter(Parameter::PortIndex);
                if old_value != new_value {
                    let event_id = Uuid::new_v4();
                    info!("[{}] set parameter port {}", event_id, BASE_PORT + new_value as u16);
                    self.transfer.set_parameter(index as usize, value);
                    self.update_port(event_id)
                }
            }
            Parameter::HoldNotes | Parameter::PatternLegato => {
                let new_value = f32_to_bool(value);
                let old_value = self.get_bool_parameter(parameter);
                if old_value != new_value {
                    self.transfer.set_parameter(index as usize, value);
                }
            }
            Parameter::Pitchbend => self.transfer.set_parameter(index as usize, value),
            Parameter::VelocitySource => self.transfer.set_parameter(index as usize, value),
        }
    }

    fn get_preset_data(&self) -> Vec<u8> {
        self.serialize_state()
    }

    fn get_bank_data(&self) -> Vec<u8> {
        self.serialize_state()
    }

    fn load_preset_data(&self, data: &[u8]) {
        let event_id = Uuid::new_v4();
        info!("[{}] Load present data", event_id);
        self.deserialize_state(data);
        #[cfg(not(feature = "midi_hack_transmission"))]
        {
            self.update_port(event_id)
        }
    }

    fn load_bank_data(&self, data: &[u8]) {
        let event_id = Uuid::new_v4();
        info!("[{}] Load bank data", event_id);
        self.deserialize_state(data);
        #[cfg(not(feature = "midi_hack_transmission"))]
        {
            self.update_port(event_id)
        }
    }
}
