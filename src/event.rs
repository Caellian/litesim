use crate::{
    error::SimulationError,
    model::{ConnectorPath, Route},
    prelude::{EventSource, TimeTrigger},
    time::{SimDuration, SimTimeValue, SimulationTime},
    util::CowStr,
};

pub trait Event: 'static {
    fn type_id() -> &'static str;
}

pub struct ProducedEvent<'a, E: Event> {
    pub event: E,
    pub source_connector: CowStr<'a>,
    pub(crate) target: Option<ConnectorPath<'a>>,
    pub(crate) scheduling: TimeTrigger,
}

impl<'a, E: Event> ProducedEvent<'a, E> {
    pub fn new(
        event: E,
        source_connector: CowStr<'a>,
        target: Option<ConnectorPath<'a>>,
        scheduling: TimeTrigger,
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
        target: Option<ConnectorPath<'a>>,
    ) -> ProducedEvent<'a, E> {
        ProducedEvent {
            event,
            source_connector,
            target,
            scheduling: TimeTrigger::Now,
        }
    }

    pub fn scheduled_at(self, time: SimTimeValue) -> Self {
        ProducedEvent {
            event: self.event,
            source_connector: self.source_connector,
            target: self.target,
            scheduling: TimeTrigger::At { time },
        }
    }

    pub fn scheduled_in(self, delay: SimDuration) -> Self {
        ProducedEvent {
            event: self.event,
            source_connector: self.source_connector,
            target: self.target,
            scheduling: TimeTrigger::In { delay },
        }
    }
}

pub struct RoutedEvent<'s, E: Event> {
    pub event: E,
    pub route: Route<'s>,
}

impl<'s, E: Event> RoutedEvent<'s, E> {
    pub fn new_external(event: E, target: ConnectorPath<'s>) -> Self {
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
        default_target: Option<ConnectorPath<'s>>,
    ) -> Result<Self, SimulationError> {
        let route = Route::new_internal(
            ConnectorPath {
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
