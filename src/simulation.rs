use std::{
    collections::{BTreeMap, HashMap},
    pin::Pin,
};

#[cfg(feature = "rand")]
mod rand_imports {
    pub use std::cell::RefCell;
    pub use std::rc::Rc;

    pub use rand::Rng;

    pub use crate::util::SimulationRng;
}
#[cfg(feature = "rand")]
use rand_imports::*;

use crate::{
    error::{RoutingError, SchedulerError, SimulationError},
    event::{Event, Message},
    model::ModelImpl,
    prelude::{BorrowedModel, ErasedEvent, TimeBounds},
    routes::{ConnectorPath, EventSource, Route},
    system::{AdjacentModels, SystemModel},
    time::{Time, TimeTrigger},
    util::{CowStr, ToCowStr},
};

#[allow(dead_code)]
pub struct Simulation<'s> {
    #[cfg(feature = "rand")]
    global_rng: Rc<RefCell<dyn SimulationRng>>,
    system: Pin<Box<SystemModel<'s>>>,
    initial_time: Time,
    scheduler: Pin<Box<Scheduler<'s>>>,
}

impl<'s> Simulation<'s> {
    pub fn new(
        #[cfg(feature = "rand")] rng: impl SimulationRng + 'static,
        mut system: SystemModel<'s>,
        initial_time: impl Into<Time>,
    ) -> Result<Self, SimulationError> {
        system.validate()?;

        #[cfg(feature = "rand")]
        let global_rng = Rc::new(RefCell::new(rng));
        let initial_time = initial_time.into();

        let mut scheduler = Box::pin(Scheduler::new(initial_time));
        for (id, mut model) in system.models.iter() {
            let sim_ref = ModelCtx::new_parameterized(
                &system.route_cache,
                initial_time,
                #[cfg(feature = "rand")]
                global_rng.clone(),
                id.clone(),
                &mut scheduler,
            );

            model.init(sim_ref)?;
        }

        Ok(Simulation {
            #[cfg(feature = "rand")]
            global_rng,
            system: Box::pin(system),
            initial_time,
            scheduler,
        })
    }

    #[inline]
    pub fn schedule_event<M: Message>(
        &mut self,
        time: impl Into<Time>,
        event: Event<M>,
        target: ConnectorPath<'s>,
    ) -> Result<(), SchedulerError> {
        self.scheduler.schedule(
            time.into(),
            Scheduled::Event {
                event: event.into(),
                route: Route {
                    from: EventSource::External,
                    to: target,
                },
            },
        )
    }

    pub fn current_time(&self) -> Time {
        self.scheduler.time
    }

    pub fn route_event(
        &mut self,
        event: ErasedEvent,
        route: Route<'s>,
    ) -> Result<(), SimulationError> {
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
                Scheduled::Event { event, route } => {
                    self.route_event(event, route)?;
                }
            }
        }

        Ok(())
    }

    /// Runs simulation until passed time is reached (inclusive) or the simulated system becomes inert
    pub fn run_until(&mut self, time: impl Into<Time>) -> Result<(), SimulationError> {
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
        self.run_until(Time::MAX)
    }
}

pub struct ModelCtx<'s> {
    pub time: Time,
    #[cfg(feature = "rand")]
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
            #[cfg(feature = "rand")]
            rng: simulation.global_rng.clone(),
            model_id: model,
            routes,
            scheduler,
        }
    }

    fn new_parameterized(
        route_cache: &HashMap<CowStr<'s>, AdjacentModels<'s>>,
        time: Time,
        #[cfg(feature = "rand")] rng: Rc<RefCell<dyn SimulationRng>>,
        model: CowStr<'s>,
        scheduler: &mut Pin<Box<Scheduler<'s>>>,
    ) -> Self {
        let routes = route_cache.get(model.as_ref()).cloned().unwrap_or_default();

        let scheduler: *mut Pin<Box<Scheduler<'s>>> = scheduler;

        ModelCtx {
            time,
            #[cfg(feature = "rand")]
            rng,
            model_id: model,
            routes,
            scheduler,
        }
    }

    pub fn model_id(&self) -> &CowStr<'s> {
        &self.model_id
    }

    #[cfg(feature = "rand")]
    pub fn rand<T>(&self) -> T
    where
        rand::distributions::Standard: rand::prelude::Distribution<T>,
    {
        self.rng.borrow_mut().gen()
    }

    #[cfg(feature = "rand")]
    pub fn rand_range<T, R>(&self, range: R) -> T
    where
        T: rand::distributions::uniform::SampleUniform,
        R: rand::distributions::uniform::SampleRange<T>,
    {
        self.rng.borrow_mut().gen_range(range)
    }

    pub fn cancel_updates(&self) {
        unsafe {
            (*self.scheduler).cancel_updates(self.model_id().clone(), None);
        }
    }

    pub fn cancel_updates_bounded(&self, range: TimeBounds) {
        unsafe {
            (*self.scheduler).cancel_updates(self.model_id().clone(), Some(range));
        }
    }

    pub fn schedule_update(&self, time: TimeTrigger) -> Result<(), SimulationError> {
        unsafe {
            (*self.scheduler)
                .schedule_update(time.to_discrete(self.time), self.model_id().clone())?;
        }
        Ok(())
    }

    pub fn push_event_with_time<M: Message>(
        &self,
        event: Event<M>,
        output_connector: CowStr<'s>,
        time: TimeTrigger,
    ) -> Result<(), SimulationError> {
        let target = match self.routes.adjacent_input(output_connector.clone()) {
            Some(first) => first,
            _ => return Ok(()),
        };

        let from = EventSource::Model(ConnectorPath {
            model: self.model_id().clone(),
            connector: output_connector,
        });

        unsafe {
            (*self.scheduler).schedule_event(
                time.to_discrete(self.time),
                event.erase_message_type(),
                Route { from, to: target },
            )?;
        }
        Ok(())
    }

    #[inline(always)]
    pub fn push_event<M: Message>(
        &self,
        event: Event<M>,
        source_connector: CowStr<'s>,
    ) -> Result<(), SimulationError> {
        self.push_event_with_time(event, source_connector, TimeTrigger::Absolute(self.time))
    }

    pub fn internal_event_with_time<M: Message>(
        &self,
        event: Event<M>,
        target_connector: CowStr<'s>,
        time: TimeTrigger,
    ) -> Result<(), SimulationError> {
        unsafe {
            (*self.scheduler).schedule_event(
                time.to_discrete(self.time),
                event.erase_message_type(),
                Route {
                    from: EventSource::Internal,
                    to: ConnectorPath {
                        model: self.model_id().clone(),
                        connector: target_connector,
                    },
                },
            )?;
        }
        Ok(())
    }

    #[inline(always)]
    pub fn internal_event<M: Message>(
        &self,
        event: Event<M>,
        target_connector: CowStr<'s>,
    ) -> Result<(), SimulationError> {
        self.internal_event_with_time(event, target_connector, TimeTrigger::Absolute(self.time))
    }
}

pub struct ConnectorCtx<'s> {
    pub(crate) model_ctx: ModelCtx<'s>,
    pub(crate) on_model: BorrowedModel<'s>,
}

pub enum Scheduled<'s> {
    Internal(CowStr<'s>),
    Event {
        event: ErasedEvent,
        route: Route<'s>,
    },
}

pub struct Scheduler<'s> {
    pub time: Time,
    scheduled: BTreeMap<Time, Vec<Scheduled<'s>>>,
}

impl<'s> Scheduler<'s> {
    pub fn new(current_time: Time) -> Self {
        Scheduler {
            time: current_time,
            scheduled: BTreeMap::new(),
        }
    }

    fn schedule(&mut self, time: Time, value: Scheduled<'s>) -> Result<(), SchedulerError> {
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

    pub fn cancel_updates(&mut self, model: impl ToCowStr<'s>, bounded: Option<TimeBounds>) {
        let model = model.to_cow_str();

        fn remove_model<'s>(entries: &mut Vec<Scheduled>, find: &str) {
            let mut occurences = vec![];
            for (i, it) in entries.iter().enumerate() {
                match it {
                    Scheduled::Internal(model) if model.as_ref() == find => {
                        occurences.push(i);
                    }
                    _ => {}
                }
            }
            occurences.reverse();
            for i in occurences.into_iter() {
                entries.remove(i);
            }
        }

        if let Some(bounded) = bounded {
            for (time, values) in self.scheduled.iter_mut() {
                if !bounded.includes(time) {
                    break;
                }
                remove_model(values, &model);
            }
        } else {
            for values in self.scheduled.values_mut() {
                remove_model(values, &model);
            }
        }
    }

    #[inline]
    pub fn schedule_update(
        &mut self,
        time: impl Into<Time>,
        model: impl ToCowStr<'s>,
    ) -> Result<(), SchedulerError> {
        self.schedule(time.into(), Scheduled::Internal(model.to_cow_str()))
    }

    #[inline]
    pub fn schedule_event(
        &mut self,
        time: impl Into<Time>,
        event: impl Into<ErasedEvent>,
        route: Route<'s>,
    ) -> Result<(), SchedulerError> {
        self.schedule(
            time.into(),
            Scheduled::Event {
                event: event.into(),
                route,
            },
        )
    }

    pub fn get_next_time(&self) -> Option<Time> {
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
