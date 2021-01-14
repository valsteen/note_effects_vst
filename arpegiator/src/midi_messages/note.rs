use std::cmp::Ordering;

#[derive(Clone, Copy, Debug)]
pub struct Note {
    pub id: usize,

    pub pressed_at: usize,
    pub released_at: usize,

    pub channel: u8,
    pub pitch: u8,
    pub velocity: u8,
    pub velocity_off: u8,

    pub pressure: u8,
    pub timbre: u8,
    pub pitchbend: i32,  // in millisemitones
}


#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub struct NoteIndex {
    pub channel: u8,
    pub pitch: u8
}


#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub struct CCIndex {
    pub channel: u8,
    pub index: u8
}


impl PartialOrd for Note {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        let cmp = self.pitch.cmp(&other.pitch);

        if cmp != Ordering::Equal {
            Some(cmp)
        } else {
            // unlikely to produce any interesting result
            Some(self.id.cmp(&other.id))
        }
    }
}


impl PartialOrd for NoteIndex {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        let cmp = self.pitch.cmp(&other.pitch);

        if cmp != Ordering::Equal {
            Some(cmp)
        } else {
            Some(self.channel.cmp(&other.channel))
        }
    }
}


impl PartialEq for Note {
    fn eq(&self, other: &Self) -> bool {
        self.id == other.id
    }
}

impl Ord for Note {
    fn cmp(&self, other: &Self) -> Ordering {
        self.partial_cmp(other).unwrap()
    }
}

impl Eq for Note {}
