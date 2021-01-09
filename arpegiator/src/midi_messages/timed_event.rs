pub trait TimedEvent {
    fn timestamp(&self) -> usize ;
    fn id(&self) -> usize ;
}
