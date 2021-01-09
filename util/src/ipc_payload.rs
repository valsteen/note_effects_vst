use serde::{Deserialize, Serialize};

use crate::midi_message_with_delta::MidiMessageWithDelta;
use ipc_channel::ipc::IpcSender;


#[derive(Debug, Serialize, Deserialize)]
pub struct PatternPayload {
    #[cfg(target_os = "macos")]
    pub time: u64,
    pub messages: Vec<MidiMessageWithDelta>,
}


#[derive(Debug, Serialize, Deserialize)]
pub enum IPCCommand {
    PatternPayload(PatternPayload),
    // stop is only used locally, but send over an IPC channel so the worker can listen both on the remote IPC
    // for new patterns, and on local IPC for stopping the worker
    Stop(IpcSender<()>),
    Ping
}
