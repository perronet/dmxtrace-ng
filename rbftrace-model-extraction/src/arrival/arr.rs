use std::cmp::Ordering;

use rbftrace_core::time::*;

#[derive(Eq, PartialEq, Copy, Clone, Debug, Default)]
pub struct Arrival {
    pub instant : Time,
    pub idx : u64,
    /// Total cost, including self-suspension time (suspension-oblivious)
    pub cost : Cost,
    /// Self-suspension time
    pub ss_time : Cost,
    /// Self-suspension count
    pub ss_cnt : u64,

    /*** Data used by only by the model matcher ***/
    pub t_avg_min : Period,
    pub t_avg_max : Period,
    pub buf_prio : u64,
}

impl Arrival {
    pub fn new (instant: Time, cost : Cost, ss_time : Duration, ss_cnt : u64) -> Self 
    {
        Arrival {
            instant,
            idx : 0, // The index depends on the arrival sequence
            cost,
            ss_time,
            ss_cnt,

            t_avg_min : Time::from_ns(u64::max_value()),
            t_avg_max : Time::from_ns(u64::min_value()),
            buf_prio : 0,
        }
    }
}

impl Ord for Arrival {
    // Priority ordering
    fn cmp(&self, other: &Self) -> Ordering {
        let cmp_prio = self.buf_prio.cmp(&other.buf_prio);
        if cmp_prio == Ordering::Equal { // Tiebreak: prioritize older observations (lower idx)
            return self.idx.cmp(&other.idx).reverse();
        }

        cmp_prio
    }
}

impl PartialOrd for Arrival {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}
