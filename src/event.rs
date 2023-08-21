use std::{
    any::{Any, TypeId},
    pin::Pin,
};

use crate::{
    error::RoutingError,
    model::{ConnectorPath, Route},
    prelude::{EventSource, TimeTrigger},
    time::{SimDuration, SimTimeValue},
    util::{CowStr, EraseTyping, NotUnit, ToCowStr},
};

pub trait Message: Any {}
impl<T> Message for T where T: Any + NotUnit {}

pub struct Event<M: Message> {
    type_info: TypeId,
    pub data: Pin<Box<M>>,
}

impl<M: Message> Event<M> {
    pub fn new(data: M) -> Self {
        Event {
            type_info: TypeId::of::<M>(),
            data: Box::pin(data),
        }
    }

    /// Erasing event type information makes it unsafe to drop the event until
    /// Message type has been restored to original value. Doing so will cause a
    /// memory leak.
    pub(crate) unsafe fn erase_message_type(self) -> ErasedEvent {
        let data: *const Pin<Box<M>> = &self.data;

        ErasedEvent {
            type_id: self.type_info,
            type_name: std::any::type_name::<M>(),
            data: data as *const Pin<Box<ErasedMessage>>,
        }
    }
}

impl<M: Message> EraseTyping<ErasedEvent> for Event<M> {
    fn erase_typing(self) -> ErasedEvent {
        unsafe { self.erase_message_type() }
    }
}

pub(crate) struct ErasedMessage;
pub struct ErasedEvent {
    pub(crate) type_id: TypeId,
    pub(crate) type_name: &'static str,
    data: *const Pin<Box<ErasedMessage>>,
}

impl ErasedEvent {
    pub fn try_restore_type<M: Message>(self) -> Result<Event<M>, ErasedEvent> {
        let data: *const Pin<Box<M>> = self.data as *const Pin<Box<M>>;
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

impl<M: Message> From<M> for Event<M> {
    fn from(value: M) -> Self {
        Event::new(value)
    }
}

pub struct ProducedEvent<'a> {
    pub event: ErasedEvent,
    pub source_connector: CowStr<'a>,
    pub(crate) target: Option<ConnectorPath<'a>>,
    pub(crate) scheduling: TimeTrigger,
}

impl<'a> ProducedEvent<'a> {
    pub fn new(
        event: impl EraseTyping<ErasedEvent>,
        source_connector: CowStr<'a>,
        target: Option<ConnectorPath<'a>>,
        scheduling: TimeTrigger,
    ) -> Self {
        Self {
            event: event.erase_typing(),
            source_connector,
            target,
            scheduling,
        }
    }

    pub fn new_instant(
        event: impl EraseTyping<ErasedEvent>,
        source_connector: impl ToCowStr<'a>,
        target: Option<ConnectorPath<'a>>,
    ) -> Self {
        Self {
            event: event.erase_typing(),
            source_connector: source_connector.to_cow_str(),
            target,
            scheduling: TimeTrigger::Now,
        }
    }

    pub fn scheduled_at(self, time: SimTimeValue) -> Self {
        Self {
            event: self.event,
            source_connector: self.source_connector,
            target: self.target,
            scheduling: TimeTrigger::At { time },
        }
    }

    pub fn scheduled_in(self, delay: SimDuration) -> Self {
        Self {
            event: self.event,
            source_connector: self.source_connector,
            target: self.target,
            scheduling: TimeTrigger::In { delay },
        }
    }
}

pub struct RoutedEvent<'s> {
    pub event: ErasedEvent,
    pub route: Route<'s>,
}

impl<'s> RoutedEvent<'s> {
    pub fn new_external(event: impl EraseTyping<ErasedEvent>, target: ConnectorPath<'s>) -> Self {
        Self {
            event: event.erase_typing(),
            route: Route {
                from: EventSource::External,
                to: target,
            },
        }
    }

    pub fn new(event: impl EraseTyping<ErasedEvent>, route: Route<'s>) -> Self {
        Self {
            event: event.erase_typing(),
            route,
        }
    }

    pub fn from_produced(
        source_model: CowStr<'s>,
        event: ProducedEvent<'s>,
        default_target: Option<ConnectorPath<'s>>,
    ) -> Result<Self, RoutingError> {
        let route = Route::new_internal(
            ConnectorPath {
                model: source_model.clone(),
                connector: event.source_connector,
            },
            event
                .target
                .or(default_target)
                .ok_or(RoutingError::MissingEventTarget {
                    model: source_model.to_string(),
                })?,
        );
        Ok(Self {
            event: event.event,
            route,
        })
    }
}
