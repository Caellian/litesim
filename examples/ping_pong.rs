use litesim::prelude::*;

pub struct PingPongEvent;

pub struct Player;

//FIXME: 's doesn't outlive 'static
impl<'s: 'static> Model<'s> for Player {
    fn input_connectors(&self) -> &'static [&'static str] {
        static RESULT: &[&'static str] = &["receive"];
        RESULT
    }

    fn output_connectors(&self) -> &'static [OutputConnectorInfo] {
        static RESULT: &[OutputConnectorInfo] = &[OutputConnectorInfo(
            "send",
            const_type_id::<PingPongEvent>(),
        )];
        RESULT
    }

    fn get_input_handler(&self, index: usize) -> Option<Box<dyn ErasedInputHandler<'s>>> {
        let handler: &dyn Fn(
            Event<PingPongEvent>,
            SimulationCtx<'s>,
        ) -> Result<(), SimulationError> = &|_: Event<PingPongEvent>, ctx: SimulationCtx<'s>| {
            ctx.schedule_change(In(ctx.rand_range(0.0..1.0)))?;
            Ok(())
        };

        let c: Box<dyn ErasedInputHandler<'s>> = Box::new(handler);

        match index {
            0 => Some(c),
            _ => None,
        }
    }

    fn handle_update(&mut self, ctx: SimulationCtx<'s>) -> Result<(), SimulationError> {
        // TODO: output connector should be strongly typed here assuming we've provided the names and types in output_connectors
        log::info!("Sending");
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
            RoutedEvent::new_external(Event::new(PingPongEvent), connection!(p1::receive)),
        )
        .expect("unable to schedule initial event");

    sim.run_until(50.0).expect("simulation error");
}
