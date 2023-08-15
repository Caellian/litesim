use litesim::prelude::*;

pub struct PingPongEvent;
impl Event for PingPongEvent {
    fn type_id() -> &'static str {
        "ping_pong"
    }
}

pub struct Player;

impl Model<PingPongEvent> for Player {
    declare_connectors! {
        input: ["recieve"],
        output: ["send"]
    }

    fn handle_input<'s>(
        &mut self,
        _event: PingPongEvent,
        connector: litesim::util::CowStr<'s>,
        sim: SimulationRef,
        source: EventSource<'s>,
    ) -> InputEffect<'s, PingPongEvent> {
        log::info!(
            "Player {} received ball on connector: {}, from {:?}, at: {}",
            sim.owner,
            connector,
            source,
            sim.time,
        );
        InputEffect::ScheduleInternal(sim.time + sim.rand_range(0.0..1.0))
    }

    fn handle_change<'s>(&mut self, sim: SimulationRef<'s>) -> ChangeEffect<'s, PingPongEvent> {
        log::info!("Player {} bounced at: {}", sim.owner, sim.time);
        ChangeEffect::Produce(sim.create_event(PingPongEvent, "send", None))
    }
}

fn main() {
    env_logger::builder()
        .filter_level(log::LevelFilter::Info)
        .init();

    let mut system = SystemModel::new();

    system.push_model("p1", Player);
    system.push_model("p2", Player);

    system.push_route(route!(p1::send -> p2::recieve));
    system.push_route(route!(p2::send -> p1::recieve));

    let mut sim = Simulation::new(rand::thread_rng(), system, 0.0).expect("invalid model");

    sim.scheduler_mut()
        .schedule_event(
            0.5,
            RoutedEvent::new_external(PingPongEvent, connection!(p1::recieve)),
        )
        .expect("unable to schedule initial event");

    sim.run_until(50.0).expect("simulation error");
}
