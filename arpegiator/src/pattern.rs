use crate::note::Note;
use crate::timed_event::TimedEvent;

pub const C3: u8 = 60 ;

#[derive(Copy, Clone, Debug)]
pub struct Pattern {
    pub id: usize,

    pub channel: u8,
    pub index: u8,
    pub octave: i8,

    pub pressed_at: usize,
    pub released_at: usize,

    pub velocity: u8,
    pub velocity_off: u8,

    pub pressure: u8,
    pub timbre: u8,
    pub pitchbend: i32,  // in millisemitones
}

impl Pattern {
    pub fn transpose(&self, pitch: u8) -> Option<u8> {
        let pitch = pitch as i16 + self.octave as i16 * 12;
        if !(0..=127).contains(&pitch) {
            // can't play. to fix if we want to change the pitch of a pattern that started
            None
        } else {
            Some(pitch as u8)
        }
    }
}


impl TimedEvent for Pattern {
    fn timestamp(&self) -> usize {
        if self.released_at > 0 {
            self.released_at
        } else {
            self.pressed_at
        }
    }

    fn id(&self) -> usize {
        self.id
    }
}


impl From<Note> for Pattern {
    fn from(note: Note) -> Self {
        let index = note.pitch % 12;
        let octave = (((note.pitch - index) as i16 - C3 as i16) / 12) as i8;

        Pattern {
            channel: note.channel,
            id: note.id,
            index,
            velocity_off: 0,
            pressure: 0,
            timbre: 64,
            octave,
            pressed_at: note.pressed_at,
            released_at: 0,
            velocity: note.velocity,
            pitchbend: 0
        }
    }
}
