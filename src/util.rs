pub type CowStr<'s> = std::borrow::Cow<'s, str>;

pub trait ToCowStr<'s> {
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

pub auto trait NotUnit {}
impl !NotUnit for () {}

pub trait HeterogeneousTuple {
    const SIZE: usize;

    fn len(&self) -> usize {
        Self::SIZE
    }
}

macro_rules! impl_heterogeneous_tuple {
    ($($size: literal => [$($n: literal : $sym: tt),*]),+) => {
        $(
        impl<$($sym : NotUnit),*> HeterogeneousTuple for ($($sym),*) {
            default const SIZE: usize = $size;
        }

        paste::paste!{
            pub trait [<HeterogeneousTuple $size>]<$($sym : NotUnit),*> {
                fn type_id_of_const<const N: usize>() -> std::any::TypeId;
                fn type_id_of(arg_i: usize) -> std::any::TypeId;
            }
            #[allow(unused_parens)]
            impl<$($sym : NotUnit + 'static),*> [<HeterogeneousTuple $size>]<$($sym),*> for ($($sym),*) {
                fn type_id_of_const<const N: usize>() -> std::any::TypeId {
                    match N {
                        $($n => std::any::TypeId::of::<$sym>(),)*
                        _ => unreachable!("Invalid type index"),
                    }
                }
                fn type_id_of(arg_i: usize) -> std::any::TypeId {
                    match arg_i {
                        $($n => std::any::TypeId::of::<$sym>(),)*
                        _ => unreachable!("Invalid type index"),
                    }
                }
            }
        }
    )+
    };
}

impl<A: NotUnit> HeterogeneousTuple for (A,) {
    default const SIZE: usize = 1;
}
pub trait HeterogeneousTuple1<A: NotUnit> {
    fn type_id_of_const<const N: usize>() -> std::any::TypeId;

    fn type_id_of(arg_i: usize) -> std::any::TypeId;
}

impl<A: NotUnit + 'static> HeterogeneousTuple1<A> for (A,) {
    fn type_id_of_const<const N: usize>() -> std::any::TypeId {
        match N {
            0 => std::any::TypeId::of::<A>(),
            _ => unreachable!("Invalid type index"),
        }
    }
    fn type_id_of(arg_i: usize) -> std::any::TypeId {
        match arg_i {
            0 => std::any::TypeId::of::<A>(),
            _ => unreachable!("Invalid type index"),
        }
    }
}

impl_heterogeneous_tuple![
    0 => [],
    2 => [0: A, 1: B],
    3 => [0: A, 1: B, 2: C],
    4 => [0: A, 1: B, 2: C, 3: D],
    5 => [0: A, 1: B, 2: C, 3: D, 4: E],
    6 => [0: A, 1: B, 2: C, 3: D, 4: E, 5: F],
    7 => [0: A, 1: B, 2: C, 3: D, 4: E, 5: F, 6: G],
    8 => [0: A, 1: B, 2: C, 3: D, 4: E, 5: F, 6: G, 7: H],
    9 => [0: A, 1: B, 2: C, 3: D, 4: E, 5: F, 6: G, 7: H, 8: I],
    10 => [0: A, 1: B, 2: C, 3: D, 4: E, 5: F, 6: G, 7: H, 8: I, 9: J],
    11 => [0: A, 1: B, 2: C, 3: D, 4: E, 5: F, 6: G, 7: H, 8: I, 9: J, 10: K],
    12 => [0: A, 1: B, 2: C, 3: D, 4: E, 5: F, 6: G, 7: H, 8: I, 9: J, 10: K, 11: L]
];

pub trait EraseTyping<Erased> {
    fn erase_typing(self) -> Erased;
}

/*
MAPT<Vararg, (ABC) => ((X, A), (X, B))>

- happens at compile time, not codegen time
- We need to know that I is in fact a tuple, and iterate over its types which isn't something we can do with macros
  - I needs to be partially evaluated by rustc

fn derp<>
 */