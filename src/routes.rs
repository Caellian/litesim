use std::{any::TypeId, fmt::Debug};

use crate::util::CowStr;

#[derive(Clone, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct ConnectorPath<'s> {
    pub model: CowStr<'s>,
    pub connector: CowStr<'s>,
}

impl<'s> ConnectorPath<'s> {
    pub fn new(model: impl AsRef<str>, connector: impl AsRef<str>) -> Self {
        Self {
            model: CowStr::Owned(model.as_ref().to_string()),
            connector: CowStr::Owned(connector.as_ref().to_string()),
        }
    }
}

impl Debug for ConnectorPath<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}::{}", self.model, self.connector)
    }
}

#[macro_export]
macro_rules! connection {
    ($model:tt :: $connector:tt) => {
        ::litesim::routes::ConnectorPath {
            model: std::borrow::Cow::Borrowed(stringify!($model)),
            connector: std::borrow::Cow::Borrowed(stringify!($connector)),
        }
    };
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EventSource<'s> {
    External,
    Internal,
    Model(ConnectorPath<'s>),
}

#[derive(Clone, PartialEq, Eq)]
pub struct Route<'s> {
    pub from: EventSource<'s>,
    pub to: ConnectorPath<'s>,
}

impl<'s> Route<'s> {
    pub fn new(from: ConnectorPath<'s>, to: ConnectorPath<'s>) -> Self {
        Route {
            from: EventSource::Model(from),
            to,
        }
    }

    pub fn new_external(target: ConnectorPath<'s>) -> Self {
        Route {
            from: EventSource::External,
            to: target,
        }
    }

    pub fn new_internal(target: ConnectorPath<'s>) -> Self {
        Route {
            from: EventSource::Internal,
            to: target,
        }
    }

    pub fn starts_in_model(&self, id: impl AsRef<str>) -> bool {
        match &self.from {
            EventSource::External => false,
            EventSource::Internal => true,
            EventSource::Model(ConnectorPath {
                model: model_id, ..
            }) => model_id.as_ref() == id.as_ref(),
        }
    }

    pub fn ends_in_model(&self, id: impl AsRef<str>) -> bool {
        self.to.model.as_ref() == id.as_ref()
    }

    pub fn from_connection(&self) -> Option<ConnectorPath<'s>> {
        match &self.from {
            EventSource::External => None,
            EventSource::Internal => None,
            EventSource::Model(connection) => Some(connection.clone()),
        }
    }

    pub fn to_connection(&self) -> ConnectorPath<'s> {
        self.to.clone()
    }
}

impl<'s> From<(ConnectorPath<'s>, ConnectorPath<'s>)> for Route<'s> {
    fn from(value: (ConnectorPath<'s>, ConnectorPath<'s>)) -> Self {
        Route {
            from: EventSource::Model(value.0),
            to: value.1,
        }
    }
}

impl<'s> From<(&ConnectorPath<'s>, &ConnectorPath<'s>)> for Route<'s> {
    fn from(value: (&ConnectorPath<'s>, &ConnectorPath<'s>)) -> Self {
        Route {
            from: EventSource::Model(value.0.clone()),
            to: value.1.clone(),
        }
    }
}

pub struct OutputConnectorInfo(pub(crate) &'static str, pub(crate) TypeId);

impl OutputConnectorInfo {
    pub const fn new<T: 'static>(id: &'static str) -> Self {
        OutputConnectorInfo(id, TypeId::of::<T>())
    }
}
