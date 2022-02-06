use std::cmp::Ordering;

use rbftrace_core::time::*;
use crate::arrival::arrival_subset::PeriodRange;

/// Choose between the two multiples of snap_ns closest to the median: pick the roundest.
/// Tiebreak 1: pick the closest to the median.
/// Tiebreak 2: pick the smallest of the two.
pub fn pick_period_heuristic(feasible_periods: PeriodRange) -> Period {
    if !feasible_periods.is_empty {
        let snap_ns = 1000;
        let t_min = feasible_periods.t_min.to_ns();
        let t_max = feasible_periods.t_max.to_ns();
        let median = ((t_min + t_max) as f64 / 2.).floor() as u64;
        let left_dist = median % snap_ns;
        let right_dist = snap_ns - left_dist;
        let left_mult = Time::from_ns(median - left_dist);
        let right_mult = Time::from_ns(median + right_dist);

        // Check if the multiples are inside the range
        match (feasible_periods.contains(left_mult), feasible_periods.contains(right_mult)) {
            (true, false) => { return left_mult; },
            (false, true) => { return right_mult; },
            (false, false) => { return Time::from_ns(median); },
            _ => {},
        }

        let roundness_left = roundness(left_mult.to_ns());
        let roundness_right = roundness(right_mult.to_ns());
        
        // Pick roundest number
        return match roundness_left.cmp(&roundness_right) {
            Ordering::Less => right_mult,
            Ordering::Greater => left_mult,
            Ordering::Equal => {
                // Tiebreak: smaller distance to median
                match left_dist.cmp(&right_dist) {
                    Ordering::Greater => right_mult,
                    // Tiebreak: smaller period
                    _ => left_mult
                }
            }
        };
    }

    Time::zero()
}

fn roundness(n: u64) -> u64 {
    let mut trailing_zeroes = 0;
    let mut n = n;
    while n > 0 && n%10 == 0 {
        trailing_zeroes += 1;
        n /= 10;
    }

    trailing_zeroes
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
        let interval = PeriodRange::new(Time::from_ns(1000), Time::from_ns(5000));
        let period = pick_period_heuristic(interval);

        assert_eq!(period, Time::from_ns(3000));
    }

    #[test]
    fn pick_period_small() {
        let interval = PeriodRange::new(Time::from_ns(1000), Time::from_ns(1001));
        let period = pick_period_heuristic(interval);

        assert_eq!(period, Time::from_ns(1000));
    }

    #[test]
    fn pick_period_single() {
        let interval = PeriodRange::new(Time::from_ns(1000), Time::from_ns(1000));
        let period = pick_period_heuristic(interval);

        assert_eq!(period, Time::from_ns(1000));
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
