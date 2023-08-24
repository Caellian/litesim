use std::{cell::RefCell, marker::PhantomData};

use litesim::prelude::*;
use rand::{distributions::Standard, prelude::Distribution, rngs::ThreadRng, Rng};

#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Generator<T, Rng: SimulationRng = ThreadRng, D: Distribution<T> = Standard> {
    generator: Option<Rng>,
    distribution: D,
    _phantom: PhantomData<T>,
}

impl<T, Rng: SimulationRng, D: Distribution<T>> Generator<T, Rng, D> {
    pub fn new(generator: Option<Rng>, distribution: D) -> Self {
        Generator {
            generator,
            distribution,
            _phantom: PhantomData,
        }
    }

    fn sample<'a>(&'a mut self, default: &'a RefCell<dyn SimulationRng>) -> T {
        match &mut self.generator {
            Some(overriden) => {
                return overriden.sample(&self.distribution);
            }
            None => {
                let mut borr = default.borrow_mut();
                return borr.sample(&self.distribution);
            }
        }
    }
}

impl<T, Rng: SimulationRng> Generator<T, Rng>
where
    Standard: Distribution<T>,
{
    pub fn new_standard(generator: Option<Rng>) -> Self {
        Self::new(generator, Standard)
    }
}

impl<T, D: Distribution<T>> Generator<T, ThreadRng, D> {
    pub fn new_shared() -> Generator<T, ThreadRng, D>
    where
        D: Default,
    {
        Self::new(None::<ThreadRng>, Default::default())
    }
}

impl<T> Generator<T>
where
    Standard: Distribution<T>,
{
    pub fn new_shared_standard() -> Self {
        Self::new(None, Standard)
    }

    pub fn new_thread_standard() -> Self {
        Self::new(Some(rand::thread_rng()), Standard)
    }
}

#[litesim_model]
impl<'s, T: 'static, Rng: SimulationRng, D: Distribution<T> + 'static> Model<'s>
    for Generator<T, Rng, D>
{
    #[input(signal)]
    fn generate(&mut self, ctx: ModelCtx<'s>) -> Result<(), SimulationError> {
        ctx.schedule_change(Now)?;
        Ok(())
    }

    #[output]
    fn output(&self, value: T);

    fn handle_update(&mut self, ctx: ModelCtx<'s>) -> Result<(), SimulationError> {
        let generated = self.sample(&ctx.rng);
        self.output(generated)?;
        Ok(())
    }
}
