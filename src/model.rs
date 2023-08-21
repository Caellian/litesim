use std::{any::TypeId, fmt::Debug, marker::PhantomData, rc::Rc};

use crate::{
    error::{RoutingError, SimulationError},
    event::{Event, Message},
    prelude::{ConnectorCtx, ErasedEvent},
    simulation::SimulationCtx,
    util::{CowStr, HeterogeneousTuple},
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

// FIXME: Lifetimes are all over the place.
// Handlers assume connector lives as long as the simulation which isn't true

trait EventConsumer<Ctx>: Fn(Event<Self::Msg>, Ctx) -> Result<Self::Ok, SimulationError> {
    type Msg: Message;
    type Ok;
}
trait InputHandler<'s>: EventConsumer<SimulationCtx<'s>, Msg = Self::In, Ok = bool> {
    type In: Message;
}
impl<'s, F: EventConsumer<SimulationCtx<'s>, Ok = bool>> InputHandler<'s> for F {
    type In = F::Msg;
}

pub trait ErasedInputHandler<'s> {
    fn apply_erased_event(
        &self,
        event: ErasedEvent,
        ctx: SimulationCtx<'s>,
    ) -> Result<(), RoutingError>;
    fn in_type_id(&self) -> TypeId;
}

impl<'s, C: InputHandler<'s>> ErasedInputHandler<'s> for C {
    fn apply_erased_event(
        &self,
        event: ErasedEvent,
        ctx: SimulationCtx<'s>,
    ) -> Result<(), RoutingError> {
        let casted = event
            .try_restore_type()
            .map_err(|got| RoutingError::InvalidEventType {
                connector: std::any::type_name::<Self>(),
                event_type: got.type_name,
                expected: std::any::type_name::<C::In>(),
            })?;
        self(casted, ctx);
        Ok(())
    }

    fn in_type_id(&self) -> TypeId {
        TypeId::of::<C::In>()
    }
}

type InputConnector<'s> = (&'static str, Rc<dyn ErasedInputHandler<'s>>);

pub trait InputConnectorList<'s> {
    fn get_entry(&self, n: usize) -> InputConnector<'s>;

    fn iter<'a>(&'a self) -> InputConnectorIter<'a, 's, Self>
    where
        Self: Sized + HeterogeneousTuple,
    {
        InputConnectorIter {
            over: self,
            pos: 0,
            _phantom: PhantomData,
        }
    }
}

pub struct InputConnectorIter<'a, 's: 'a, L: InputConnectorList<'s> + HeterogeneousTuple> {
    over: &'a L,
    pos: u16,
    _phantom: PhantomData<&'s ()>,
}

impl<'s, 'a: 's, L: InputConnectorList<'s> + HeterogeneousTuple> Iterator
    for InputConnectorIter<'a, 's, L>
{
    type Item = InputConnector<'s>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.pos >= self.over.len() as u16 {
            return None;
        }
        let result = self.over.get_entry(self.pos as usize);
        self.pos += 1;
        return Some(result);
    }
}

trait OutputHandler<'c, 's: 'c>: EventConsumer<ConnectorCtx<'c, 's>, Msg = Self::Out, Ok = ()> {
    type Out: Message;
}
impl<'c, 's: 'c, F: EventConsumer<ConnectorCtx<'c, 's>, Ok = ()>> OutputHandler<'c, 's> for F {
    type Out = F::Msg;
}

pub trait ErasedOutputHandler<'s> {
    fn apply_erased_event<'c: 's>(
        &'c mut self,
        event: ErasedEvent,
        ctx: ConnectorCtx<'c, 's>,
    ) -> Result<(), RoutingError>;
    fn out_type_id(&self) -> TypeId;
}

impl<'c, 's: 'c, C: OutputHandler<'c, 's>> ErasedOutputHandler<'s> for C {
    fn apply_erased_event<'a: 'c>(
        &'a mut self,
        event: ErasedEvent,
        ctx: ConnectorCtx<'a, 's>,
    ) -> Result<(), RoutingError> {
        let casted = event
            .try_restore_type()
            .map_err(|got| RoutingError::InvalidEventType {
                connector: std::any::type_name::<Self>(),
                event_type: got.type_name,
                expected: std::any::type_name::<C::Out>(),
            })?;
        self(casted, ctx);
        Ok(())
    }

    fn out_type_id(&self) -> TypeId {
        TypeId::of::<C::Out>()
    }
}

type OutputConnector<'s> = (&'static str, Rc<dyn ErasedOutputHandler<'s>>);

pub trait OutputConnectorList<'s> {
    fn get_entry(&self, n: usize) -> OutputConnector<'s>;

    fn iter<'i>(&'i self) -> OutputConnectorIter<'i, 's, Self>
    where
        Self: Sized + HeterogeneousTuple,
    {
        OutputConnectorIter {
            over: self,
            pos: 0,
            _phantom: PhantomData,
        }
    }
}

pub struct OutputConnectorIter<'i, 's: 'i, L: OutputConnectorList<'s> + HeterogeneousTuple> {
    over: &'i L,
    pos: u16,
    _phantom: PhantomData<&'s ()>,
}

impl<'i, 'c, 's: 'c + 'i, L: OutputConnectorList<'s> + HeterogeneousTuple> Iterator
    for OutputConnectorIter<'i, 's, L>
{
    type Item = OutputConnector<'s>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.pos >= self.over.len() as u16 {
            return None;
        }
        let result = self.over.get_entry(self.pos as usize);
        self.pos += 1;
        return Some(result);
    }
}

macro_rules! in_connector_tuple {
    ($($size: literal => [$($n: tt : $sym: tt),*]),+) => {$(
        impl<'s, $($sym: InputHandler<'s> + 'static),*> InputConnectorList<'s> for ($((&'static str, Rc<$sym>)),*) {
            fn get_entry(&self, n: usize) -> InputConnector<'s> {
                match n {
                    $($n => return (self.$n.0, self.$n.1.clone()),)*
                    _ => panic!("Invalid connector index"),
                }
            }
        }
    )+};
}
macro_rules! out_connector_tuple {
    ($($size: literal => [$($n: tt : $sym: tt),*]),+) => {$(
        impl<'c, 's: 'c, $($sym: OutputHandler<'c, 's> + 'static),*> OutputConnectorList<'s> for ($((&'static str, Rc<$sym>)),*) {
            fn get_entry(&self, n: usize) -> OutputConnector<'s> {
                match n {
                    $($n => return (self.$n.0, self.$n.1.clone()),)*
                    _ => panic!("Invalid connector index"),
                }
            }
        }
    )+};
}
macro_rules! connector_tuple {
    ($($size: literal => [$($n: tt : $sym: tt),*]),+) => {
        in_connector_tuple![$($size => [$($n: $sym),*]),+];
        out_connector_tuple![$($size => [$($n: $sym),*]),+];
    };
}

impl<'s, A: InputHandler<'s> + 'static> InputConnectorList<'s> for ((&'static str, Rc<A>),) {
    fn get_entry(&self, n: usize) -> InputConnector<'s> {
        match n {
            0 => return (self.0 .0, self.0 .1.clone()),
            _ => panic!("Invalid connector index"),
        }
    }
}
impl<'c, 's: 'c, A: OutputHandler<'c, 's> + 'static> OutputConnectorList<'s>
    for ((&'static str, Rc<A>),)
{
    fn get_entry(&self, n: usize) -> OutputConnector<'s> {
        match n {
            0 => return (self.0 .0, self.0 .1.clone()),
            _ => panic!("Invalid connector index"),
        }
    }
}

connector_tuple![
    0 => [],
    2 => [0: A, 1: B],
    3 => [0: A, 1: B, 2: C],
    4 => [0: A, 1: B, 2: C, 3: D],
    5 => [0: A, 1: B, 2: C, 3: D, 4: E],
    6 => [0: A, 1: B, 2: C, 3: D, 4: E, 5: F],
    7 => [0: A, 1: B, 2: C, 3: D, 4: E, 5: F, 6: G],
    8 => [0: A, 1: B, 2: C, 3: D, 4: E, 5: F, 6: G, 7: H],
    9 => [0: A, 1: B, 2: C, 3: D, 4: E, 5: F, 6: G, 7: H, 8: I],
    10 => [0: A, 1: B, 2: C, 3: D, 4: E, 5: F, 6: G, 7: H, 8: I, 9: J],
    11 => [0: A, 1: B, 2: C, 3: D, 4: E, 5: F, 6: G, 7: H, 8: I, 9: J, 10: K],
    12 => [0: A, 1: B, 2: C, 3: D, 4: E, 5: F, 6: G, 7: H, 8: I, 9: J, 10: K, 11: L]
];

#[macro_export]
macro_rules! push {
    ($ctx: ident, $id: literal, $msg: expr) => {{
        let connector_ctx = ctx.on_connector($id);
        connector_ctx.push_event(::litesim::event::Event::new($msg), None)?;
        Ok(())
    }};
    ($ctx: ident, $id: literal, $msg: expr, $time: expr) => {{
        let connector_ctx = ctx.on_connector($id);
        connector_ctx.push_event_with_time(::litesim::event::Event::new($msg), $time, None)?;
        Ok(())
    }};
    ($ctx: ident, $id: literal, $msg: expr, Now, $target: literal) => {{
        let connector_ctx = ctx.on_connector($id);
        connector_ctx.push_event(
            ::litesim::event::Event::new($msg),
            Some(::litesim::prelude::CowStr::Borrowed($target)),
        )?;
        Ok(())
    }};
    ($ctx: ident, $id: literal, $msg: expr, $time: expr, $target: literal) => {{
        let connector_ctx = ctx.on_connector($id);
        connector_ctx.push_event_with_time(
            ::litesim::event::Event::new($msg),
            $time,
            Some(::litesim::prelude::CowStr::Borrowed($target)),
        )?;
        Ok(())
    }};
}

pub trait Model<'s> {
    type I: InputConnectorList<'s> + HeterogeneousTuple + 'static;
    type O: OutputConnectorList<'s> + HeterogeneousTuple + 'static;

    /// Lists all model input connectors
    fn input_connectors(&'s self) -> Self::I;
    /// Lists all model output connectors
    fn output_connectors(&'s self) -> Self::O;

    /// Called during initalization.
    ///
    /// This method allows models like generators to schedule inital changes.
    #[allow(unused_variables)]
    fn init(&mut self, ctx: SimulationCtx<'s>) -> Result<(), SimulationError> {
        Ok(())
    }

    /// Handler for internal model changes when the elapsed time is supposed to affect
    /// the state of the model.'
    fn handle_update(&mut self, ctx: SimulationCtx<'s>) -> Result<(), SimulationError>;
}

pub trait ErasedModel<'s> {
    fn get_erased_input_handler<'a>(&'a self, id: &str) -> Option<Rc<dyn ErasedInputHandler<'s>>>
    where
        's: 'a;
    fn get_erased_output_handler<'a, 'c: 'a>(
        &'a self,
        id: &str,
    ) -> Option<Rc<dyn ErasedOutputHandler<'s>>>
    where
        's: 'a;

    fn erased_init(&mut self, ctx: SimulationCtx<'s>) -> Result<(), SimulationError>;
    fn erased_handle_update(&mut self, ctx: SimulationCtx<'s>) -> Result<(), SimulationError>;
}

impl<'s, M: Model<'s>> ErasedModel<'s> for M {
    fn get_erased_input_handler<'a>(&'a self, id: &str) -> Option<Rc<dyn ErasedInputHandler<'s>>>
    where
        's: 'a,
    {
        self.input_connectors()
            .iter()
            .find(|it| it.0 == id)
            .map(|it| it.1.clone())
    }

    fn get_erased_output_handler<'a, 'c: 'a>(
        &'a self,
        id: &str,
    ) -> Option<Rc<dyn ErasedOutputHandler<'s>>>
    where
        's: 'a,
    {
        self.output_connectors()
            .iter()
            .find(|it| it.0 == id)
            .map(|it| it.1.clone())
    }

    fn erased_init(&mut self, ctx: SimulationCtx<'s>) -> Result<(), SimulationError> {
        self.init(ctx)
    }

    fn erased_handle_update(&mut self, ctx: SimulationCtx<'s>) -> Result<(), SimulationError> {
        self.handle_update(ctx)
    }
}
