use std::fmt::Debug;

use crate::{
    event::{Event, ProducedEvent},
    prelude::SimulationTime,
    simulation::SimulationCtx,
    util::CowStr,
};

#[derive(Clone, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct ConnectorPath<'s> {
    pub model: CowStr<'s>,
    pub connector: CowStr<'s>,
}

impl<'s> ConnectorPath<'s> {
    pub fn new(model: impl AsRef<str>, connector: impl AsRef<str>) -> Self {
        Self {
            model: CowStr::Owned(model.as_ref().to_string()),
            connector: CowStr::Owned(connector.as_ref().to_string()),
        }
    }
}

impl Debug for ConnectorPath<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}::{}", self.model, self.connector)
    }
}

#[macro_export]
macro_rules! connection {
    ($model:tt :: $connector:tt) => {
        ::litesim::model::ConnectorPath {
            model: ::litesim::util::CowStr::Borrowed(stringify!($model)),
            connector: ::litesim::util::CowStr::Borrowed(stringify!($connector)),
        }
    };
}

#[macro_export]
macro_rules! declare_connectors {
    {input: [$($input: literal),*], output: [$($output: literal),*]} => {
        fn input_connectors<'s>(&'s self) -> Vec<::litesim::util::CowStr<'s>> {
            vec![$(::litesim::util::CowStr::Borrowed($input),
            )*]
        }
        fn output_connectors<'s>(&'s self) -> Vec<::litesim::util::CowStr<'s>> {
            vec![$(::litesim::util::CowStr::Borrowed($output),
            )*]
        }
    };
    {output: [$($output: literal),*], input: [$($input: literal),*]} => {
        declare_connectors!{
            input: [$($input),*],
            output: [$($output),*]
        }
    };
    {input: [$($input: literal),*]} => {
        declare_connectors!{
            input: [$($input),*],
            output: []
        }
    };
    {output: [$($output: literal),*]} => {
        declare_connectors!{
            input: [],
            output: [$($output),*]
        }
    };
    {} => {
        declare_connectors!{
            input: [],
            output: []
        }
    };
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EventSource<'s> {
    External,
    Model(ConnectorPath<'s>),
}

#[derive(Clone, PartialEq, Eq)]
pub struct Route<'s> {
    pub from: EventSource<'s>,
    pub to: ConnectorPath<'s>,
}

#[macro_export]
macro_rules! route {
    ($model_a: tt :: $connector_a: tt -> $model_b: tt :: $connector_b: tt) => {
        ::litesim::model::Route::new_internal(
            connection!($model_a::$connector_a),
            connection!($model_b::$connector_b),
        )
    };
}

impl<'s> Route<'s> {
    pub fn new_internal(from: ConnectorPath<'s>, to: ConnectorPath<'s>) -> Self {
        Route {
            from: EventSource::Model(from),
            to,
        }
    }

    pub fn starts_in_model(&self, id: impl AsRef<str>) -> bool {
        match &self.from {
            EventSource::External => false,
            EventSource::Model(ConnectorPath {
                model: model_id, ..
            }) => model_id.as_ref() == id.as_ref(),
        }
    }

    pub fn ends_in_model(&self, id: impl AsRef<str>) -> bool {
        self.to.model.as_ref() == id.as_ref()
    }

    pub fn from_connection(&self) -> ConnectorPath<'s> {
        match &self.from {
            EventSource::External => panic!("expected route from to be an internal connection"),
            EventSource::Model(connection) => connection.clone(),
        }
    }

    pub fn to_connection(&self) -> ConnectorPath<'s> {
        self.to.clone()
    }
}

impl<'s> From<(ConnectorPath<'s>, ConnectorPath<'s>)> for Route<'s> {
    fn from(value: (ConnectorPath<'s>, ConnectorPath<'s>)) -> Self {
        Route {
            from: EventSource::Model(value.0),
            to: value.1,
        }
    }
}

impl<'s> From<(&ConnectorPath<'s>, &ConnectorPath<'s>)> for Route<'s> {
    fn from(value: (&ConnectorPath<'s>, &ConnectorPath<'s>)) -> Self {
        Route {
            from: EventSource::Model(value.0.clone()),
            to: value.1.clone(),
        }
    }
}

pub enum InputEffect<'a, E: Event> {
    /// Signals that the model consumed input event
    Consume,
    /// Signals that the model dropped input event
    Drop(E),
    /// Signals that an internal change should occur at provided time
    ScheduleInternal(SimulationTime),
    /// Signals that an event was produced as a consequence of receiving input event
    Produce(ProducedEvent<'a, E>),
}

pub enum ChangeEffect<'a, E: Event> {
    /// Model state hasn't changed and requires no neighbor updates
    None,
    /// Signals that an internal change should occur at provided time
    ScheduleInternal(SimulationTime),
    /// Signals that an event was created
    Produce(ProducedEvent<'a, E>),
}

#[allow(unused_variables)]
pub trait Model<E: Event> {
    /// Lists all model input connectors
    fn input_connectors<'s>(&'s self) -> Vec<CowStr<'s>>;
    /// Lists all model output connectors
    fn output_connectors<'s>(&'s self) -> Vec<CowStr<'s>>;

    /// Returns true if this model has given input connector
    fn has_input_connector<'s>(&'s self, id: &str) -> bool {
        self.input_connectors()
            .into_iter()
            .find(|c| c.as_ref() == id)
            .is_some()
    }
    fn has_output_connector<'s>(&'s self, id: &str) -> bool {
        self.output_connectors()
            .into_iter()
            .find(|c| c.as_ref() == id)
            .is_some()
    }

    /// Handler for external event inputs.
    #[must_use]
    fn handle_input<'s>(
        &mut self,
        event: E,
        connector: CowStr<'s>,
        ctx: SimulationCtx,
        source: EventSource<'s>,
    ) -> InputEffect<'s, E> {
        InputEffect::Drop(event)
    }
    /// Handler for internal model changes when the elapsed time is supposed to affect
    /// the state of the model.
    #[must_use]
    fn handle_change<'s>(&mut self, ctx: SimulationCtx<'s>) -> ChangeEffect<'s, E> {
        ChangeEffect::None
    }
    /// Returns time of next expected internal change.
    ///
    /// This method is allowed to modify the model to store the returned value.
    #[must_use]
    fn next_change_time<'s>(&mut self, ctx: SimulationCtx<'s>) -> Option<SimulationTime> {
        None
    }
}
