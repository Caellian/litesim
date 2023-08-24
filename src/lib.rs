#![allow(incomplete_features)]
#![feature(const_type_id, box_into_inner)]

pub mod error;
pub mod event;
pub mod model;
pub mod routes;
pub mod simulation;
pub mod system;
pub mod time;

pub(crate) mod util;

pub mod prelude {
    pub use crate::error::*;
    pub use crate::event::*;
    pub use crate::model::*;
    pub use crate::routes::*;
    pub use crate::simulation::*;
    pub use crate::system::*;
    pub use crate::time::TimeTrigger::*;
    pub use crate::time::*;
    pub use crate::util::const_type_id;
    pub use crate::util::SimulationRng;

    pub use crate::connection;
    pub use crate::push_event;
    pub use crate::route;

    pub use litesim_macros::litesim_model;
}
