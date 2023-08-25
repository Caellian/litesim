pub type CowStr<'s> = std::borrow::Cow<'s, str>;

pub trait ToCowStr<'s>: AsRef<str> {
    fn to_cow_str(self) -> CowStr<'s>;
}

impl<'s> ToCowStr<'s> for &'s str {
    fn to_cow_str(self) -> CowStr<'s> {
        CowStr::Borrowed(self)
    }
}

impl<'s> ToCowStr<'s> for CowStr<'s> {
    fn to_cow_str(self) -> CowStr<'s> {
        self.clone()
    }
}

impl<'s> ToCowStr<'s> for String {
    fn to_cow_str(self) -> CowStr<'s> {
        CowStr::Owned(self)
    }
}

#[cfg(feature = "rand")]
mod rng {
    pub trait SimulationRng: rand_core::RngCore + 'static {}
    impl<T: rand_core::RngCore + 'static> SimulationRng for T {}
}
#[cfg(feature = "rand")]
pub use rng::*;

/// Re-exported const TypeId constructor so dependants don't need to enable const_type_id
/// flag.
pub const fn const_type_id<T: 'static>() -> std::any::TypeId {
    std::any::TypeId::of::<T>()
}
