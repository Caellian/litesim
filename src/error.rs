use thiserror::Error;

use crate::prelude::SimulationTime;

#[derive(Debug, Error)]
pub enum ValidationError {
    #[error("Simulation missing a model with id: {id}")]
    MissingModel { id: String },
    #[error("A model '{model}' is missing connector: {id}")]
    MissingConnector { model: String, id: String },
    #[error("Connection output ({output_model}::{output_connector}) and input ({input_model}::{input_connector}) types do not match")]
    ConnectionTypeMismatch {
        output_model: String,
        output_connector: String,
        input_model: String,
        input_connector: String,
    },
    #[error("Connector '{connector}' takes in a wrong model type")]
    InvalidConnectorModel { connector: &'static str },
    #[error("Model store error: {0}")]
    ModelStore(
        #[from]
        #[source]
        ModelStoreError,
    ),
}

#[derive(Debug, Error)]
pub enum SchedulerError {
    #[error("Tried scheduling an occurence in the past: {insertion}; current time is: {current}")]
    TimeRegression {
        current: SimulationTime,
        insertion: SimulationTime,
    },
}

#[derive(Debug, Error)]
pub enum RoutingError {
    #[error("Connector got invalid event type: {event_type}; expected: {expected}")]
    InvalidEventType {
        event_type: &'static str,
        expected: &'static str,
    },
    #[error("Connector handler expected model to of type {expected}; got something else")]
    InvalidModelType { expected: &'static str },
    #[error("Called apply_event with context without a model")]
    MissingModel,
    #[error("Model '{model}' doesn't have an input connector named '{connector}'")]
    UnknownModelConnector { model: String, connector: String },
    #[error("Event generated by {model} is missing a target")]
    MissingEventTarget { model: String },
}

#[derive(Debug, Error)]
pub enum ModelStoreError {
    #[error("Tried taking a model from an empty slot")]
    ModelMissing,
    #[error("Tried returning a model into an occupied slot")]
    SlotOccupied,
}

#[derive(Debug, Error)]
pub enum SimulationError {
    #[error("Unable to locate model: {id}")]
    ModelNotFound { id: String },

    #[error("Scheduler error: {0}")]
    Scheduler(
        #[from]
        #[source]
        SchedulerError,
    ),
    #[error("Unable to validate model: {0}")]
    Validation(
        #[from]
        #[source]
        ValidationError,
    ),
    #[error("Routing error: {0}")]
    Routing(
        #[from]
        #[source]
        RoutingError,
    ),
    #[error("Model store error: {0}")]
    ModelStore(
        #[from]
        #[source]
        ModelStoreError,
    ),
    #[error(transparent)]
    Other(Box<dyn std::error::Error>),
}
