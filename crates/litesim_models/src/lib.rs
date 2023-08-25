#[cfg(feature = "cloner")]
pub mod cloner;
#[cfg(any(feature = "rand", feature = "generator"))]
pub mod generator;
#[cfg(feature = "queue")]
pub mod queue;
#[cfg(feature = "timer")]
pub mod timer;

pub mod prelude {
    #[cfg(feature = "cloner")]
    pub use crate::cloner::Cloner as ClonerModel;
    #[cfg(all(feature = "rand", not(feature = "generator")))]
    pub use crate::generator::Generator;
    #[cfg(all(feature = "rand", feature = "generator"))]
    pub use crate::generator::Generator as GeneratorModel;
    #[cfg(feature = "queue")]
    pub use crate::queue::Queue as QueueModel;
    #[cfg(feature = "timer")]
    pub use crate::timer::Timer as TimerModel;

    pub use litesim::prelude as litesim;
}
