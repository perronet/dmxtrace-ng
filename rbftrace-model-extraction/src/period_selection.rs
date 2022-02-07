use std::cmp::Ordering;

use rbftrace_core::time::*;
use crate::arrival::arrival_subset::PeriodRange;

pub fn pick_period_heuristic(feasible_periods: PeriodRange) -> Period {
    let least = feasible_periods.t_min;
    let largest = feasible_periods.t_max;
    let mean: Time = (largest - least) / 2_u32;

    let mag = (least.to_ns() as f64).log10().floor();
    let mut granularity = Time::from_ns(10_f64.powi(mag as i32) as u64);

    let mean_ns = mean.to_ns() as i64;

    while granularity >= Time::from_us(100.0) {
        let mut first = (least.truncate(granularity) + granularity).to_ns();

        // let mut first = ((least / granularity.to_ns()) + 1) * granularity.to_ns();
        
        let mut candidates = Vec::new();

        while first < largest.to_ns() {
            candidates.push(first as i64);
            first += granularity.to_ns();
        }

        candidates.sort_by_key(|a| (*a as i64 - mean_ns).abs());

        if !candidates.is_empty() {
            return Time::from_ns(candidates[0] as u64);
        }

        granularity /= 10_u32;
    }

    mean.round(Time::from_us(1.))
}

#[cfg(test)]
mod tests {
    use rbftrace_core::time::Time;

    use super::{PeriodRange, pick_period_heuristic};
    
    #[test]
    fn pick_period_empty() {
        let interval = PeriodRange::default();
        let period = pick_period_heuristic(interval);

        assert_eq!(period, Time::zero());
    }

    #[test]
    fn pick_period() {
        let interval_1 = PeriodRange::new(Time::from_ns(1111), Time::from_ns(3222));
        let interval_2 = PeriodRange::new(Time::from_ns(999), Time::from_ns(3222));
        let period_1 = pick_period_heuristic(interval_1);
        let period_2 = pick_period_heuristic(interval_2);

        assert_eq!(period_1, Time::from_ns(2170));
        assert_eq!(period_2, Time::from_ns(2110));
    }

    #[test]
    fn pick_period_round_bound() {
        let interval = PeriodRange::new(Time::from_s(1.), Time::from_s(5.));
        let period = pick_period_heuristic(interval);

        assert_eq!(period, Time::from_s(2.));
    }

    #[test]
    fn pick_period_small() {
        let interval = PeriodRange::new(Time::from_ns(1000), Time::from_ns(1001));
        let period = pick_period_heuristic(interval);

        assert_eq!(period, Time::from_ns(1000));
    }

    #[test]
    fn pick_period_single() {
        let interval = PeriodRange::new(Time::from_s(1.), Time::from_s(1.1));
        let period = pick_period_heuristic(interval);

        assert_eq!(period, Time::from_s(1.));
    }

    #[test]
    fn pick_period_single_nonround() {
        let interval = PeriodRange::new(Time::from_ns(33333333), Time::from_ns(33333333));
        let period = pick_period_heuristic(interval);

        assert_eq!(period, Time::from_ns(33333333));
    }

    #[test]
    fn pick_period_tiebreak() {
        let interval = PeriodRange::new(Time::from_ns(90), Time::from_ns(100));
        let period = pick_period_heuristic(interval);

        assert_eq!(period, Time::from_ns(100));
    }

    #[test]
    fn pick_period_tiebreak_2() {
        let interval = PeriodRange::new(Time::from_ns(80), Time::from_ns(90));
        let period = pick_period_heuristic(interval);

        assert_eq!(period, Time::from_ns(80));
    }

    #[test]
    fn pick_period_out_of_bounds() {
        let interval = PeriodRange::new(Time::from_ns(22), Time::from_ns(24));
        let period = pick_period_heuristic(interval);

        assert_eq!(period, Time::from_ns(23));
    }

    #[test]
    fn pick_period_out_of_bounds_2() {
        let interval = PeriodRange::new(Time::from_ns(90), Time::from_ns(99));
        let period = pick_period_heuristic(interval);

        assert_eq!(period, Time::from_ns(90));
    }
}
