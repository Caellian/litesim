use std::collections::VecDeque;

use litesim::prelude::*;

pub struct Queue<T: Message> {
    queue: VecDeque<T>,
}

#[litesim_model]
impl<'s, T: Message> Model<'s> for Queue<T> {
    #[input(name = "in")]
    fn input(&mut self, value: T, _: ModelCtx<'s>) -> Result<(), SimulationError> {
        self.queue.push_front(value);
        Ok(())
    }

    #[input(signal)]
    fn pop(&mut self, _: ModelCtx<'s>) -> Result<(), SimulationError> {
        if let Some(popped) = self.queue.pop_back() {
            self.output(popped);
        }
        Ok(())
    }

    #[output]
    fn output(&self, ev: T);
}
