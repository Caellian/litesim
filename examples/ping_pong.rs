use litesim::prelude::*;

pub struct Player;

/*
#[litesim]
impl<'s> Model<'s> for Player {
    #[input]
    fn receive(&mut self, _: Event<PingPongEvent>, ctx: SimulationCtx<'s>) {
        ctx.schedule_change(In(ctx.rand_range(0.0..1.0)))?;
        Ok(())
    },

    #[output]
    fn send(&self, data: PingPongEvent);

    fn handle_update(&mut self, ctx: SimulationCtx<'s>) -> Result<(), SimulationError> {
        // TODO: output connector should be strongly typed here assuming we've provided the names and types in output_connectors
        log::info!("Sending");
        self.send(PingPongEvent)
    }
}
*/

#[litesim_model]
impl<'s> Model<'s> for Player {
    #[input(signal)]
    fn receive(&self, ctx: ModelCtx<'s>) -> Result<(), SimulationError> {
        ctx.schedule_change(In(ctx.rand_range(0.0..1.0)))?;
        Ok(())
    }

    #[output(signal)]
    fn send(&self);

    fn handle_update(&mut self, _: ModelCtx<'s>) -> Result<(), SimulationError> {
        // TODO: output connector should be strongly typed here assuming we've provided the names and types in output_connectors
        log::info!("Sending");
        self.send()?;
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

    system.push_route(route!(p1::send -> p2::receive));
    system.push_route(route!(p2::send -> p1::receive));

    let mut sim = Simulation::new(rand::thread_rng(), system, 0.0).expect("invalid model");

    sim.scheduler_mut()
        .schedule_event(
            0.5,
            RoutedEvent::new_external(Signal::new(()), connection!(p1::receive)),
        )
        .expect("unable to schedule initial event");

    sim.run_until(50.0).expect("simulation error");
}
