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
    pub use crate::event::*;
    pub use crate::model::*;
    pub use crate::routes::*;
    pub use crate::simulation::*;
    pub use crate::system::*;

    pub use crate::time::TimeTrigger::Now;
    pub use crate::time::*;

    pub use crate::error::*;
    pub use crate::util::const_type_id;
    #[cfg(feature = "rand")]
    pub use crate::util::SimulationRng;

    // macros
    pub use crate::connection;
    pub use litesim_macros::input_handler;
    pub use litesim_macros::litesim_model;
}
