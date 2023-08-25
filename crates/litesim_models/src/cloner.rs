use std::{borrow::Cow, marker::PhantomData};

use litesim::prelude::*;

pub struct Cloner<T: Message + Clone> {
    outputs: usize,
    _phantom: PhantomData<T>,
}

impl<T: Message + Clone> Cloner<T> {
    pub fn new(outputs: usize) -> Self {
        Cloner {
            outputs,
            _phantom: PhantomData,
        }
    }
}

#[litesim_model]
impl<'s, T: Message + Clone> Model<'s> for Cloner<T> {
    #[input]
    fn input(&mut self, value: T, ctx: ModelCtx<'s>) -> _ {
        for i in 0..self.outputs {
            ctx.push_event(
                Event::new(value.clone()),
                Cow::Owned(format!("output_{}", i)),
            )?;
        }
        Ok(())
    }

    fn output_connectors(&self) -> Vec<OutputConnectorInfo> {
        let mut result = Vec::with_capacity(self.outputs);
        for i in 0..self.outputs {
            result.push(OutputConnectorInfo::new::<T>(format!("output_{}", i)))
        }
        result
    }
}
