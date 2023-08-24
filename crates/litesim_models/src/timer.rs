use litesim::prelude::*;

#[derive(Default)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Timer {
    pub start: Option<SimulationTime>,
    pub end: Option<SimulationTime>,
    pub delay: Option<SimDuration>,
    pub repeat: Option<SimDuration>,
}

#[litesim_model]
impl<'s> Model<'s> for Timer {
    #[output(signal)]
    fn signal(&mut self);

    fn init(&mut self, ctx: ModelCtx<'s>) -> Result<(), SimulationError> {
        let initial = self
            .start
            .map(|it| self.delay.map(|delay| it + delay).unwrap_or(it).into())
            .or(self.delay.map(|it| In(it)))
            .unwrap_or(Now);
        if let Some(end) = self.end {
            if initial.clone().to_discrete(ctx.time) > end {
                return Ok(());
            }
        }
        ctx.schedule_change(initial)?;
        Ok(())
    }

    fn handle_update(&mut self, ctx: ModelCtx<'s>) -> Result<(), SimulationError> {
        self.signal()?;
        if let Some(repeat) = self.repeat {
            if let Some(end) = self.end {
                if ctx.time + repeat > end {
                    return Ok(());
                }
            }
            ctx.schedule_change(In(repeat))?;
        }
        Ok(())
    }
}
