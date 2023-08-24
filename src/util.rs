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

pub trait SimulationRng: rand_core::RngCore {}
impl<T: rand_core::RngCore> SimulationRng for T {}

/// Re-exported const TypeId constructor so dependants don't need to enable const_type_id
/// flag.
pub const fn const_type_id<T: 'static>() -> std::any::TypeId {
    std::any::TypeId::of::<T>()
}
