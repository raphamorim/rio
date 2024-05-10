pub enum WindowEvent {
    Focus(bool),
}

pub enum QueuedEvent {
    Window(u16, WindowEvent),
}
