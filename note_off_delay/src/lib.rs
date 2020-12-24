mod parameters;
mod datastructures;

#[macro_use]
extern crate vst;

use std::sync::Arc;

use vst::api;
use vst::buffer::{AudioBuffer, SendEventBuffer};
use vst::plugin::{CanDo, Category, HostCallback, Info, Plugin};
use std::cell::RefCell;

use datastructures::CurrentPlayingNotes;
use parameters::NoteOffDelayPluginParameters;
use parameters::Parameter;
use util::absolute_time_midi_message::AbsoluteTimeMidiMessage;
use util::absolute_time_midi_message_vector::AbsoluteTimeMidiMessageVector;
use util::debug::DebugSocket;
use util::delayed_message_consumer::DelayedMessageConsumer;
use util::messages::format_event;
use util::midi_message_type::MidiMessageType;
use util::parameters::ParameterConversion;

plugin_main!(NoteOffDelayPlugin);

pub struct NoteOffDelayPlugin {
    current_playing_notes: CurrentPlayingNotes,
    current_time_in_samples: usize,
    message_queue: AbsoluteTimeMidiMessageVector,
    parameters: Arc<NoteOffDelayPluginParameters>,
    sample_rate: f32,
    send_buffer: RefCell<SendEventBuffer>,
}

impl Default for NoteOffDelayPlugin {
    fn default() -> Self {
        NoteOffDelayPlugin {
            send_buffer: Default::default(),
            parameters: Arc::new(Default::default()),
            sample_rate: 44100.0,
            current_time_in_samples: 0,
            message_queue: Default::default(),
            current_playing_notes: Default::default(),
        }
    }
}

impl NoteOffDelayPlugin {
    fn send_events(&mut self, samples: usize) {
        if let Ok(mut host_callback_lock) = self.parameters.host_mutex.lock() {
            let message_consumer: DelayedMessageConsumer = DelayedMessageConsumer {
                samples_in_buffer: samples,
                messages: &mut self.message_queue,
                current_time_in_samples: self.current_time_in_samples,
                drop_late_events: true
            };

            let mut messages: Vec<AbsoluteTimeMidiMessage> = message_consumer.collect();

            // TODO decouple limiting and updating playing notes. first we generate the note off necessary to cut notes that
            // exceeds the limit. this will generate duplicate note off messages with the same ID
            // then right when doing send_events, only then we update current playing notes, skipping
            // note off if the corresponding ID is not found in current playing notes. this sorts out the
            // duplicate note off issue, two same ID can live there as long as we know we skip them right at sending.

            let notes_off = self
                .current_playing_notes
                .update(&messages, self.parameters.get_max_notes());

            for note_off in notes_off {
                messages.push(note_off);
            }

            self.send_buffer.borrow_mut()
                .send_events(messages.iter().map(|e| e.new_midi_event(self.current_time_in_samples) ), &mut host_callback_lock.host);
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
            DebugSocket::send(
                &*(format_event(&e)
                    + &*format!(" current time={}", self.current_time_in_samples)),
            );
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
            parameters: 2,
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
        DebugSocket::send(
            build_info::format!("{{{} v{} built with {} at {}}}", $.crate_info.name, $.crate_info.version, $.compiler, $.timestamp),
        );
        NoteOffDelayPlugin {
            current_playing_notes: CurrentPlayingNotes::default(),
            current_time_in_samples: 0,
            message_queue: Default::default(),
            parameters: Arc::new(parameters),
            sample_rate: 44100.0,
            send_buffer: Default::default(),
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
            Other(_) => {
                // Bitwig will mark it as "MPE" by default if 'Yes', but somehow either there is a
                // bug here, or bitwig ends up being confused about midi events coming out of VSTs,
                // and some notes end up still running. As it's not really useful in this context,
                // let the feature off.
                // if s == "MPE" {
                //     Yes
                // } else {
                //     DebugSocket::send(&*s);
                //     No
                // }
                Maybe
            }
        }
    }

    fn process(&mut self, audio_buffer: &mut AudioBuffer<f32>) {
        self.send_events(audio_buffer.samples());
        self.increase_time_in_samples(audio_buffer.samples());
    }

    fn process_events(&mut self, events: &api::Events) {
        self.debug_events_in(events);

        let note_off_delay = match self
            .parameters
            .get_exponential_scale_parameter(Parameter::Delay, 10., 20.)
        {
            Some(value) => self.seconds_to_samples(value),
            _ => 0,
        };

        let mut notes_off = AbsoluteTimeMidiMessageVector::default();

        for event in events.events() {
            // TODO: minimum time, maximum time ( with delay )

            // TODO we can't just create from midi message anymore, first match on type, then create AbsoluteTimeMidiMessage with
            // a specific ID when it's a note off


            if let Some(mut absolute_time_midi_message) = AbsoluteTimeMidiMessage::from_event(&event, self.current_time_in_samples) {
                let midi_message = MidiMessageType::from(&absolute_time_midi_message);
                match midi_message {
                    MidiMessageType::NoteOffMessage(_) => {
                        // TODO find the corresponding note on ID in current playing notes and assign it
                        // maybe using a constructor that uses the original note on
                        notes_off.insert_message(absolute_time_midi_message)
                    }
                    MidiMessageType::Unsupported => {}
                    MidiMessageType::NoteOnMessage(_) => {
                        // TODO find the ID of the note on this one replaces, get the corresponding note off by id,
                        // replace it at the absolute time location of this new note on, this note on then receives time+1

                        // find any pending note off that was planned after this note on, and place
                        // it just before. This is in order to still trigger the note off message.
                        if let Some(delayed_note_off_position) = self.message_queue.iter().position(
                            |delayed_note_off| midi_message.is_same_note(&MidiMessageType::from(delayed_note_off))
                        ) {
                            let mut note_off = self.message_queue.remove(delayed_note_off_position);
                            note_off.play_time_in_samples = absolute_time_midi_message.play_time_in_samples;
                            self.message_queue.insert_message(note_off);
                            DebugSocket::send(&*format!(
                                "delayed note off moved before replacing note on {}",
                                note_off
                            ));

                            // make sure the note on is after the note off. The daw may randomly immediately stop the note otherwise
                            // even if the note off is placed before the note on.
                            absolute_time_midi_message.play_time_in_samples += 1;
                        }

                        self.message_queue.insert_message(absolute_time_midi_message);
                    }
                    _ => {
                        self.message_queue.insert_message(absolute_time_midi_message)
                    }
                }
            }
        }

        self.message_queue.merge_notes_off(&mut notes_off, note_off_delay);
    }

    fn get_parameter_object(&mut self) -> Arc<dyn vst::plugin::PluginParameters> {
        Arc::clone(&self.parameters) as Arc<dyn vst::plugin::PluginParameters>
    }
}
