use std::{
    cell::RefCell,
    collections::{BTreeMap, HashMap},
    pin::Pin,
    rc::Rc,
};

use rand::Rng;

use crate::{
    error::{RoutingError, SchedulerError, SimulationError},
    event::{Event, Message, RoutedEvent},
    model::{ConnectorPath, EventSource, ModelImpl, Route},
    prelude::BorrowedModel,
    system::{AdjacentModels, SystemModel},
    time::{SimulationTime, TimeTrigger},
    util::{CowStr, SimulationRng, ToCowStr},
};

#[allow(dead_code)]
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
        for (id, mut model) in system.models.iter() {
            let sim_ref = ModelCtx::new_parameterized(
                &system.route_cache,
                initial_time,
                global_rng.clone(),
                id.clone(),
                &mut scheduler,
            );

            model.init(sim_ref)?;
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

    pub fn route_event(&mut self, event: RoutedEvent<'s>) -> Result<(), SimulationError> {
        let RoutedEvent { event, route } = event;
        let target_model = route.to.model.clone();
        let target_connector = route.to.connector.clone();

        let model = self.system.models.borrow(target_model.clone())?.ok_or(
            SimulationError::ModelNotFound {
                id: target_model.to_string(),
            },
        )?;

        let handler = model
            .get_input_handler_by_name(target_connector.as_ref())
            .ok_or_else(|| RoutingError::UnknownModelConnector {
                model: target_model.to_string(),
                connector: target_connector.to_string(),
            })?;

        let state = ConnectorCtx {
            model_ctx: ModelCtx::new(self, target_model),
            on_model: model,
        };

        handler.apply_event(event, state)?;
        Ok(())
    }

    pub fn step(&mut self) -> Result<(), SimulationError> {
        let scheduled = match self.scheduler.next() {
            Some(it) => it,
            None => return Ok(()),
        };

        for entry in scheduled {
            match entry {
                Scheduled::Internal(model_id) => {
                    let mut model = self.system.models.borrow(model_id.clone())?.ok_or(
                        SimulationError::ModelNotFound {
                            id: model_id.to_string(),
                        },
                    )?;

                    let state = ModelCtx::new(self, model_id);

                    model.handle_update(state)?;
                }
                Scheduled::Event(event) => {
                    self.route_event(event)?;
                }
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

            self.step()?;
        }

        Ok(())
    }

    /// Runs simulation until the simulated system becomes inert
    pub fn run(&mut self) -> Result<(), SimulationError> {
        self.run_until(SimulationTime::MAX)
    }
}

pub struct ModelCtx<'s> {
    pub time: SimulationTime,
    pub rng: Rc<RefCell<dyn SimulationRng>>,
    pub model_id: CowStr<'s>,
    pub routes: AdjacentModels<'s>,
    pub scheduler: *mut Pin<Box<Scheduler<'s>>>,
}

impl<'s> ModelCtx<'s> {
    pub fn new(simulation: &mut Simulation<'s>, model: CowStr<'s>) -> Self {
        let routes = simulation
            .system
            .route_cache
            .get(model.as_ref())
            .cloned()
            .unwrap_or_default();

        let scheduler: *mut Pin<Box<Scheduler<'s>>> = &mut simulation.scheduler;

        ModelCtx {
            time: simulation.current_time(),
            rng: simulation.global_rng.clone(),
            model_id: model,
            routes,
            scheduler,
        }
    }

    fn new_parameterized(
        route_cache: &HashMap<CowStr<'s>, AdjacentModels<'s>>,
        time: SimulationTime,
        rng: Rc<RefCell<dyn SimulationRng>>,
        model: CowStr<'s>,
        scheduler: &mut Pin<Box<Scheduler<'s>>>,
    ) -> Self {
        let routes = route_cache.get(model.as_ref()).cloned().unwrap_or_default();

        let scheduler: *mut Pin<Box<Scheduler<'s>>> = scheduler;

        ModelCtx {
            time,
            rng,
            model_id: model,
            routes,
            scheduler,
        }
    }

    pub fn model_id(&self) -> &CowStr<'s> {
        &self.model_id
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
                .schedule_change(time.to_discrete(self.time), self.model_id().clone())?;
        }
        Ok(())
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
                model: self.model_id().to_string(),
            })?;
        unsafe {
            let routed = RoutedEvent::new(
                event.erase_message_type(),
                Route {
                    from: EventSource::Model(ConnectorPath {
                        model: self.model_id().clone(),
                        connector: source_connector,
                    }),
                    to: target,
                },
            );

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

pub struct ConnectorCtx<'s> {
    pub(crate) model_ctx: ModelCtx<'s>,
    pub(crate) on_model: BorrowedModel<'s>,
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
        model: impl ToCowStr<'s>,
    ) -> Result<(), SchedulerError> {
        self.schedule(time.into(), Scheduled::Internal(model.to_cow_str()))
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
