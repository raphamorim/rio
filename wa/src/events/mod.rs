use crate::native::apple::frameworks::ObjcId;

pub enum WindowEvent {
    Focus(bool),
    Initialize(ObjcId),
}

pub enum QueuedEvent {
    Window(u16, WindowEvent),
}
