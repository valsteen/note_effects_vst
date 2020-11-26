mod parameters;

#[macro_use]
extern crate vst;

use std::sync::Arc;

use vst::api;
use vst::buffer::{AudioBuffer, SendEventBuffer};
use vst::event::{Event, MidiEvent};
use vst::plugin::{CanDo, Category, HostCallback, Info, Plugin, PluginParameters};

use parameters::NoteOffDelayPluginParameters;
use sorted_list::SortedList;
use util::constants::{NOTE_OFF, NOTE_ON};
use vst::event::Event::Midi;

plugin_main!(NoteOffDelayPlugin);

#[derive(Clone)]
struct SortableMidiEvent {
    midi_event: MidiEvent,
    play_time_in_samples: usize,
}

impl PartialEq for SortableMidiEvent {
    fn eq(&self, other: &Self) -> bool {
        self.play_time_in_samples == other.play_time_in_samples
            && self.midi_event.delta_frames == other.midi_event.delta_frames
            && self.midi_event.data == other.midi_event.data
    }
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
    delayed_events: SortedList<usize, SortableMidiEvent>,
}

impl Default for NoteOffDelayPlugin {
    fn default() -> Self {
        NoteOffDelayPlugin {
            events: Default::default(),
            send_buffer: Default::default(),
            parameters: Arc::new(Default::default()),
            sample_rate: 44100.0,
            current_time_in_samples: 0,
            delayed_events: SortedList::new(),
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
        let iterable = &mut self.delayed_events.values();
        let mut sortable_midi_event: &SortableMidiEvent;

        // empty check
        if let Some(next_sortable_midi_event) = iterable.next() {
            sortable_midi_event = next_sortable_midi_event
        } else {
            // empty already
            return;
        }

        // remove elements in the past
        loop {
            if sortable_midi_event.play_time_in_samples >= self.current_time_in_samples {
                // all in the future, but we removed elements in the past
                break;
            } else {
                if let Some(next_sortable_midi_event) = iterable.next() {
                    sortable_midi_event = next_sortable_midi_event
                } else {
                    // everything happened before, clear the list, return
                    self.delayed_events = SortedList::new();
                    return;
                }
            }
        }

        // play current elements
        loop {
            if sortable_midi_event.play_time_in_samples > self.current_time_in_samples + samples {
                break;
            }

            let mut midi_event = sortable_midi_event.midi_event.clone();
            midi_event.delta_frames =
                (sortable_midi_event.play_time_in_samples - self.current_time_in_samples) as i32;
            self.events.push(sortable_midi_event.midi_event.clone());

            if let Some(next_sortable_midi_event) = iterable.next() {
                sortable_midi_event = next_sortable_midi_event
            } else {
                break;
            }
        }

        let mut new_events = SortedList::new();
        new_events.insert(
            sortable_midi_event.play_time_in_samples,
            (*sortable_midi_event).clone(),
        );
        for sortable_midi_event in iterable {
            new_events.insert(
                sortable_midi_event.play_time_in_samples,
                (*sortable_midi_event).clone(),
            );
        }
        self.delayed_events = new_events;
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
        // TODO ending up with that complexity just because SortedList does not implement removing items. Should just use an unsorted vec.

        if self
            .delayed_events
            .values()
            .find(|v| same_note(&v.midi_event, &midi_event))
            .is_some()
        {
            self.parameters.debug(&*format!(
                "found already : [{:#04X} {:#04X} {:#04X}]",
                midi_event.data[0], midi_event.data[1], midi_event.data[2]
            ));

            let mut delayed_events: SortedList<usize, SortableMidiEvent> = SortedList::new();
            for (delta, event) in self
                .delayed_events
                .iter()
                .filter(|(_k, v)| !same_note(&v.midi_event, &midi_event))
            {
                delayed_events.insert(*delta, event.clone());
            }
            self.delayed_events = delayed_events;
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
            delayed_events: SortedList::new(),
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
                        let delayed_event = SortableMidiEvent {
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
                        self.delayed_events
                            .insert(delayed_event.play_time_in_samples, delayed_event);
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
