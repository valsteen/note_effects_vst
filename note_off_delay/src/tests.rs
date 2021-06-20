#[cfg(test)]
mod test {
    use crate::parameters::Parameter;
    use crate::NoteOffDelayPlugin;
    use core::mem;
    use std::convert::TryFrom;
    use util::parameters::get_reverse_exponential_scale_value;
    use vst::api::{Event, EventType, Events, MidiEvent};
    use vst::plugin::{Plugin, PluginParameters};

    fn make_event(message: [u8; 3], delta_frames: i32) -> Event {
        let midi_event: MidiEvent = MidiEvent {
            event_type: EventType::Midi,
            byte_size: mem::size_of::<MidiEvent>() as i32,
            delta_frames,
            flags: 0,
            note_length: 0,
            note_offset: 0,
            midi_data: [message[0], message[1], message[2]],
            _midi_reserved: 0,
            detune: 0,
            note_off_velocity: 0,
            _reserved1: 0,
            _reserved2: 0,
        };
        let mut event: Event = unsafe { std::mem::transmute(midi_event) };
        event.event_type = EventType::Midi;

        event
    }

    #[test]
    fn test_process_scheduled_events() {
        let original = vec![([0x90, 0x60, 0x60], 10), ([0x80, 0x60, 0x60], 20)];
        let expected = vec![([0x90, 0x60, 0x60], 10), ([0x80, 0x60, 0x60], 120)];

        assert_process_scheduled_events(original, expected, 100)
    }

    #[test]
    fn test_process_scheduled_events_feature_off() {
        let original = vec![([0x90, 0x60, 0x60], 10), ([0x80, 0x60, 0x60], 20)];
        let expected = vec![([0x90, 0x60, 0x60], 10), ([0x80, 0x60, 0x60], 20)];

        assert_process_scheduled_events(original, expected, 0)
    }

    #[test]
    fn test_process_scheduled_events_next() {
        let original = vec![([0x90, 0x60, 0x60], 10), ([0x80, 0x60, 0x60], 20)];
        let expected = vec![([0x90, 0x60, 0x60], 10)];

        assert_process_scheduled_events(original, expected, 1004)
    }

    fn compare(events: Vec<vst::event::MidiEvent>, expected: Vec<([u8; 3], i32)>) {
        assert_eq!(events.len(), expected.len());
        for ((test_data, test_delta_frame), (expected_data, expected_delta_frame)) in events
            .iter()
            .map(|event| (event.data, event.delta_frames))
            .zip(expected.iter())
        {
            assert_eq!(test_data, *expected_data);
            assert_eq!(test_delta_frame, *expected_delta_frame);
        }
    }

    fn assert_process_scheduled_events(
        original: Vec<([u8; 3], i32)>,
        expected: Vec<([u8; 3], i32)>,
        delay_parameter_value_samples: usize,
    ) {
        let mut plugin = NoteOffDelayPlugin::default();

        let delay_parameter_value_seconds = delay_parameter_value_samples as f32 / 44100.;
        let delay_parameter_value = get_reverse_exponential_scale_value(delay_parameter_value_seconds, 10., 20.);

        plugin
            .parameters
            .set_parameter(i32::from(Parameter::DelayOffset), delay_parameter_value);

        let mut original_as_events: Vec<Event> = original
            .iter()
            .map(|(raw_message, delta_frame)| make_event(*raw_message, *delta_frame))
            .collect();

        let events: Vec<*mut Event> = original_as_events.iter_mut().map(|x| x as *mut Event).collect();
        let events = Events {
            num_events: events.len() as i32,
            _reserved: 0,
            events: <[*mut Event; 2]>::try_from(events).unwrap(),
        };

        plugin.process_events(&events);
        let (_, events) = plugin.process_scheduled_events(1024);

        compare(events, expected);
    }
}
