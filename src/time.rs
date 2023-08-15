use std::fmt::Display;

#[cfg(feature = "time_f32")]
pub type SimTimeValue = f32;
#[cfg(feature = "time_f64")]
pub type SimTimeValue = f64;
#[cfg(feature = "time_chrono")]
pub type SimTimeValue = chrono::NaiveDateTime;

#[cfg(feature = "time_f32")]
pub type SimDuration = f32;
#[cfg(feature = "time_f64")]
pub type SimDuration = f64;
#[cfg(feature = "time_chrono")]
pub type SimDuration = chrono::Duration;

#[derive(Debug, Default, Clone, Copy, PartialEq)]
#[repr(transparent)]
pub struct SimulationTime {
    pub(crate) value: SimTimeValue,
}

impl SimulationTime {
    pub const MIN: SimulationTime = SimulationTime::new(SimTimeValue::MIN);
    pub const MAX: SimulationTime = SimulationTime::new(SimTimeValue::MAX);

    pub const fn new(value: SimTimeValue) -> Self {
        SimulationTime { value }
    }

    pub fn into_inner(self) -> SimTimeValue {
        self.value
    }
}

impl PartialOrd for SimulationTime {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        #[cfg(any(feature = "time_f32", feature = "time_f64"))]
        {
            return float_ord::FloatOrd(self.value).partial_cmp(&float_ord::FloatOrd(other.value));
        }
        #[cfg(feature = "time_chrono")]
        {
            return self.value.partial_cmp(&other.value);
        }
    }
}

impl Eq for SimulationTime {}

impl Ord for SimulationTime {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        #[cfg(any(feature = "time_f32", feature = "time_f64"))]
        {
            return float_ord::FloatOrd(self.value).cmp(&float_ord::FloatOrd(other.value));
        }
        #[cfg(feature = "time_chrono")]
        {
            return self.value.cmp(&other.value);
        }
    }
}

impl From<SimTimeValue> for SimulationTime {
    fn from(value: SimTimeValue) -> Self {
        Self::new(value)
    }
}

impl std::ops::Add<SimDuration> for SimulationTime {
    type Output = SimulationTime;

    fn add(self, rhs: SimDuration) -> Self::Output {
        Self::new(self.value + rhs)
    }
}

impl Display for SimulationTime {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.value.fmt(f)
    }
}
