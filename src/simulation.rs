use std::{
    cell::RefCell,
    collections::{BTreeMap, HashMap},
    ops::Deref,
    pin::Pin,
    rc::Rc,
};

use rand::Rng;

use crate::{
    error::{RoutingError, SchedulerError, SimulationError},
    event::{Event, Message, ProducedEvent, RoutedEvent},
    model::{ConnectorPath, EventSource, ModelImpl, Route},
    system::{AdjacentModels, BorrowedModel, SystemModel},
    time::{SimulationTime, TimeTrigger},
    util::{CowStr, SimulationRng},
};

pub struct Simulation<'s> {
    global_rng: Rc<RefCell<dyn SimulationRng>>,
    system: Pin<Box<SystemModel<'s>>>,
    initial_time: SimulationTime,
    scheduler: Pin<Box<Scheduler<'s>>>,
}

impl<'s> Simulation<'s> {
    pub fn new(
        rng: impl SimulationRng + 'static,
        mut system: SystemModel<'s>,
        initial_time: impl Into<SimulationTime>,
    ) -> Result<Self, SimulationError> {
        system.validate()?;

        let global_rng = Rc::new(RefCell::new(rng));
        let initial_time = initial_time.into();

        let mut scheduler = Box::pin(Scheduler::new(initial_time));
        for (id, model) in system.models.iter_mut() {
            let route_cache = system.route_cache.borrow();
            let sim_ref = SimulationCtx::new_parameterized(
                &*route_cache,
                initial_time,
                global_rng.clone(),
                id.clone(),
                &mut scheduler,
            );

            model.init(sim_ref);
        }

        Ok(Simulation {
            global_rng,
            system: Box::pin(system),
            initial_time,
            scheduler,
        })
    }

    pub fn scheduler(&self) -> &Scheduler<'s> {
        &self.scheduler
    }

    pub fn scheduler_mut(&mut self) -> &mut Scheduler<'s> {
        &mut self.scheduler
    }

    pub fn current_time(&self) -> SimulationTime {
        self.scheduler.time
    }

    fn handle_produced_event(
        &mut self,
        model_id: CowStr<'s>,
        event: ProducedEvent<'s>,
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

    pub fn route_event(&mut self, event: RoutedEvent<'s>) -> Result<(), SimulationError> {
        let RoutedEvent { event, route } = event;
        let target_model = route.to.model.clone();
        let target_connector = route.to.connector.clone();
        let model = BorrowedModel::new(&mut self.system, target_model.clone()).ok_or(
            SimulationError::ModelNotFound {
                id: target_model.to_string(),
            },
        )?;

        let state = SimulationCtx::new(self, target_model.clone());

        let handler = model
            .get_input_handler_by_name(target_connector.as_ref())
            .ok_or_else(|| RoutingError::UnknownModelConnector {
                model: target_model.to_string(),
                connector: target_connector.to_string(),
            })?;

        handler.apply_erased_event(event, state)?;
        /* TODO: Handle after input event
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
        }*/

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
                        let state = SimulationCtx::new(self, model_id.clone());
                        let default_target = if state.routes.outputs.len() == 1 {
                            state.routes.outputs.first().cloned()
                        } else {
                            None
                        };
                        let mut model = BorrowedModel::new(&mut self.system, model_id.clone())
                            .ok_or(SimulationError::ModelNotFound {
                                id: model_id.to_string(),
                            })?;

                        /* TODO: Handle after internal
                        let produced: Option<ProducedEvent<'_, E>> =
                            match model.handle_update(state) {
                                ChangeEffect::None => None,
                                ChangeEffect::ScheduleInternal(new_time) => {
                                    self.scheduler.schedule_change(new_time, model_id.clone())?;
                                    None
                                }
                                ChangeEffect::Produce(produced) => Some(produced),
                            };

                        if let Some(event) = produced {
                            self.handle_produced_event(model_id, event, default_target)?;
                        }
                        */
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
    pub on_model: CowStr<'s>,
    pub routes: AdjacentModels<'s>,
    pub scheduler: *mut Pin<Box<Scheduler<'s>>>,
}

impl<'s> SimulationCtx<'s> {
    pub fn new(simulation: &mut Simulation<'s>, owner: CowStr<'s>) -> Self {
        let routes = simulation
            .system
            .route_cache
            .borrow()
            .get(&owner)
            .cloned()
            .unwrap_or_default();

        let scheduler: *mut Pin<Box<Scheduler<'s>>> = &mut simulation.scheduler;

        SimulationCtx {
            time: simulation.current_time(),
            rng: simulation.global_rng.clone(),
            on_model: owner,
            routes,
            scheduler,
        }
    }

    fn new_parameterized(
        route_cache: &HashMap<CowStr<'s>, AdjacentModels<'s>>,
        time: SimulationTime,
        rng: Rc<RefCell<dyn SimulationRng>>,
        owner: CowStr<'s>,
        scheduler: &mut Pin<Box<Scheduler<'s>>>,
    ) -> Self {
        let routes = route_cache.get(&owner).cloned().unwrap_or_default();

        let scheduler: *mut Pin<Box<Scheduler<'s>>> = scheduler;

        SimulationCtx {
            time,
            rng,
            on_model: owner,
            routes,
            scheduler,
        }
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

    pub fn schedule_change(&self, time: TimeTrigger) -> Result<(), SimulationError> {
        unsafe {
            (*self.scheduler)
                .schedule_change(time.to_discrete(self.time), self.on_model.clone())?;
        }
        Ok(())
    }

    pub fn on_connector<'c>(&'c self, connector: CowStr<'s>) -> ConnectorCtx<'c, 's>
    where
        's: 'c,
    {
        ConnectorCtx {
            sim: self,
            on_connector: connector,
        }
    }

    pub fn push_event_with_time_and_source<M: Message>(
        &self,
        event: Event<M>,
        target: Option<ConnectorPath<'s>>,
        time: impl Into<SimulationTime>,
        source_connector: CowStr<'s>,
    ) -> Result<(), SimulationError> {
        let target = target
            .or_else(|| match self.routes.outputs.first() {
                Some(first) if self.routes.outputs.len() == 1 => Some(first.to.clone()),
                _ => None,
            })
            .ok_or(RoutingError::MissingEventTarget {
                model: self.on_model.to_string(),
            })?;
        let routed = RoutedEvent::new(
            event,
            Route {
                from: EventSource::Model(ConnectorPath {
                    model: self.on_model.clone(),
                    connector: source_connector,
                }),
                to: target,
            },
        );
        unsafe {
            (*self.scheduler).schedule_event(time, routed)?;
        }
        Ok(())
    }

    #[inline(always)]
    pub fn push_event_with_source<M: Message>(
        &self,
        event: Event<M>,
        target: Option<ConnectorPath<'s>>,
        source_connector: CowStr<'s>,
    ) -> Result<(), SimulationError> {
        self.push_event_with_time_and_source(event, target, self.time, source_connector)
    }
}

pub struct ConnectorCtx<'c, 's: 'c> {
    sim: &'c SimulationCtx<'s>,
    pub on_connector: CowStr<'s>,
}

impl<'c, 's: 'c> ConnectorCtx<'c, 's> {
    #[inline(always)]
    pub fn push_event_with_time<M: Message>(
        &self,
        event: Event<M>,
        target: Option<ConnectorPath<'s>>,
        time: impl Into<SimulationTime>,
    ) -> Result<(), SimulationError> {
        self.sim
            .push_event_with_time_and_source(event, target, time, self.on_connector.clone())
    }

    #[inline(always)]
    pub fn push_event<M: Message>(
        &self,
        event: Event<M>,
        target: Option<ConnectorPath<'s>>,
    ) -> Result<(), SimulationError> {
        self.sim
            .push_event_with_source(event, target, self.on_connector.clone())
    }
}

impl<'c, 's: 'c> Deref for ConnectorCtx<'c, 's> {
    type Target = SimulationCtx<'s>;

    fn deref(&self) -> &Self::Target {
        self.sim
    }
}

pub enum Scheduled<'s> {
    Internal(CowStr<'s>),
    Event(RoutedEvent<'s>),
}

pub struct Scheduler<'s> {
    pub time: SimulationTime,
    scheduled: BTreeMap<SimulationTime, Vec<Scheduled<'s>>>,
}

impl<'s> Scheduler<'s> {
    pub fn new(current_time: SimulationTime) -> Self {
        Scheduler {
            time: current_time,
            scheduled: BTreeMap::new(),
        }
    }

    fn schedule(
        &mut self,
        time: SimulationTime,
        value: Scheduled<'s>,
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
    pub fn schedule_change(
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
        event: RoutedEvent<'s>,
    ) -> Result<(), SchedulerError> {
        self.schedule(time.into(), Scheduled::Event(event))
    }

    pub fn get_next_time(&self) -> Option<SimulationTime> {
        self.scheduled.first_key_value().map(|(it, _)| it.clone())
    }
}

impl<'s> Iterator for Scheduler<'s> {
    type Item = Vec<Scheduled<'s>>;

    fn next(&mut self) -> Option<Self::Item> {
        let (time, result) = self.scheduled.pop_first()?;
        self.time = time;
        Some(result)
    }
}
