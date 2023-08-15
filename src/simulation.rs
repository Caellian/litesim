use std::{
    cell::RefCell,
    collections::{BTreeMap, HashMap},
    pin::Pin,
    rc::Rc,
};

use rand::Rng;

use crate::{
    error::{SchedulerError, SimulationError},
    event::{Event, ProducedEvent, RoutedEvent},
    model::{ChangeEffect, ConnectorPath, InputEffect},
    system::{AdjacentModels, BorrowedModel, SystemModel},
    time::{SimulationTime, TimeTrigger},
    util::{CowStr, SimulationRng, ToCowStr},
};

pub struct Simulation<'s, E: Event> {
    global_rng: Rc<RefCell<dyn SimulationRng>>,
    system: Pin<Box<SystemModel<'s, E>>>,
    initial_time: SimulationTime,
    scheduler: Scheduler<'s, E>,
}

impl<'s, E: Event + 's> Simulation<'s, E> {
    pub fn new(
        rng: impl SimulationRng + 'static,
        mut system: SystemModel<'s, E>,
        initial_time: impl Into<SimulationTime>,
    ) -> Result<Self, SimulationError> {
        system.validate()?;

        let global_rng = Rc::new(RefCell::new(rng));
        let initial_time = initial_time.into();

        let mut scheduler = Scheduler::new(initial_time);
        for (id, model) in system.models.iter_mut() {
            let route_cache = system.route_cache.borrow();
            let sim_ref = SimulationCtx::new_parameterized::<E>(
                &*route_cache,
                initial_time,
                global_rng.clone(),
                id.clone(),
            );

            if let Some(time) = model.next_change_time(sim_ref) {
                scheduler.schedule_internal(time, id.clone())?;
            }
        }

        Ok(Simulation {
            global_rng,
            system: Box::pin(system),
            initial_time,
            scheduler,
        })
    }

    pub fn scheduler(&self) -> &Scheduler<'s, E> {
        &self.scheduler
    }

    pub fn scheduler_mut(&mut self) -> &mut Scheduler<'s, E> {
        &mut self.scheduler
    }

    pub fn current_time(&self) -> SimulationTime {
        self.scheduler.time
    }

    fn handle_produced_event(
        &mut self,
        model_id: CowStr<'s>,
        event: ProducedEvent<'s, E>,
        default_target: Option<ConnectorPath<'s>>,
    ) -> Result<(), SimulationError> {
        let scheduling = event.scheduling.clone();
        let routed = RoutedEvent::from_produced(model_id, event, default_target)?;
        let time = self.initial_time.clone();
        match scheduling {
            TimeTrigger::Now => self.route_event(routed)?,
            scheduling => self
                .scheduler
                .schedule_event(scheduling.to_discrete(time), routed)?,
        }

        Ok(())
    }

    pub fn route_event(&mut self, event: RoutedEvent<'s, E>) -> Result<(), SimulationError> {
        let RoutedEvent { event, route } = event;
        let target_model = route.to.model.clone();
        let target_connector = route.to.connector.clone();
        let mut model = BorrowedModel::new(&mut self.system, target_model.clone()).ok_or(
            SimulationError::ModelNotFound {
                id: target_model.to_string(),
            },
        )?;

        let state = SimulationCtx::new(&self, target_model.clone());

        let result = model.handle_input(event, target_connector, state, route.from.clone());

        match result {
            InputEffect::Consume | InputEffect::Drop(_) => {}
            InputEffect::ScheduleInternal(time) => {
                self.scheduler.schedule_internal(time, target_model)?;
            }
            InputEffect::Produce(event) => {
                let adjacent = self
                    .system
                    .route_cache
                    .borrow()
                    .get(&target_model)
                    .cloned()
                    .unwrap_or_default();
                let default_target = if adjacent.outputs.len() == 1 {
                    adjacent.outputs.first().cloned()
                } else {
                    None
                };
                self.handle_produced_event(target_model, event, default_target)?;
            }
        }

        Ok(())
    }

    /// Runs simulation until passed time is reached (inclusive) or the simulated system becomes inert
    pub fn run_until(&mut self, time: impl Into<SimulationTime>) -> Result<(), SimulationError> {
        let max_time = time.into();

        while let Some(expected_time) = self.scheduler.get_next_time() {
            if expected_time >= max_time {
                break;
            }
            let scheduled = self.scheduler.next().unwrap();

            for entry in scheduled {
                match entry {
                    Scheduled::Internal(model_id) => {
                        let state = SimulationCtx::new(&self, model_id.clone());
                        let default_target = if state.routes.outputs.len() == 1 {
                            state.routes.outputs.first().cloned()
                        } else {
                            None
                        };
                        let mut model = BorrowedModel::new(&mut self.system, model_id.clone())
                            .ok_or(SimulationError::ModelNotFound {
                                id: model_id.to_string(),
                            })?;

                        let produced: Option<ProducedEvent<'_, E>> =
                            match model.handle_change(state) {
                                ChangeEffect::None => None,
                                ChangeEffect::ScheduleInternal(new_time) => {
                                    self.scheduler
                                        .schedule_internal(new_time, model_id.clone())?;
                                    None
                                }
                                ChangeEffect::Produce(produced) => Some(produced),
                            };

                        if let Some(event) = produced {
                            self.handle_produced_event(model_id, event, default_target)?;
                        }
                    }
                    Scheduled::Event(event) => {
                        self.route_event(event)?;
                    }
                }
            }
        }

        Ok(())
    }

    /// Runs simulation until the simulated system becomes inert
    pub fn run(&mut self) -> Result<(), SimulationError> {
        self.run_until(SimulationTime::MAX)
    }
}

#[derive(Clone)]
pub struct SimulationCtx<'s> {
    pub time: SimulationTime,
    pub rng: Rc<RefCell<dyn SimulationRng>>,
    pub owner: CowStr<'s>,
    pub routes: AdjacentModels<'s>,
}

impl<'s> SimulationCtx<'s> {
    pub fn new<E: Event + 's>(simulation: &Simulation<'s, E>, owner: CowStr<'s>) -> Self {
        let routes = simulation
            .system
            .route_cache
            .borrow()
            .get(&owner)
            .cloned()
            .unwrap_or_default();

        SimulationCtx {
            time: simulation.current_time(),
            rng: simulation.global_rng.clone(),
            owner,
            routes,
        }
    }

    fn new_parameterized<E: Event + 's>(
        route_cache: &HashMap<CowStr<'s>, AdjacentModels<'s>>,
        time: SimulationTime,
        rng: Rc<RefCell<dyn SimulationRng>>,
        owner: CowStr<'s>,
    ) -> Self {
        let routes = route_cache.get(&owner).cloned().unwrap_or_default();

        SimulationCtx {
            time,
            rng,
            owner,
            routes,
        }
    }

    pub fn create_event<E: Event>(
        &self,
        event: E,
        source_connector: impl ToCowStr<'s>,
        target: Option<ConnectorPath<'s>>,
    ) -> ProducedEvent<'s, E> {
        ProducedEvent::new_instant(event, source_connector.to_cow_str(), target)
    }

    pub fn rand<T>(&self) -> T
    where
        rand::distributions::Standard: rand::prelude::Distribution<T>,
    {
        self.rng.borrow_mut().gen()
    }

    pub fn rand_range<T, R>(&self, range: R) -> T
    where
        T: rand::distributions::uniform::SampleUniform,
        R: rand::distributions::uniform::SampleRange<T>,
    {
        self.rng.borrow_mut().gen_range(range)
    }
}

pub enum Scheduled<'s, E: Event> {
    Internal(CowStr<'s>),
    Event(RoutedEvent<'s, E>),
}

pub struct Scheduler<'s, E: Event> {
    pub time: SimulationTime,
    scheduled: BTreeMap<SimulationTime, Vec<Scheduled<'s, E>>>,
}

impl<'s, E: Event> Scheduler<'s, E> {
    pub fn new(current_time: SimulationTime) -> Self {
        Scheduler {
            time: current_time,
            scheduled: BTreeMap::new(),
        }
    }

    fn schedule(
        &mut self,
        time: SimulationTime,
        value: Scheduled<'s, E>,
    ) -> Result<(), SchedulerError> {
        if time < self.time {
            return Err(SchedulerError::TimeRegression {
                current: self.time.clone(),
                insertion: time,
            });
        }

        match self.scheduled.get_mut(&time) {
            Some(events) => {
                events.push(value);
            }
            None => {
                self.scheduled.insert(time, vec![value]);
            }
        }

        Ok(())
    }

    #[inline]
    pub fn schedule_internal(
        &mut self,
        time: impl Into<SimulationTime>,
        model: CowStr<'s>,
    ) -> Result<(), SchedulerError> {
        self.schedule(time.into(), Scheduled::Internal(model))
    }

    #[inline]
    pub fn schedule_event(
        &mut self,
        time: impl Into<SimulationTime>,
        event: RoutedEvent<'s, E>,
    ) -> Result<(), SchedulerError> {
        self.schedule(time.into(), Scheduled::Event(event))
    }

    pub fn get_next_time(&self) -> Option<SimulationTime> {
        self.scheduled.first_key_value().map(|(it, _)| it.clone())
    }
}

impl<'s, E: Event> Iterator for Scheduler<'s, E> {
    type Item = Vec<Scheduled<'s, E>>;

    fn next(&mut self) -> Option<Self::Item> {
        let (time, result) = self.scheduled.pop_first()?;
        self.time = time;
        Some(result)
    }
}
