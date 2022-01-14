use crate::arrival::arr::Arrival;
use rbftrace_core::time::*;

/// A buffer that contains only the most relevant arrivals from another arrival sequence
/// Only buf_size arrivals are considered
/// The most "relevant" arrivals are the ones which change dramatically the period range
/// (i.e. observations that are likely to invalidate our past observations)
#[derive(Debug, Clone)]
pub struct ArrivalSequenceSubset {
    pub pid: Pid,
    pub arrivals: Vec<Arrival>,
    pub buf_size: usize,
    pub last_arrival: Option<Arrival>,

    pub min_interarrival: Duration,
    pub wcet: Cost,
    pub tot_observations: u64, 

    /// Current feasible periods range
    pub t_interval: PeriodRange,

    /// Jitter bound parameter, used for extracting the period range
    pub jitter_bound: Jitter,
}

impl ArrivalSequenceSubset {

    /// Returns None if there are no feasible periods
    pub fn add_arrival(&mut self, mut new_arrival: Arrival) -> Option<PeriodRange> {
        // We assume this to avoid issues related to underflow
        assert!(new_arrival.instant >= self.jitter_bound);

        new_arrival.idx = self.tot_observations;
        self.tot_observations += 1;

        // Update wcet
        self.wcet = self.wcet.max(new_arrival.cost);

        // Update min_interarrival
        if let Some(last) = self.last_arrival {
            let new_interarrival = new_arrival.instant - last.instant;
            if self.min_interarrival.is_zero() {
                self.min_interarrival = new_interarrival;
            } else {
                self.min_interarrival = self.min_interarrival.min(new_interarrival);
            }
        }

        // Update all priorities, compute new period range
        for arr in &mut self.arrivals {
            let l = new_arrival.idx - arr.idx;
            let t_avg: f64 = (new_arrival.instant - arr.instant).to_ns() as f64 / l as f64;
            let error: f64 = self.jitter_bound.to_ns() as f64 / l as f64;
            let t_min = Time::from_ns(((t_avg - error).max(1.0)).ceil() as u64);
            let t_max = Time::from((t_avg + error).floor() as u64);

            // Perform intersection of the intervals
            let t_interval_arr = PeriodRange::new(t_min, t_max);
            let intersection = self.t_interval.intersect(&t_interval_arr);
            if let Some(range) = intersection {
                self.t_interval.is_empty = false;
                self.t_interval = range; 
            } else {
                self.t_interval.is_empty = true;
                return None; // TODO is it okay to stop updating and just return?
            }

            // Update t_avg_max and t_avg_min (also for the new arrival)
            let t_avg_ = Time::from_ns(t_avg.floor() as u64);
            arr.t_avg_min = t_avg_.min(arr.t_avg_min);
            arr.t_avg_max = t_avg_.max(arr.t_avg_max);
            new_arrival.t_avg_min = t_avg_.min(new_arrival.t_avg_min);
            new_arrival.t_avg_max = t_avg_.max(new_arrival.t_avg_max);
            // Update priorities
            arr.buf_prio = (arr.t_avg_max - arr.t_avg_min).to_ns();
            new_arrival.buf_prio = (new_arrival.t_avg_max - new_arrival.t_avg_min).to_ns();
        }

        // Sort by decreasing priority. Tiebreak: prioritize older observations
        self.arrivals.sort_by(|a, b| b.cmp(a)); // TODO we should use a priority queue

        // If the buffer is full, replace the entry with the lowest priority if the new observation has a higher priority
        if self.arrivals.len() >= self.buf_size {
            let lowest_prio_arr = self.arrivals.pop().unwrap();

            if new_arrival.buf_prio > lowest_prio_arr.buf_prio {
                self.arrivals.push(new_arrival);
            } else {
                self.arrivals.push(lowest_prio_arr);
            }
        } else {
            self.arrivals.push(new_arrival);
        }

        // Update last_arrival
        self.last_arrival = Some(new_arrival);

        Some(self.t_interval)
    }

    pub fn new(pid: Pid, buf_size: usize, jitter_bound: Jitter) -> Self {
        ArrivalSequenceSubset { 
            arrivals: Vec::with_capacity(buf_size),
            pid,
            buf_size,
            last_arrival: None,
            min_interarrival: Time::zero(),
            wcet: Time::zero(),
            tot_observations: 0,
            t_interval: PeriodRange::default(),
            jitter_bound,
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct PeriodRange {
    pub t_min: Period,
    pub t_max: Period,
    pub is_empty: bool,
}

impl PeriodRange {
    pub fn intersect(&mut self, other: &PeriodRange) -> Option<PeriodRange> {
        if other.t_min > self.t_max || self.t_min > other.t_max {
            return None;
        }
        Some(PeriodRange::new(self.t_min.max(other.t_min), self.t_max.min(other.t_max)))
    }

    pub fn contains(&self, num: Period) -> bool {
        if !self.is_empty {
            return self.t_min <= num && num <= self.t_max;
        }

        false
    }

    pub fn new(t_min: Period, t_max: Period) -> Self {
        PeriodRange { 
            t_min,
            t_max,
            is_empty: false,
        }
    }

    pub fn default() -> Self {
        PeriodRange { 
            t_min: Time::from_ns(1),
            t_max: Time::from_ns(u64::max_value() - 1),
            is_empty: true,
        }
    }
}
