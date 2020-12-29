mod parameters;

#[macro_use]
extern crate vst;

use std::sync::Arc;

use vst::buffer::{AudioBuffer, SendEventBuffer};
use vst::plugin::{CanDo, Category, HostCallback, Info, Plugin};
use vst::event::{Event, MidiEvent};
use vst::api::Events;

use parameters::MaxNoteDurationPluginParameters;
use parameters::Parameter;
use util::midi_message_type::MidiMessageType;
use util::parameters::ParameterConversion;
use std::collections::HashSet;
use std::hash::{Hash, Hasher};

plugin_main!(MaxNoteDurationPlugin);

#[derive(Eq, Clone, Copy)]
struct PlayingNote {
    channel: u8,
    pitch: u8,
    deadline: usize,
}

impl PartialEq for PlayingNote {
    fn eq(&self, other: &Self) -> bool {
        self.channel == other.channel && self.pitch == other.pitch
    }
}


impl Hash for PlayingNote {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.channel.hash(state);
        self.pitch.hash(state);
    }
}


pub struct MaxNoteDurationPlugin {
    current_time_in_samples: usize,
    parameters: Arc<MaxNoteDurationPluginParameters>,
    sample_rate: f32,
    send_buffer: SendEventBuffer,
    current_playing_notes: HashSet<PlayingNote>,
    host: HostCallback,
    block_size: usize,
    events: Vec<MidiEvent>,
}

impl Default for MaxNoteDurationPlugin {
    fn default() -> Self {
        MaxNoteDurationPlugin {
            send_buffer: Default::default(),
            parameters: Arc::new(Default::default()),
            sample_rate: 44100.0,
            current_time_in_samples: 0,
            current_playing_notes: Default::default(),
            host: Default::default(),
            block_size: 0,
            events: vec![],
        }
    }
}


impl MaxNoteDurationPlugin {
    #[allow(dead_code)]
    fn seconds_per_sample(&self) -> f32 {
        1.0 / self.sample_rate
    }

    fn seconds_to_samples(&self, seconds: f32) -> usize {
        (seconds * self.sample_rate) as usize
    }

    fn increase_time_in_samples(&mut self, samples: usize) {
        let new_time_in_samples = self.current_time_in_samples + samples;
        self.current_time_in_samples = new_time_in_samples;
    }
}


impl Plugin for MaxNoteDurationPlugin {
    fn get_info(&self) -> Info {
        Info {
            name: "Max note duration".to_string(),
            vendor: "DJ Crontab".to_string(),
            unique_id: 231213173,
            parameters: 1,
            category: Category::Effect,
            initial_delay: 0,
            version: 1,
            inputs: 0,
            outputs: 0,
            midi_inputs: 1,
            f64_precision: false,
            presets: 1,
            midi_outputs: 1,
            preset_chunks: true,
            silent_when_stopped: true,
        }
    }

    fn set_block_size(&mut self, block_size: i64) {
        self.block_size = block_size as usize
    }

    fn new(host: HostCallback) -> Self {
        let parameters = MaxNoteDurationPluginParameters::new(host);

        MaxNoteDurationPlugin {
            current_time_in_samples: 0,
            parameters: Arc::new(parameters),
            sample_rate: 44100.0,
            send_buffer: Default::default(),
            current_playing_notes: Default::default(),
            host,
            block_size: 0,
            events: vec![],
        }
    }

    fn set_sample_rate(&mut self, rate: f32) {
        self.sample_rate = rate
    }

    fn can_do(&self, can_do: CanDo) -> vst::api::Supported {
        use vst::api::Supported::*;
        use vst::plugin::CanDo::*;

        match can_do {
            SendEvents | SendMidiEvent | ReceiveEvents | ReceiveMidiEvent | Offline | ReceiveTimeInfo | MidiKeyBasedInstrumentControl | Bypass => Yes,
            MidiProgramNames => No,
            ReceiveSysExEvent => Yes,
            MidiSingleNoteTuningChange => No,
            Other(_) => {
                Maybe
            }
        }
    }

    fn process(&mut self, audio_buffer: &mut AudioBuffer<f32>) {
        if !self.current_playing_notes.is_empty() {
            let mut next_playing_notes = HashSet::new(); // can't iterate and modify, copy to a new one

            for playing_note in self.current_playing_notes.drain() {
                if playing_note.deadline < self.current_time_in_samples + self.block_size {
                    self.events.push(MidiEvent {
                        data: [0x80 + playing_note.channel, playing_note.pitch, 0],
                        delta_frames: (playing_note.deadline - self.current_time_in_samples) as i32,
                        live: false,
                        note_length: None,
                        note_offset: None,
                        detune: 0,
                        note_off_velocity: 0,
                    });
                } else {
                    next_playing_notes.insert(playing_note);
                }
            }
            self.current_playing_notes = next_playing_notes
        }

        self.send_buffer.send_events(&self.events, &mut self.host);
        self.events.clear();

        self.increase_time_in_samples(audio_buffer.samples());
    }

    fn process_events(&mut self, events: &Events) {
        let maximum_duration = self.seconds_to_samples(self.parameters.get_exponential_scale_parameter(Parameter::MaxDuration,
                                                                                                       10., 20.));

        for event in events.events() {
            let midi_event = if let Event::Midi(midi_event) = event {
                midi_event
            } else {
                continue;
            };

            match MidiMessageType::from(&midi_event.data) {
                MidiMessageType::NoteOffMessage(note) => {
                    self.current_playing_notes.remove(&PlayingNote {
                        channel: note.channel,
                        pitch: note.pitch,
                        deadline: 0,  // ignored for comparisons
                    });
                }
                MidiMessageType::NoteOnMessage(note) => {
                    self.current_playing_notes.insert(PlayingNote {
                        channel: note.channel,
                        pitch: note.pitch,
                        deadline: self.current_time_in_samples + maximum_duration,
                    });
                }
                MidiMessageType::Unsupported => {
                    continue;
                }
                _ => {}
            }
            self.events.push(midi_event);
        }
    }

    fn get_parameter_object(&mut self) -> Arc<dyn vst::plugin::PluginParameters> {
        Arc::clone(&self.parameters) as Arc<dyn vst::plugin::PluginParameters>
    }
}
