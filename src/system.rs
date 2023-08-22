use std::{
    cell::RefCell,
    collections::HashMap,
    mem::MaybeUninit,
    ops::{Deref, DerefMut},
    pin::Pin,
};

use crate::{
    error::ValidationError,
    model::{ConnectorPath, Model, Route},
    prelude::ModelImpl,
    util::CowStr,
};

pub(crate) type IdStore<'s, Value> = HashMap<CowStr<'s>, Value>;
pub(crate) type ModelStore<'s> = IdStore<'s, Box<dyn Model<'s>>>;

pub struct SystemModel<'s> {
    pub models: ModelStore<'s>,
    pub routes: HashMap<ConnectorPath<'s>, ConnectorPath<'s>>,
    pub validated: RefCell<bool>,
    pub(crate) route_cache: RefCell<IdStore<'s, AdjacentModels<'s>>>,
}

impl<'s> Default for SystemModel<'s> {
    fn default() -> Self {
        SystemModel::new()
    }
}

impl<'s> SystemModel<'s> {
    pub fn new() -> Self {
        Self {
            models: HashMap::new(),
            routes: HashMap::new(),
            validated: RefCell::new(false),
            route_cache: RefCell::new(IdStore::new()),
        }
    }

    pub fn push_model(&mut self, id: impl AsRef<str>, model: impl Model<'s> + 'static) {
        self.models
            .insert(CowStr::Owned(id.as_ref().to_string()), Box::new(model));
        *self.validated.borrow_mut() = false;
    }

    pub fn push_route(&mut self, route: Route<'s>) {
        match route.from {
            crate::prelude::EventSource::External => {
                panic!("system model route can't contain external references")
            }
            crate::prelude::EventSource::Model(from) => {
                self.routes.insert(from, route.to);
            }
        }
        *self.validated.borrow_mut() = false;
    }

    pub fn routes<'a>(&'a self) -> impl Iterator<Item = Route<'s>> + 'a {
        self.routes.iter().map(Route::from)
    }

    pub fn validate(&self) -> Result<(), ValidationError> {
        if *self.validated.borrow() == true {
            return Ok(());
        }

        for (a, b) in self.routes.iter() {
            let model_a =
                self.models
                    .get(a.model.as_ref())
                    .ok_or_else(|| ValidationError::MissingModel {
                        id: a.model.to_string(),
                    })?;

            let model_b =
                self.models
                    .get(b.model.as_ref())
                    .ok_or_else(|| ValidationError::MissingModel {
                        id: b.model.to_string(),
                    })?;

            let output_type = model_a
                .output_type_id(a.connector.to_string())
                .ok_or_else(|| ValidationError::MissingConnector {
                    model: a.model.to_string(),
                    id: a.connector.to_string(),
                })?;

            let input_type = model_b.input_type_id(b.connector.as_ref()).ok_or_else(|| {
                ValidationError::MissingConnector {
                    model: b.model.to_string(),
                    id: b.connector.to_string(),
                }
            })?;

            if input_type != output_type {
                return Err(ValidationError::ConnectionTypeMismatch {
                    output_model: a.model.to_string(),
                    output_connector: a.connector.to_string(),
                    input_model: b.model.to_string(),
                    input_connector: b.connector.to_string(),
                });
            }
        }

        *self.validated.borrow_mut() = true;

        self.cache_connections();

        Ok(())
    }

    fn cache_connections(&self) {
        let mut cache = self.route_cache.borrow_mut();
        cache.clear();

        for id in self.models.keys() {
            let mut inputs = vec![];
            let mut outputs = vec![];

            for route in self.routes() {
                if route.ends_in_model(&id) {
                    inputs.push(route.clone());
                } else if route.starts_in_model(&id) {
                    outputs.push(route.clone());
                }
            }
            cache.insert(id.clone(), AdjacentModels { inputs, outputs });
        }
    }
}

#[derive(Clone)]
pub struct AdjacentModels<'s> {
    pub inputs: Vec<Route<'s>>,
    pub outputs: Vec<Route<'s>>,
}

impl<'s> Default for AdjacentModels<'s> {
    fn default() -> Self {
        AdjacentModels {
            inputs: vec![],
            outputs: vec![],
        }
    }
}

pub(crate) struct BorrowedModel<'s> {
    owner: *mut Pin<Box<SystemModel<'s>>>,
    id: CowStr<'s>,
    model: MaybeUninit<Box<dyn Model<'s>>>,
}

impl<'s> BorrowedModel<'s> {
    pub fn new(owner: &mut Pin<Box<SystemModel<'s>>>, id: CowStr<'s>) -> Option<Self> {
        let model = MaybeUninit::new(owner.as_mut().models.remove(id.as_ref())?);

        Some(BorrowedModel { owner, id, model })
    }
}

impl<'s> Drop for BorrowedModel<'s> {
    fn drop(&mut self) {
        unsafe {
            let mut model = MaybeUninit::uninit();
            std::mem::swap(&mut model, &mut self.model);
            (*self.owner)
                .models
                .insert(self.id.clone(), model.assume_init());
        }
    }
}

impl<'s> Deref for BorrowedModel<'s> {
    type Target = dyn Model<'s>;

    fn deref(&self) -> &Self::Target {
        unsafe { self.model.assume_init_ref().as_ref() }
    }
}

impl DerefMut for BorrowedModel<'_> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        unsafe { self.model.assume_init_mut().as_mut() }
    }
}
