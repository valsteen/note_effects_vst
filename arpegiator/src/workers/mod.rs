#[cfg(not(feature = "midi_hack_transmission"))]
pub mod ipc_worker;
#[cfg(not(feature = "midi_hack_transmission"))]
pub mod main_worker;
#[cfg(not(feature = "midi_hack_transmission"))]
mod midi_output_worker;
