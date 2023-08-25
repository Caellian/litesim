use std::any::{Any, TypeId};

pub trait Message: Any + 'static {}
impl<T> Message for T where T: Any {}

pub struct Event<M: Message> {
    type_info: TypeId,
    pub data: Box<M>,
}

impl<M: Message> Event<M> {
    pub fn new(data: M) -> Self {
        Event {
            type_info: TypeId::of::<M>(),
            data: Box::new(data),
        }
    }

    /// Erasing event type information makes it unsafe to drop the event until
    /// Message type has been restored to original value. Doing so will cause a
    /// memory leak.
    pub(crate) unsafe fn erase_message_type(self) -> ErasedEvent {
        let data: *const Box<M> = &self.data;

        ErasedEvent {
            type_id: self.type_info,
            type_name: std::any::type_name::<M>(),
            data: data as *const Box<ErasedMessage>,
        }
    }

    pub fn inner(&self) -> &M {
        &*self.data
    }

    pub fn into_inner(self) -> M {
        *self.data
    }
}

pub type Signal = Event<()>;

#[allow(non_snake_case)]
pub fn Signal() -> Signal {
    Signal::new(())
}

struct ErasedMessage;
pub struct ErasedEvent {
    pub(crate) type_id: TypeId,
    pub(crate) type_name: &'static str,
    data: *const Box<ErasedMessage>,
}

impl ErasedEvent {
    pub fn try_restore_type<M: Message>(self) -> Result<Event<M>, ErasedEvent> {
        let data = self.data as *const Box<M>;
        unsafe {
            if self.type_id == TypeId::of::<M>() {
                Ok(Event {
                    type_info: self.type_id,
                    data: std::ptr::read(data),
                })
            } else {
                Err(self)
            }
        }
    }
}

impl<M: Message> From<Event<M>> for ErasedEvent {
    fn from(value: Event<M>) -> Self {
        unsafe { value.erase_message_type() }
    }
}

impl<M: Message> From<M> for Event<M> {
    fn from(value: M) -> Self {
        Event::new(value)
    }
}
