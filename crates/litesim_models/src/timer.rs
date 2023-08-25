use std::ops::Bound;

use litesim::prelude::*;

#[derive(Default)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Timer {
    pub limits: TimeBounds,
    pub delay: Option<TimeDelta>,
    pub repeat: Option<TimeDelta>,
}

#[litesim_model]
impl<'s> Model<'s> for Timer {
    #[output(signal)]
    fn signal(&mut self);

    fn init(&mut self, ctx: ModelCtx<'s>) -> Result<(), SimulationError> {
        let initial = match self.limits.start {
            Bound::Excluded(limit) => At(limit),
            Bound::Included(limit) => At(limit),
            Bound::Unbounded => Now,
        }
        .to_discrete(ctx.time)
            + self.delay.unwrap_or(TimeDelta::MIN);

        let overshoot_initial = match self.limits.end {
            Bound::Excluded(limit) => initial > limit,
            Bound::Included(limit) => initial >= limit,
            Bound::Unbounded => false,
        };
        if !overshoot_initial {
            ctx.schedule_update(At(initial))?;
        }
        Ok(())
    }

    fn handle_update(&mut self, ctx: ModelCtx<'s>) -> Result<(), SimulationError> {
        self.signal()?;
        if let Some(repeat) = self.repeat {
            let next_time = ctx.time + repeat;
            let overshoot_next = match self.limits.end {
                Bound::Excluded(limit) => next_time > limit,
                Bound::Included(limit) => next_time >= limit,
                Bound::Unbounded => false,
            };
            if !overshoot_next {
                ctx.schedule_update(In(repeat))?;
            }
        }
        Ok(())
    }
}

#[cfg(feature = "rand")]
mod randomized {
    use std::{cell::RefCell, ops::Bound};

    use crate::generator::Generator;
    use litesim::prelude::*;
    use rand::{prelude::Distribution, Rng};

    use super::TimeBounds;

    #[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
    pub struct RandomizedTimer<Rng: SimulationRng, D: Distribution<TimeDelta> + 'static> {
        pub limits: TimeBounds,
        pub repeat: Option<TimeDelta>,
        pub generator: Generator<TimeDelta, Rng, D>,
    }

    impl<Rng: SimulationRng, D: Distribution<TimeDelta> + 'static> RandomizedTimer<Rng, D> {
        fn sample_delay<'a>(&'a mut self, default: &'a RefCell<dyn SimulationRng>) -> TimeDelta {
            match &mut self.generator.generator {
                Some(overriden) => {
                    return overriden.sample(&self.generator.distribution);
                }
                None => {
                    let mut borr = default.borrow_mut();
                    return borr.sample(&self.generator.distribution);
                }
            }
        }
    }

    #[litesim_model]
    impl<'s, Rng: SimulationRng, D: Distribution<TimeDelta> + 'static> Model<'s>
        for RandomizedTimer<Rng, D>
    {
        #[output(signal)]
        fn signal(&mut self);

        fn init(&mut self, ctx: ModelCtx<'s>) -> Result<(), SimulationError> {
            let initial = match self.limits.start {
                Bound::Excluded(limit) => At(limit),
                Bound::Included(limit) => At(limit),
                Bound::Unbounded => Now,
            };

            let added = self.sample_delay(&ctx.rng);

            let initial_discrete = initial.clone().to_discrete(ctx.time) + added;

            let overshoot_initial = match self.limits.end {
                Bound::Excluded(limit) => initial_discrete > limit,
                Bound::Included(limit) => initial_discrete >= limit,
                Bound::Unbounded => false,
            };
            if !overshoot_initial {
                ctx.schedule_update(initial)?;
            }
            Ok(())
        }

        fn handle_update(&mut self, ctx: ModelCtx<'s>) -> Result<(), SimulationError> {
            self.signal()?;
            if let Some(repeat) = self.repeat {
                let next_time = ctx.time + self.sample_delay(&ctx.rng);

                let overshoot_next = match self.limits.end {
                    Bound::Excluded(limit) => next_time > limit,
                    Bound::Included(limit) => next_time >= limit,
                    Bound::Unbounded => false,
                };
                if !overshoot_next {
                    ctx.schedule_update(In(repeat))?;
                }
            }
            Ok(())
        }
    }
}
pub use randomized::*;
