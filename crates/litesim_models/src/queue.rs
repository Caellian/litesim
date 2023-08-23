use std::collections::VecDeque;

use litesim::prelude::*;

pub struct Queue<T: Message> {
    queue: VecDeque<T>,
}

impl<'s, T: Message + 'static> Model<'s> for Queue<T> {
    fn input_connectors(&self) -> Vec<&'static str> {
        vec!["in", "pop"]
    }

    fn output_connectors(&self) -> Vec<OutputConnectorInfo> {
        vec![OutputConnectorInfo::new::<T>("out")]
    }

    fn get_input_handler<'h>(&self, index: usize) -> Option<Box<dyn ErasedInputHandler<'h, 's>>>
    where
        's: 'h,
    {
        match index {
            0 => {
                let handler: Box<
                    &dyn Fn(&mut Queue<T>, Event<T>, ModelCtx<'s>) -> Result<(), SimulationError>,
                > = Box::new(&|this: &mut Queue<T>, ev: Event<T>, _: ModelCtx<'s>| {
                    this.queue.push_front(ev.into_inner());
                    Ok(())
                });
                return Some(handler);
            }
            1 => {
                let handler: Box<
                    &dyn Fn(&mut Queue<T>, Signal, ModelCtx<'s>) -> Result<(), SimulationError>,
                > = Box::new(&|this: &mut Queue<T>, _: Signal, ctx: ModelCtx<'s>| {
                    push_event!(ctx, "out", this.queue.pop_back())
                });
                return Some(handler);
            }
            _ => return None,
        };
    }

    fn type_id(&self) -> std::any::TypeId {
        const_type_id::<Self>()
    }
}
