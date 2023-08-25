use std::collections::VecDeque;

use litesim::prelude::*;

#[derive(Default)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Queue<T: Message> {
    queue: VecDeque<T>,
}

#[litesim_model]
impl<'s, T: Message> Model<'s> for Queue<T> {
    #[input]
    fn input(&mut self, value: T, _: ModelCtx<'s>) -> _ {
        self.queue.push_front(value);
        Ok(())
    }

    #[input(signal)]
    fn pop(&mut self, _: ModelCtx<'s>) -> _ {
        if let Some(popped) = self.queue.pop_back() {
            self.output(popped)?;
        }
        Ok(())
    }

    #[output]
    fn output(&self, ev: T) -> _;
}
