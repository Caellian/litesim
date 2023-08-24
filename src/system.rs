use std::{
    any::TypeId,
    collections::HashMap,
    ops::{Deref, DerefMut},
};

use crate::{
    error::ValidationError,
    model::{ConnectorPath, Model, Route},
    prelude::{ModelImpl, ModelStoreError},
    util::{CowStr, ToCowStr},
};

pub(crate) type IdStore<'s, Value> = HashMap<CowStr<'s>, Value>;
pub struct SystemModel<'s> {
    pub models: ModelStore<'s>,
    pub routes: HashMap<ConnectorPath<'s>, ConnectorPath<'s>>,
    pub validated: bool,
    pub(crate) route_cache: IdStore<'s, AdjacentModels<'s>>,
}

impl<'s> Default for SystemModel<'s> {
    fn default() -> Self {
        SystemModel::new()
    }
}

impl<'s> SystemModel<'s> {
    pub fn new() -> Self {
        Self {
            models: ModelStore::new(),
            routes: HashMap::new(),
            validated: false,
            route_cache: IdStore::new(),
        }
    }

    pub fn push_model(&mut self, id: impl ToString, model: impl Model<'s> + 'static) {
        self.models.insert(id, model);
        self.validated = false;
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
        self.validated = false;
    }

    pub fn routes<'a>(&'a self) -> impl Iterator<Item = Route<'s>> + 'a {
        self.routes.iter().map(Route::from)
    }

    pub fn validate(&mut self) -> Result<(), ValidationError> {
        if self.validated == true {
            return Ok(());
        }

        for (a, b) in self.routes.iter() {
            let model_a = self.models.borrow(a.model.clone())?.ok_or_else(|| {
                ValidationError::MissingModel {
                    id: a.model.to_string(),
                }
            })?;

            let model_b = self.models.borrow(b.model.clone())?.ok_or_else(|| {
                ValidationError::MissingModel {
                    id: b.model.to_string(),
                }
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

            let non_matching = (0..model_b.input_connectors().len())
                .filter_map(|i| model_b.get_input_handler(i).map(|h| (i, h)))
                .map(|(i, handler)| (i, handler.model_type_id()))
                .find(|(_, id)| *id != model_b.type_id());

            if let Some((found_i, _)) = non_matching {
                return Err(ValidationError::InvalidConnectorModel {
                    connector: model_b.input_connectors()[found_i],
                });
            }
        }

        self.validated = true;

        self.cache_connections();

        Ok(())
    }

    fn cache_connections(&mut self) {
        self.route_cache.clear();

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
            self.route_cache
                .insert(id.clone(), AdjacentModels { inputs, outputs });
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

pub struct ModelSlot<'s> {
    value: Box<dyn Model<'s>>,
    taken: bool,
    // mutex: Mutex<()>,
}

impl<'s> ModelSlot<'s> {
    pub(crate) fn new(value: impl Model<'s> + 'static) -> Self {
        Self {
            value: Box::new(value),
            taken: false,
        }
    }

    pub(crate) unsafe fn data_ptr(&self) -> *const dyn Model<'s> {
        let result: *const dyn Model<'s> = &*self.value;
        result
    }

    pub(crate) unsafe fn data_ptr_mut(&mut self) -> *mut dyn Model<'s> {
        let result: *mut dyn Model<'s> = &mut *self.value;
        result
    }

    pub(crate) fn take(&mut self) -> Result<*mut dyn Model<'s>, ModelStoreError> {
        if self.taken {
            return Err(ModelStoreError::ModelMissing);
        }
        self.taken = true;
        Ok(unsafe { self.data_ptr_mut() })
    }

    pub(crate) fn release(&mut self) -> Result<(), ModelStoreError> {
        if !self.taken {
            return Err(ModelStoreError::SlotOccupied);
        }
        self.taken = false;
        Ok(())
    }
}

pub struct ModelStore<'s> {
    data: HashMap<CowStr<'s>, ModelSlot<'s>>,
}

impl<'s> ModelStore<'s> {
    pub fn new() -> Self {
        Self {
            data: HashMap::new(),
        }
    }

    pub fn insert(&mut self, id: impl ToString, model: impl Model<'s> + 'static) {
        self.data
            .insert(CowStr::Owned(id.to_string()), ModelSlot::new(model));
    }

    pub fn get(&mut self, id: impl AsRef<str>) -> Option<&dyn Model<'s>> {
        let slot = match self.data.get_mut(id.as_ref()) {
            Some(it) => it,
            None => return None,
        };
        if !slot.taken {
            Some(unsafe { &*slot.data_ptr() })
        } else {
            None
        }
    }

    pub fn get_i(&mut self, index: usize) -> Option<&dyn Model<'s>> {
        let name = match self.data.keys().nth(index) {
            Some(it) => it,
            None => return None,
        }
        .clone();
        self.get(name)
    }

    pub fn borrow(
        &mut self,
        id: impl ToCowStr<'s>,
    ) -> Result<Option<BorrowedModel<'s>>, ModelStoreError> {
        let slot = match self.data.get_mut(id.as_ref()) {
            Some(it) => it,
            None => return Ok(None),
        };
        let slot_ptr: *mut ModelSlot<'s> = slot;

        Ok(Some(BorrowedModel::new(slot_ptr, id.to_cow_str())?))
    }

    pub fn borrow_i(&mut self, index: usize) -> Result<Option<BorrowedModel<'s>>, ModelStoreError> {
        let name = match self.data.keys().nth(index) {
            Some(it) => it,
            None => return Ok(None),
        }
        .clone();
        self.borrow(name)
    }

    pub fn keys(&self) -> impl Iterator<Item = &CowStr<'s>> + '_ {
        self.data.keys()
    }

    pub fn iter(&mut self) -> ModelStoreIter<'_, 's> {
        ModelStoreIter {
            store: self,
            pos: 0,
        }
    }
}

pub struct ModelStoreIter<'i, 's: 'i> {
    store: &'i mut ModelStore<'s>,
    pos: usize,
}

impl<'i, 's: 'i> Iterator for ModelStoreIter<'i, 's> {
    type Item = (CowStr<'s>, BorrowedModel<'s>);

    fn next(&mut self) -> Option<Self::Item> {
        let key: CowStr<'s> = self.store.data.keys().nth(self.pos)?.clone();
        let value = match self.store.borrow(key.clone()) {
            Ok(value) => value?,
            Err(ModelStoreError::ModelMissing) => {
                self.pos += 1;
                return self.next();
            }
            _ => return None,
        };
        self.pos += 1;
        Some((key, value))
    }
}

pub struct BorrowedModel<'s> {
    owner: *mut ModelSlot<'s>,
    id: CowStr<'s>,
    model: *mut dyn Model<'s>,
}

impl<'s> BorrowedModel<'s> {
    pub fn new(owner: *mut ModelSlot<'s>, id: impl ToCowStr<'s>) -> Result<Self, ModelStoreError> {
        let model = unsafe { (*owner).take()? };

        Ok(Self {
            owner,
            id: id.to_cow_str(),
            model,
        })
    }

    pub fn id(&self) -> &CowStr<'s> {
        &self.id
    }

    pub unsafe fn cast<M: Model<'s> + 'static>(&self) -> Option<&M> {
        if (*self.model).type_id() == TypeId::of::<M>() {
            let val: *mut M = self.model as *mut M;
            Some(&*val)
        } else {
            None
        }
    }

    pub unsafe fn cast_mut<M: Model<'s> + 'static>(&mut self) -> Option<&mut M> {
        if (*self.model).type_id() == TypeId::of::<M>() {
            let val: *mut M = self.model as *mut M;
            Some(&mut *val)
        } else {
            None
        }
    }
}

impl<'s> Drop for BorrowedModel<'s> {
    fn drop(&mut self) {
        unsafe {
            if !self.owner.is_null() {
                (*self.owner)
                    .release()
                    .expect("BorrowedModel released a slot that wasn't taken");
            }
        }
    }
}

impl<'s> Deref for BorrowedModel<'s> {
    type Target = dyn Model<'s>;

    fn deref(&self) -> &Self::Target {
        unsafe { &*self.model }
    }
}

impl DerefMut for BorrowedModel<'_> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        unsafe { &mut *self.model }
    }
}
