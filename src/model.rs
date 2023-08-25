use std::any::TypeId;

use crate::{
    error::{RoutingError, SimulationError},
    event::{ErasedEvent, Event, Message},
    routes::OutputConnectorInfo,
    simulation::{ConnectorCtx, ModelCtx},
};

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
