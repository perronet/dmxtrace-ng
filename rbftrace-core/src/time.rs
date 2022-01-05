pub type Pid = u32;
pub type Cpu = u32;
pub type Priority = u32;
pub type Time = u64; // Integer nanoseconds
pub type Duration = Time;
pub type Cost = Time;
pub type Period = Time;
pub type Jitter = Time;
pub type Mit = Time;
pub type Offset = Time;

pub const ONE_US: Time = 1_000;
pub const ONE_MS: Time = 1_000 * ONE_US;
pub const  ONE_S: Time = 1_000 * ONE_MS; 

pub fn ns_to_s(ns: Time) -> f32 {
    (ns as f32)/((10 as f32).powi(9))
}

pub fn s_to_ns(s: f32) -> Time {
    (s*((10 as f32).powi(9))) as Time
}
