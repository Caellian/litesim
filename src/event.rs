use crate::{
    error::SimulationError,
    model::{ModelConnection, Route},
    prelude::EventSource,
    time::{SimDuration, SimTimeValue, SimulationTime},
    util::CowStr,
};

pub trait Event: 'static {
    fn type_id() -> &'static str;
}

#[derive(Clone)]
pub enum EventScheduling {
    Now,
    At { time: SimTimeValue },
    In { delay: SimDuration },
}

impl EventScheduling {
    pub fn final_time(&self, current: SimulationTime) -> SimulationTime {
        match self {
            EventScheduling::Now => current,
            EventScheduling::At { time } => SimulationTime::new(time.clone()),
            EventScheduling::In { delay } => current + delay.clone(),
        }
    }
}

pub struct ProducedEvent<'a, E: Event> {
    pub event: E,
    pub source_connector: CowStr<'a>,
    pub(crate) target: Option<ModelConnection<'a>>,
    pub(crate) scheduling: EventScheduling,
}

impl<'a, E: Event> ProducedEvent<'a, E> {
    pub fn new(
        event: E,
        source_connector: CowStr<'a>,
        target: Option<ModelConnection<'a>>,
        scheduling: EventScheduling,
    ) -> Self {
        Self {
            event,
            source_connector,
            target,
            scheduling,
        }
    }

    pub fn new_instant(
        event: E,
        source_connector: CowStr<'a>,
        target: Option<ModelConnection<'a>>,
    ) -> ProducedEvent<'a, E> {
        ProducedEvent {
            event,
            source_connector,
            target,
            scheduling: EventScheduling::Now,
        }
    }

    pub fn scheduled_at(self, time: SimTimeValue) -> Self {
        ProducedEvent {
            event: self.event,
            source_connector: self.source_connector,
            target: self.target,
            scheduling: EventScheduling::At { time },
        }
    }

    pub fn scheduled_in(self, delay: SimDuration) -> Self {
        ProducedEvent {
            event: self.event,
            source_connector: self.source_connector,
            target: self.target,
            scheduling: EventScheduling::In { delay },
        }
    }
}

pub struct RoutedEvent<'s, E: Event> {
    pub event: E,
    pub route: Route<'s>,
}

impl<'s, E: Event> RoutedEvent<'s, E> {
    pub fn new_external(event: E, target: ModelConnection<'s>) -> Self {
        Self {
            event,
            route: Route {
                from: EventSource::External,
                to: target,
            },
        }
    }

    pub fn new(event: E, route: Route<'s>) -> Self {
        Self { event, route }
    }

    pub fn from_produced(
        source_model: CowStr<'s>,
        event: ProducedEvent<'s, E>,
        default_target: Option<ModelConnection<'s>>,
    ) -> Result<Self, SimulationError> {
        let route = Route::new_internal(
            ModelConnection {
                model: source_model.clone(),
                connector: event.source_connector,
            },
            event
                .target
                .or(default_target)
                .ok_or(SimulationError::MissingEventTarget {
                    model: source_model.to_string(),
                })?,
        );
        Ok(Self {
            event: event.event,
            route,
        })
    }
}
