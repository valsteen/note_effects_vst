mod parameters;

#[macro_use]
extern crate vst;

use std::sync::Arc;

use vst::api;
use vst::buffer::{AudioBuffer, SendEventBuffer};
use vst::event::{Event, MidiEvent};
use vst::plugin::{CanDo, Category, HostCallback, Info, Plugin, PluginParameters};

use parameters::NoteOffDelayPluginParameters;
use util::constants::{NOTE_OFF, NOTE_ON};
use vst::event::Event::Midi;

plugin_main!(NoteOffDelayPlugin);

#[derive(Clone)]
struct DelayedMidiEvent {
    midi_event: MidiEvent,
    play_time_in_samples: usize,
}

fn same_note(this: &MidiEvent, other: &MidiEvent) -> bool {
    // same channel, same pitch. we don't test if it's a note on / off
    (this.data[0] & 0x0F) == (other.data[0] & 0x0F) && this.data[1] == other.data[1]
}

pub struct NoteOffDelayPlugin {
    events: Vec<MidiEvent>,
    send_buffer: SendEventBuffer,
    parameters: Arc<NoteOffDelayPluginParameters>,
    sample_rate: f32,
    current_time_in_samples: usize,
    delayed_events: Vec<DelayedMidiEvent>,
}

impl Default for NoteOffDelayPlugin {
    fn default() -> Self {
        NoteOffDelayPlugin {
            events: Default::default(),
            send_buffer: Default::default(),
            parameters: Arc::new(Default::default()),
            sample_rate: 44100.0,
            current_time_in_samples: 0,
            delayed_events: Vec::new(),
        }
    }
}

impl NoteOffDelayPlugin {
    fn send_midi(&mut self) {
        if let Ok(mut host_callback_lock) = self.parameters.host_mutex.lock() {
            self.send_buffer
                .send_events(&self.events, &mut host_callback_lock.host);
        }
        self.events.clear();
    }

    fn trigger_delayed_notes(&mut self, samples: usize) {
        loop {
            if self.delayed_events.is_empty() { break }

            if self.delayed_events[0].play_time_in_samples < self.current_time_in_samples {
                self.delayed_events.remove(0);
                continue
            }

            if self.delayed_events[0].play_time_in_samples >= self.current_time_in_samples + samples {
                break
            }

            let mut delayed_midi_event = self.delayed_events.remove(0);
            delayed_midi_event.midi_event.delta_frames = (delayed_midi_event.play_time_in_samples - self.current_time_in_samples) as i32;
            self.events.push(delayed_midi_event.midi_event);
        }
    }

    #[allow(dead_code)]
    fn seconds_per_sample(&self) -> f32 {
        1.0 / self.sample_rate
    }

    fn seconds_to_samples(&self, seconds: f32) -> usize {
        (seconds * self.sample_rate) as usize
    }

    fn debug_events_in(&mut self, events: &api::Events) {
        for e in events.events() {
            match e {
                Midi(e) => {
                    self.parameters.debug(
                        format!(
                            "[{:#04X} {:#04X} {:#04X}] delta_frames={}",
                            e.data[0], e.data[1], e.data[2], e.delta_frames
                        )
                        .as_str(),
                    );
                }
                Event::Deprecated(e) => {
                    let out = e
                        ._reserved
                        .iter()
                        .fold(String::new(), |acc, x| acc + &*x.to_string());
                    self.parameters.debug(&*format!("? : {}", out));
                }
                Event::SysEx(e) => {
                    let out = e
                        .payload
                        .iter()
                        .fold(String::new(), |acc, x| acc + &*x.to_string());
                    self.parameters.debug(&*format!("Sysex : {}", out));
                }
            }
        }
    }

    fn remove_duplicate_note(&mut self, midi_event: &MidiEvent) {
        // if a note off for the same pitch/channel already exists, remove it or it will interrupt the ongoing note. target instrument interrupts
        // a running note if another note on comes in anyway

        if let Some(position) = self.delayed_events.iter().position(
            |e| same_note(&e.midi_event, midi_event)
        ) {
            self.delayed_events.remove(position);
        }
    }

    fn increase_time_in_samples(&mut self, samples: usize) {
        let new_time_in_samples = self.current_time_in_samples + samples;

        // tick every second in the debug socket
        // let old_time_in_seconds = self.seconds_per_sample() * self.current_time_in_samples as f32;
        // let new_time_in_seconds = self.seconds_per_sample() * new_time_in_samples as f32;
        //
        // if old_time_in_seconds.trunc() != new_time_in_seconds.trunc() {
        //     self.parameters.debug(&*format!("{}s", new_time_in_seconds));
        // }
        self.current_time_in_samples = new_time_in_samples;
    }
}

impl Plugin for NoteOffDelayPlugin {
    fn get_info(&self) -> Info {
        Info {
            name: "Note Off Delay".to_string(),
            vendor: "DJ Crontab".to_string(),
            unique_id: 234213173,
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

    fn new(host: HostCallback) -> Self {
        let parameters = NoteOffDelayPluginParameters::new(host);
        parameters.debug(build_info::format!("{{{} v{} built with {} at {}}}", $.crate_info.name, $.crate_info.version, $.compiler, $.timestamp));
        NoteOffDelayPlugin {
            events: Default::default(),
            send_buffer: Default::default(),
            parameters: Arc::new(parameters),
            sample_rate: 44100.0,
            current_time_in_samples: 0,
            delayed_events: Vec::new(),
        }
    }

    fn set_sample_rate(&mut self, rate: f32) {
        self.sample_rate = rate
    }

    fn can_do(&self, can_do: CanDo) -> vst::api::Supported {
        use vst::api::Supported::*;
        use vst::plugin::CanDo::*;

        match can_do {
            SendEvents
            | SendMidiEvent
            | ReceiveEvents
            | ReceiveMidiEvent
            | Offline
            | ReceiveTimeInfo
            | MidiKeyBasedInstrumentControl
            | Bypass => Yes,
            MidiProgramNames => No,
            ReceiveSysExEvent => Yes,
            MidiSingleNoteTuningChange => No,
            Other(s) => {
                if s == "MPE" {
                    Yes
                } else {
                    self.parameters.debug(&*s);
                    No
                }
            }
        }
    }

    fn process(&mut self, audio_buffer: &mut AudioBuffer<f32>) {
        self.trigger_delayed_notes(audio_buffer.samples());
        self.send_midi();
        self.increase_time_in_samples(audio_buffer.samples());
    }

    fn process_events(&mut self, events: &api::Events) {
        self.debug_events_in(events);
        for e in events.events() {
            match e {
                Midi(e) => {
                    if e.data[0] >= NOTE_OFF && e.data[0] < NOTE_ON + 16 {
                        self.remove_duplicate_note(&e);
                    }

                    if e.data[0] >= NOTE_OFF && e.data[0] < NOTE_OFF + 0x10 {
                        let delayed_event = DelayedMidiEvent {
                            midi_event: e,
                            play_time_in_samples: {
                                self.current_time_in_samples
                                    + self.seconds_to_samples(
                                    self.parameters
                                        .get_parameter(NoteOffDelayPluginParameters::DELAY),
                                )
                                    + e.delta_frames as usize
                            },
                        };

                        if let Some(insert_point) = self.delayed_events.iter().position(
                            |e| e.play_time_in_samples > delayed_event.play_time_in_samples
                        ) {
                            self.delayed_events.insert( insert_point, delayed_event);
                        } else {
                            self.delayed_events.push(delayed_event);
                        }
                    } else {
                        self.events.push(e);
                    }
                }
                _ => {}
            }
        }
    }

    fn get_parameter_object(&mut self) -> Arc<dyn vst::plugin::PluginParameters> {
        Arc::clone(&self.parameters) as Arc<dyn vst::plugin::PluginParameters>
    }
}
