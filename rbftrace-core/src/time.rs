use std::{ops::{Add, AddAssign, Sub, SubAssign, Div, Rem, RemAssign, DivAssign, Mul, MulAssign}, fmt::Display, str::FromStr, num::ParseIntError};

use duplicate::duplicate;
use serde::{Serialize, Deserialize};



#[derive(PartialEq, Eq, Clone, Copy, PartialOrd, Ord, Serialize, Deserialize, Debug, Default)]
#[serde(from="u64")]
#[serde(into="u64")]
pub struct Time {
    ns: u64
}

// pub type Time = u64; // Integer nanoseconds
pub type Duration = Time;
pub type Cost = Time;
pub type Period = Time;
pub type Jitter = Time;
pub type Mit = Time;
pub type Offset = Time;

impl Time {
    pub const fn zero() -> Self {
        Time { ns: 0}
    }

    pub fn is_zero(&self) -> bool {
        self.ns == 0
    }
    
    pub fn to_ns(&self) -> u64 {
        self.ns
    }

    pub fn from_ns(ns: u64) -> Self {
        Time {ns}
    }
    
    pub fn to_us(&self) -> f64 {
        (self.ns as f64) / (10_f64.powi(3))
    }

    pub fn from_us(us: f64) -> Self {
        let ns = (us * (10_f64.powi(3))) as u64;
        
        Time::from_ns(ns)
    }
    
    pub fn to_ms(&self) -> f64 {
        (self.ns as f64) / (10_f64.powi(6))
    }

    pub fn from_ms(ms: f64) -> Self {
        let ns = (ms * (10_f64.powi(6))) as u64;
        
        Time::from_ns(ns)
    }

    
    pub fn from_s(s: f64) -> Self {
        let ns = (s*((10_f64).powi(9))) as u64;
        
        Time::from_ns(ns)
    }

    pub fn to_s(&self) -> f64 {
        (self.ns as f64) / ((10_f64).powi(9))
    }

    pub fn truncate(&self, resolution: Time) -> Time {
        let new_ns = (self.ns / resolution.ns) * resolution.ns;

        Time::from_ns(new_ns)
    }

    pub fn round(self, resolution: Time) -> Time {
        let halfway = resolution / 2_u32;

        let mut new_ns = self.ns / resolution.ns;
        
        
        if (self % resolution) >= halfway {
            new_ns += 1;   
        }

        new_ns *= resolution.ns;

        Time::from_ns(new_ns)
    }
}

impl From<u64> for Time {
    fn from(ns: u64) -> Self {
        Time::from_ns(ns)
    }
}

impl From<Time> for u64 {
    fn from(val: Time) -> Self {
        val.ns
    }
}

impl Add for Time {
    type Output = Time;

    fn add(self, rhs: Self) -> Self::Output {
        Time::from_ns(self.ns + rhs.ns)
    }
}

impl AddAssign for Time {
    fn add_assign(&mut self, rhs: Self) {
        self.ns += rhs.ns
    }
}

impl Sub for Time {
    type Output = Time;

    fn sub(self, rhs: Self) -> Self::Output {
        Time::from_ns(self.ns - rhs.ns)
    }
}

impl SubAssign for Time {
    fn sub_assign(&mut self, rhs: Self) { 
        self.ns -= rhs.ns
    }
}

#[duplicate(
    int_type; [u8]; [u16]; [u32]; [u64];[usize]
)]
impl Div<int_type> for Time {
    type Output = Time;

    fn div(self, rhs: int_type) -> Self::Output {
        Time::from_ns(self.ns / rhs as u64)
    }
}


#[duplicate(
    int_type; [u8]; [u16]; [u32]; [u64];[usize]
)]
impl DivAssign<int_type> for Time {
    fn div_assign(&mut self, rhs: int_type) {
        self.ns /= rhs as u64
    }
}

#[duplicate(
    float_type; [f32]; [f64]
)]
impl Div<float_type> for Time {
    type Output = Time;

    fn div(self, rhs: float_type) -> Self::Output {
        let ns_f = self.ns as float_type / rhs;

        Time::from_ns(ns_f as u64)
    }
}

#[duplicate(
    float_type; [f32]; [f64]
)]
impl DivAssign<float_type> for Time {
    fn div_assign(&mut self, rhs: float_type) {
        let ns_f = (self.ns as float_type / rhs) as u64;
        self.ns /= ns_f
    }
}

#[duplicate(
    int_type; [u8]; [u16]; [u32]; [u64]; [usize];
)]
impl Mul<int_type> for Time {
    type Output = Time;
     
    fn mul(self, rhs: int_type) -> Self::Output {
        Time::from_ns(self.ns * rhs as u64)
    }
}

#[duplicate(
    int_type; [u8]; [u16]; [u32]; [u64];[usize]
)]
impl MulAssign<int_type> for Time {
    fn mul_assign(&mut self, rhs: int_type) {
        self.ns *= rhs as u64
    }
}

#[duplicate(
    float_type; [f32]; [f64]
)]
impl Mul<float_type> for Time {
    type Output = Time;

    fn mul(self, rhs: float_type) -> Self::Output {
        let ns_f = self.ns as float_type * rhs;

        Time::from_ns(ns_f as u64)
    }
}

#[duplicate(
    float_type; [f32]; [f64]
)]
impl MulAssign<float_type> for Time {
    fn mul_assign(&mut self, rhs: float_type) {
        let ns_f = (self.ns as float_type / rhs) as u64;
        self.ns *= ns_f
    }
}

impl Rem for Time {
    type Output = Time;

    fn rem(self, rhs: Self) -> Self::Output {
        Time::from_ns(self.ns % rhs.ns)
    }
}

impl RemAssign for Time {
    fn rem_assign(&mut self, rhs: Self) {
        self.ns %= rhs.ns
    }
}

impl Display for Time {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.ns)
    }
}

impl FromStr for Time {
    type Err = ParseIntError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let ns = s.parse::<u64>()?;

        Ok(Time::from_ns(ns))
    }
}

#[cfg(test)]
mod tests {
    use crate::time::Time;

    #[test]
    fn test_truncate() {
        assert_eq!(Time::from_ms(1.55).truncate(Time::from_ms(1.)), Time::from_ms(1.));
        assert_eq!(Time::from_ms(1.55).truncate(Time::from_ms(0.1)), Time::from_ms(1.5));
    }

    #[test]
    fn test_round(){
        let r1 = Time::from_ms(1.0);
        let r2 = Time::from_ms(0.1);

        let t1 = Time::from_ms(1.5);
        let t2 = Time::from_ms(1.55);
        let t3 = Time::from_ms(1.4);
        let t4 = Time::from_ms(1.45);

        assert_eq!(t1.round(r1), Time::from_ms(2.0));
        assert_eq!(t2.round(r1), Time::from_ms(2.0));
        assert_eq!(t3.round(r1), Time::from_ms(1.0));
        assert_eq!(t4.round(r1), Time::from_ms(1.0));
        
        assert_eq!(t1.round(r2), Time::from_ms(1.5));
        assert_eq!(t2.round(r2), Time::from_ms(1.6));
        assert_eq!(t3.round(r2), Time::from_ms(1.4));
        assert_eq!(t4.round(r2), Time::from_ms(1.5));
    }

}