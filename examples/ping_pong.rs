use litesim::prelude::*;

pub struct Player;

#[litesim_model]
impl<'s> Model<'s> for Player {
    #[input(signal)]
    fn receive(&self, ctx: ModelCtx<'s>) -> Result<(), SimulationError> {
        ctx.schedule_update(Now)?;
        Ok(())
    }

    #[output(signal)]
    fn send(&self);

    fn handle_update(&mut self, ctx: ModelCtx<'s>) -> Result<(), SimulationError> {
        log::info!(
            "Player {} got the ball at {}",
            ctx.model_id.as_ref(),
            ctx.time
        );
        self.send(In(ctx.rand_range(0.0..1.0)))?;
        Ok(())
    }
}

fn main() {
    env_logger::builder()
        .filter_level(log::LevelFilter::Info)
        .init();

    let mut system = SystemModel::new();

    system.push_model("p1", Player);
    system.push_model("p2", Player);

    system.push_route(connection!(p1::send), connection!(p2::receive));
    system.push_route(connection!(p2::send), connection!(p1::receive));

    let mut sim = Simulation::new(rand::thread_rng(), system, 0.0).expect("invalid model");

    sim.schedule_event(0.5, Signal(), connection!(p1::receive))
        .expect("unable to schedule initial event");

    sim.run_until(50.0).expect("simulation error");
}
