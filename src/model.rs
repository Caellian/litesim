use std::{any::TypeId, fmt::Debug};

use crate::{
    error::{RoutingError, SimulationError},
    event::{Event, Message},
    prelude::{ConnectorCtx, ErasedEvent},
    simulation::ModelCtx,
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
            model: std::borrow::Cow::Borrowed(stringify!($model)),
            connector: std::borrow::Cow::Borrowed(stringify!($connector)),
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

pub trait InputHandler<'s>:
    Fn(&mut Self::Model, Event<Self::In>, ModelCtx<'s>) -> Result<(), SimulationError>
{
    type Model: Model<'s> + 'static;
    type In: Message;
}
impl<'s, S: Model<'s> + 'static, M: Message> InputHandler<'s>
    for &dyn Fn(&mut S, Event<M>, ModelCtx<'s>) -> Result<(), SimulationError>
{
    type Model = S;
    type In = M;
}

pub trait ErasedInputHandler<'h, 's: 'h>: 'h {
    fn apply_event(&self, event: ErasedEvent, ctx: ConnectorCtx<'s>)
        -> Result<(), SimulationError>;
    fn model_type_id(&self) -> TypeId;
    fn event_type_id(&self) -> TypeId;
}

impl<'h, 's: 'h, C: InputHandler<'s> + 'h> ErasedInputHandler<'h, 's> for C {
    fn apply_event(
        &self,
        event: ErasedEvent,
        ctx: ConnectorCtx<'s>,
    ) -> Result<(), SimulationError> {
        let casted = event
            .try_restore_type()
            .map_err(|got| RoutingError::InvalidEventType {
                event_type: got.type_name,
                expected: std::any::type_name::<C::In>(),
            })?;

        let ConnectorCtx {
            model_ctx,
            mut on_model,
        } = ctx;

        let model = unsafe {
            on_model
                .cast_mut::<C::Model>()
                .ok_or_else(|| RoutingError::InvalidModelType {
                    expected: std::any::type_name::<C::Model>(),
                })?
        };
        self(model, casted, model_ctx)?;
        Ok(())
    }

    fn model_type_id(&self) -> TypeId {
        TypeId::of::<C::Model>()
    }

    fn event_type_id(&self) -> TypeId {
        TypeId::of::<C::In>()
    }
}

pub struct OutputConnectorInfo(&'static str, TypeId);

impl OutputConnectorInfo {
    pub const fn new<T: 'static>(id: &'static str) -> Self {
        OutputConnectorInfo(id, TypeId::of::<T>())
    }
}

pub trait Model<'s> {
    /// Lists all model input connectors
    ///
    /// Returned value must stay the same for each model instance for the
    /// duration of the simulation.
    fn input_connectors(&self) -> Vec<&'static str>;
    /// Lists all model output connectors
    ///
    /// Returned value must stay the same for each model instance for the
    /// duration of the simulation.
    fn output_connectors(&self) -> Vec<OutputConnectorInfo>;

    /// Returns input handlers for all input connectors.
    ///
    /// Index argument matches indices of [Self::input_connectors].
    fn get_input_handler<'h>(&self, index: usize) -> Option<Box<dyn ErasedInputHandler<'h, 's>>>
    where
        's: 'h;

    /// Called during initalization.
    ///
    /// This method allows models like generators to schedule their inital changes.
    #[allow(unused_variables)]
    fn init(&mut self, ctx: ModelCtx<'s>) -> Result<(), SimulationError> {
        Ok(())
    }

    /// Handler for internal model changes when the elapsed time is supposed to affect
    /// the state of the model.
    #[allow(unused_variables)]
    fn handle_update(&mut self, ctx: ModelCtx<'s>) -> Result<(), SimulationError> {
        Ok(())
    }

    fn type_id(&self) -> TypeId;
}

pub trait ModelImpl<'s>: Model<'s> {
    fn get_input_handler_by_name<'h>(
        &self,
        name: impl AsRef<str>,
    ) -> Option<Box<dyn ErasedInputHandler<'h, 's>>>
    where
        's: 'h,
    {
        let i = self
            .input_connectors()
            .iter()
            .enumerate()
            .find(|(_, it)| **it == name.as_ref())
            .map(|it| it.0)?;

        self.get_input_handler(i)
    }

    fn input_type_id(&self, name: impl AsRef<str>) -> Option<TypeId> {
        let handler = self.get_input_handler_by_name(name)?;
        Some(handler.event_type_id())
    }

    fn output_type_id(&self, name: impl AsRef<str>) -> Option<TypeId> {
        self.output_connectors()
            .iter()
            .find(|it| it.0 == name.as_ref())
            .map(|it| it.1.clone())
    }
}

impl<'s, M: Model<'s> + ?Sized> ModelImpl<'s> for M {}

#[macro_export]
macro_rules! push_event {
    ($ctx: ident, $id: literal, $msg: expr) => {
        $ctx.push_event_with_source(
            ::litesim::event::Event::new($msg),
            None,
            std::borrow::Cow::Borrowed($id),
        )
    };
    ($ctx: ident, $id: literal, $msg: expr, $time: expr) => {
        $ctx.push_event_with_time_and_source(
            ::litesim::event::Event::new($msg),
            $time,
            None,
            std::borrow::Cow::Borrowed($id),
        )
    };
    ($ctx: ident, $id: literal, $msg: expr, Now, $target: literal) => {
        $ctx.push_event_with_source(
            ::litesim::event::Event::new($msg),
            Some(std::borrow::Cow::Borrowed($target)),
            std::borrow::Cow::Borrowed($id),
        )
    };
    ($ctx: ident, $id: literal, $msg: expr, $time: expr, $target: literal) => {
        $ctx.push_event_with_time_and_source(
            ::litesim::event::Event::new($msg),
            $time,
            Some(std::borrow::Cow::Borrowed($target)),
            std::borrow::Cow::Borrowed($id),
        )
    };
}
