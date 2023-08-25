use litesim::prelude::*;
use serde::{Deserialize, Serialize};

#[derive(Default, Serialize, Deserialize)]
pub struct Fish {
    was_bumped: bool,
    bump_count: usize,
}

#[litesim_model]
impl<'s> Model<'s> for Fish {
    #[input(signal)]
    fn get_bumped(&mut self, ctx: ModelCtx<'s>) -> _ {
        ctx.cancel_updates();
        if self.bump_count > 20 {
            return Ok(());
        }
        self.was_bumped = true;
        ctx.schedule_update(Now)?;
        Ok(())
    }

    #[output(signal)]
    fn bump(&self) -> _;

    fn init(&mut self, ctx: ModelCtx<'s>) -> Result<(), SimulationError> {
        log::info!("{} woke up!", ctx.model_id);
        ctx.schedule_update(In(ctx.rand_range(0.0..1.0)))?;
        Ok(())
    }

    fn handle_update(&mut self, ctx: ModelCtx<'s>) -> Result<(), SimulationError> {
        if self.was_bumped {
            log::info!(
                "{} moved at {} because he was bumped",
                ctx.model_id,
                ctx.time
            );
            self.was_bumped = false;
            self.bump_count += 1;
        }

        if ctx.rand::<f32>() < 0.2f32 {
            self.bump()?;
        }

        if self.bump_count > 20 {
            log::info!(
                "{} died after being bumped {} times",
                ctx.model_id,
                self.bump_count
            );
        } else {
            ctx.schedule_update(In(ctx.rand_range(0.0..1.0)))?;
        }

        Ok(())
    }
}

fn main() {
    env_logger::builder()
        .filter_level(log::LevelFilter::Info)
        .init();

    let mut system = SystemModel::new();

    system.push_model("Jerry", Fish::default());
    system.push_model("Larry", Fish::default());
    system.push_model("Berry", Fish::default());
    system.push_model("Harry", Fish::default());

    system.push_route(connection!(Jerry::bump), connection!(Larry::get_bumped));
    system.push_route(connection!(Larry::bump), connection!(Berry::get_bumped));
    system.push_route(connection!(Berry::bump), connection!(Harry::get_bumped));
    system.push_route(connection!(Harry::bump), connection!(Jerry::get_bumped));

    let mut sim = Simulation::new(rand::thread_rng(), system, 0.0).expect("invalid model");

    sim.run_until(10.0).expect("simulation error");
    sim.run_until(20.0).expect("simulation error");
    sim.run_until(30.0).expect("simulation error");
    sim.run_until(40.0).expect("simulation error");
}
