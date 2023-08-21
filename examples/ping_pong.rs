use std::rc::Rc;

use litesim::prelude::*;

pub struct PingPongEvent;

pub struct Player;

fn recieve_handler(_: PingPongEvent, ctx: SimulationCtx) -> Result<(), SimulationError> {
    ctx.schedule_change(In(ctx.rand_range(0.0..1.0)));
    Ok(())
}

impl<'s> Model<'s> for Player {
    type I = ((&'static str, Rc<dyn ErasedInputHandler<'s>>),);
    type O = ();

    fn input_connectors(&self) -> Self::I {
        (("receive", Rc::new(recieve_handler)),)
    }

    fn output_connectors(&self) -> Self::O {
        ()
    }

    fn handle_update(&mut self, ctx: SimulationCtx<'s>) -> Result<(), SimulationError> {
        // TODO: output connector should be strongly typed here assuming we've provided the names and types in output_connectors
        push_event!(ctx, "send", PingPongEvent)
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
            RoutedEvent::new_external(Event::new(PingPongEvent), connection!(p1::recieve)),
        )
        .expect("unable to schedule initial event");

    sim.run_until(50.0).expect("simulation error");
}
