use std::{
    fmt::Display,
    ops::{Bound, RangeBounds},
};

#[cfg(feature = "time_f32")]
type TimeRepr = f32;
#[cfg(feature = "time_f64")]
type TimeRepr = f64;
#[cfg(feature = "time_chrono")]
type TimeRepr = chrono::NaiveDateTime;

#[derive(Debug, Default, Clone, Copy, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[repr(transparent)]
pub struct Time(TimeRepr);

impl Time {
    #[cfg(feature = "time_f32")]
    pub const MIN: Time = Self(0.0f32);
    #[cfg(feature = "time_f64")]
    pub const MIN: Time = Self(0.0f64);
    #[cfg(feature = "time_chrono")]
    pub const MIN: Time = Self(chrono::NaiveDateTime::MIN);

    #[cfg(feature = "time_f32")]
    pub const MAX: Time = Self(f32::MAX);
    #[cfg(feature = "time_f64")]
    pub const MAX: Time = Self(f64::MAX);
    #[cfg(feature = "time_chrono")]
    pub const MAX: Time = Self(chrono::NaiveDateTime::MAX);

    pub const fn new(value: TimeRepr) -> Self {
        Self(value)
    }

    pub fn into_repr(self) -> TimeRepr {
        self.0
    }
}

impl PartialOrd for Time {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        #[cfg(any(feature = "time_f32", feature = "time_f64"))]
        {
            return float_ord::FloatOrd(self.0).partial_cmp(&float_ord::FloatOrd(other.0));
        }
        #[cfg(feature = "time_chrono")]
        {
            return self.0.partial_cmp(&other.0);
        }
    }
}

impl Eq for Time {}

impl Ord for Time {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        #[cfg(any(feature = "time_f32", feature = "time_f64"))]
        {
            return float_ord::FloatOrd(self.0).cmp(&float_ord::FloatOrd(other.0));
        }
        #[cfg(feature = "time_chrono")]
        {
            return self.0.cmp(&other.0);
        }
    }
}

impl From<TimeRepr> for Time {
    fn from(value: TimeRepr) -> Self {
        Self::new(value)
    }
}
impl Into<TimeRepr> for Time {
    fn into(self) -> TimeRepr {
        self.0
    }
}

#[cfg(feature = "time_f32")]
type DurationRepr = f32;
#[cfg(feature = "time_f64")]
type DurationRepr = f64;
#[cfg(feature = "time_chrono")]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub struct DurationRepr {
    secs: i64,
    nanos: i32,
}

#[cfg(feature = "time_chrono")]
impl Display for DurationRepr {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        Into::<chrono::Duration>::into(*self).fmt(f)
    }
}

#[cfg(feature = "time_chrono")]
impl From<chrono::Duration> for DurationRepr {
    fn from(value: chrono::Duration) -> Self {
        unsafe { std::mem::transmute(value) }
    }
}

#[cfg(feature = "time_chrono")]
impl Into<chrono::Duration> for DurationRepr {
    fn into(self) -> chrono::Duration {
        unsafe { std::mem::transmute(self) }
    }
}

#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[derive(Debug, Copy, Clone)]
pub struct TimeDelta(DurationRepr);

#[cfg(feature = "time_chrono")]
const NANOS_IN_SEC: i32 = 1_000_000_000;

impl TimeDelta {
    #[cfg(feature = "time_f32")]
    pub const MIN: Self = Self(0.0f32);
    #[cfg(feature = "time_f64")]
    pub const MIN: Self = Self(0.0f64);
    #[cfg(feature = "time_chrono")]
    pub const MIN: Self = unsafe { std::mem::transmute((0i64, 0i32)) };

    #[cfg(feature = "time_f32")]
    pub const MAX: Self = Self(f32::MAX);
    #[cfg(feature = "time_f64")]
    pub const MAX: Self = Self(f64::MAX);
    #[cfg(feature = "time_chrono")]
    pub const MAX: Self = TimeDelta(DurationRepr {
        secs: i64::MAX,
        nanos: NANOS_IN_SEC - 1,
    });

    #[cfg(feature = "time_f32")]
    pub const EPSILON: Self = Self(f32::EPSILON);
    #[cfg(feature = "time_f64")]
    pub const EPSILON: Self = Self(f64::EPSILON);
    #[cfg(feature = "time_chrono")]
    pub const EPSILON: Self = TimeDelta(DurationRepr { secs: 0, nanos: 1 });

    #[cfg(any(feature = "time_f32", feature = "time_f64"))]
    pub fn new(value: DurationRepr) -> Self {
        Self(value)
    }
    #[cfg(feature = "time_chrono")]
    pub fn new(value: chrono::Duration) -> Self {
        Self(value.into())
    }

    #[cfg(any(feature = "time_f32", feature = "time_f64"))]
    pub fn into_repr(self) -> DurationRepr {
        self.0
    }

    #[cfg(feature = "time_chrono")]
    pub fn into_repr(self) -> chrono::Duration {
        self.0.into()
    }

    #[cfg(feature = "time_chrono")]
    pub fn seconds(self) -> i64 {
        self.0.secs
    }

    #[cfg(feature = "time_chrono")]
    pub fn nanoseconds(self) -> i32 {
        self.0.nanos
    }
}

#[cfg(any(feature = "time_f32", feature = "time_f64"))]
impl From<DurationRepr> for TimeDelta {
    fn from(value: DurationRepr) -> Self {
        TimeDelta(value)
    }
}
#[cfg(any(feature = "time_f32", feature = "time_f64"))]
impl Into<DurationRepr> for TimeDelta {
    fn into(self) -> DurationRepr {
        self.0
    }
}
#[cfg(feature = "time_chrono")]
impl From<chrono::Duration> for TimeDelta {
    fn from(value: chrono::Duration) -> Self {
        unsafe { std::mem::transmute(value) }
    }
}
#[cfg(feature = "time_chrono")]
impl Into<chrono::Duration> for TimeDelta {
    fn into(self) -> chrono::Duration {
        unsafe { std::mem::transmute(self) }
    }
}

mod op_impl {
    use super::{Time, TimeDelta};
    use std::ops::{Add, AddAssign, Sub, SubAssign};

    impl Add for TimeDelta {
        type Output = TimeDelta;

        fn add(self, rhs: TimeDelta) -> Self::Output {
            Self::new(self.into_repr() + rhs.into_repr())
        }
    }

    impl AddAssign for TimeDelta {
        fn add_assign(&mut self, rhs: TimeDelta) {
            self.0 = (self.into_repr() + rhs.into_repr()).into();
        }
    }

    impl Sub for TimeDelta {
        type Output = TimeDelta;

        fn sub(self, rhs: TimeDelta) -> Self::Output {
            Self::new(self.into_repr() + rhs.into_repr())
        }
    }

    impl SubAssign for TimeDelta {
        fn sub_assign(&mut self, rhs: TimeDelta) {
            self.0 = (self.into_repr() - rhs.into_repr()).into();
        }
    }

    impl Add<TimeDelta> for Time {
        type Output = Time;

        fn add(self, rhs: TimeDelta) -> Self::Output {
            Self::new(self.0 + rhs.into_repr())
        }
    }

    impl AddAssign<TimeDelta> for Time {
        fn add_assign(&mut self, rhs: TimeDelta) {
            self.0 += rhs.into_repr();
        }
    }

    impl Sub<TimeDelta> for Time {
        type Output = Time;

        fn sub(self, rhs: TimeDelta) -> Self::Output {
            Self::new(self.0 + rhs.into_repr())
        }
    }

    impl SubAssign<TimeDelta> for Time {
        fn sub_assign(&mut self, rhs: TimeDelta) {
            self.0 -= rhs.into_repr();
        }
    }

    impl Sub for Time {
        type Output = TimeDelta;

        fn sub(self, rhs: Time) -> Self::Output {
            TimeDelta::new(self.0 - rhs.0)
        }
    }
}
pub use op_impl::*;

#[cfg(feature = "rand")]
mod rand_impl {
    #[cfg(feature = "time_chrono")]
    use rand::distributions::uniform::{SampleUniform, UniformSampler};
    use rand::prelude::Distribution;

    #[cfg(feature = "time_chrono")]
    use super::NANOS_IN_SEC;
    use super::{DurationRepr, TimeDelta};

    #[cfg(feature = "time_chrono")]
    impl SampleUniform for DurationRepr {
        type Sampler = UniformDurationSampler;
    }

    #[cfg(feature = "time_chrono")]
    pub struct UniformDurationSampler {
        low: DurationRepr,
        high: DurationRepr,
        inclusive: bool,
    }

    #[cfg(feature = "time_chrono")]
    impl UniformSampler for UniformDurationSampler {
        type X = DurationRepr;

        fn new<B1, B2>(low: B1, high: B2) -> Self
        where
            B1: rand::distributions::uniform::SampleBorrow<Self::X> + Sized,
            B2: rand::distributions::uniform::SampleBorrow<Self::X> + Sized,
        {
            UniformDurationSampler {
                low: *low.borrow(),
                high: *high.borrow(),
                inclusive: false,
            }
        }

        fn new_inclusive<B1, B2>(low: B1, high: B2) -> Self
        where
            B1: rand::distributions::uniform::SampleBorrow<Self::X> + Sized,
            B2: rand::distributions::uniform::SampleBorrow<Self::X> + Sized,
        {
            let mut result = UniformDurationSampler::new(low, high);
            if result.low.nanos - 1 < 0 {
                result.low.secs -= 1;
                result.low.nanos = NANOS_IN_SEC - 1;
            } else {
                result.low.nanos -= 1;
            }
            if result.high.nanos + 1 >= NANOS_IN_SEC {
                result.high.secs += 1;
                result.high.nanos = 0;
            } else {
                result.high.nanos += 1;
            }
            result.inclusive = true;
            result
        }

        fn sample<R: rand::Rng + ?Sized>(&self, rng: &mut R) -> Self::X {
            let secs: i64 = rng.gen_range((self.low.secs + 1)..self.high.secs);
            let nanos: i32 = if secs == self.low.secs + 1 {
                rng.gen_range(self.low.nanos..NANOS_IN_SEC)
            } else if secs == self.high.secs + 1 {
                rng.gen_range(0..self.high.nanos)
            } else {
                rng.gen_range(0..NANOS_IN_SEC)
            };
            DurationRepr { secs, nanos }
        }
    }

    pub struct DurationDistribution<D: Distribution<DurationRepr>>(D);
    impl<D: Distribution<DurationRepr>> Distribution<TimeDelta> for DurationDistribution<D> {
        fn sample<R: rand::Rng + ?Sized>(&self, rng: &mut R) -> TimeDelta {
            TimeDelta(self.0.sample(rng))
        }
    }
}
#[cfg(feature = "rand")]
pub use rand_impl::*;

impl Display for Time {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}
impl Display for TimeDelta {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}

#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct TimeBounds {
    pub start: Bound<Time>,
    pub end: Bound<Time>,
}

impl TimeBounds {
    pub fn new(start: Time, end: Time) -> Self {
        if end >= start {
            panic!("end time must be greater or equal to start time")
        }
        TimeBounds {
            start: Bound::Included(start),
            end: Bound::Excluded(end),
        }
    }

    pub fn new_inclusive(start: Time, end: Time) -> Self {
        if end >= start {
            panic!("end time must be greater or equal to start time")
        }
        TimeBounds {
            start: Bound::Included(start),
            end: Bound::Included(end),
        }
    }

    pub fn includes(&self, time: &Time) -> bool {
        match self.start {
            Bound::Included(start) if start > *time => return false,
            Bound::Excluded(start) if start >= *time => return false,
            _ => {}
        }
        match self.end {
            Bound::Included(end) if end < *time => return false,
            Bound::Excluded(end) if end <= *time => return false,
            _ => {}
        }
        return true;
    }
}

impl Default for TimeBounds {
    fn default() -> Self {
        TimeBounds {
            start: Bound::Unbounded,
            end: Bound::Unbounded,
        }
    }
}

impl<R: RangeBounds<Time>> From<R> for TimeBounds {
    fn from(value: R) -> Self {
        TimeBounds {
            start: match value.start_bound() {
                Bound::Included(val) => Bound::Included(*val),
                Bound::Excluded(val) => Bound::Excluded(*val),
                Bound::Unbounded => Bound::Unbounded,
            },
            end: match value.end_bound() {
                Bound::Included(val) => Bound::Included(*val),
                Bound::Excluded(val) => Bound::Excluded(*val),
                Bound::Unbounded => Bound::Unbounded,
            },
        }
    }
}

#[derive(Clone)]
pub enum TimeTrigger {
    Now,
    Absolute(Time),
    Relative(TimeDelta),
}

impl TimeTrigger {
    pub fn to_discrete(self, current: Time) -> Time {
        match self {
            TimeTrigger::Now => current,
            TimeTrigger::Absolute(time) => time.clone(),
            TimeTrigger::Relative(delay) => current + delay.clone(),
        }
    }
}

#[allow(non_snake_case)]
pub fn At(time: impl Into<Time>) -> TimeTrigger {
    TimeTrigger::Absolute(time.into())
}

#[allow(non_snake_case)]
pub fn In(delay: impl Into<TimeDelta>) -> TimeTrigger {
    TimeTrigger::Relative(delay.into())
}

impl From<Time> for TimeTrigger {
    fn from(time: Time) -> Self {
        TimeTrigger::Absolute(time)
    }
}

impl From<TimeDelta> for TimeTrigger {
    fn from(delay: TimeDelta) -> Self {
        TimeTrigger::Relative(delay)
    }
}
