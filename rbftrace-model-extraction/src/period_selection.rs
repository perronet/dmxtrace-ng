use rbftrace_core::time::*;
use crate::arrival::arrival_subset::PeriodRange;

/// Choose between the two multiples of 10 closest to the median: pick the roundest.
/// Tiebreak 1: pick the closest to the median.
/// Tiebreak 2: pick the smallest of the two.
pub fn pick_period_heuristic(feasible_periods: PeriodRange) -> Period {
    if !feasible_periods.is_empty {
        let median = ((feasible_periods.t_min + feasible_periods.t_max) as f64 / 2.).floor() as Period;
        let left_dist = median % 10;
        let right_dist = 10 - left_dist;
        let left_mult = median - left_dist;
        let right_mult = median + right_dist;

        // Check if the multiples are inside the range
        match (feasible_periods.contains(left_mult), feasible_periods.contains(right_mult)) {
            (true, false) => { return left_mult; },
            (false, true) => { return right_mult; },
            (false, false) => { return median; },
            _ => {},
        }

        let roundness_left = roundness(left_mult);
        let roundness_right = roundness(right_mult);

        // Pick roundest number
        if roundness_left < roundness_right {
            return right_mult;
        } else if roundness_left > roundness_right {
            return left_mult;
        } else {
            // Tiebreak: smaller distance to median
            if left_dist > right_dist {
                return right_mult;
            } else {
                // Tiebreak: smaller period
                return left_mult;
            }
        }
    }

    0
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
    use super::{PeriodRange, pick_period_heuristic};
    
    #[test]
    fn pick_period_empty() {
        let interval = PeriodRange::default();
        let period = pick_period_heuristic(interval);

        assert_eq!(period, 0);
    }

    #[test]
    fn pick_period() {
        let interval_1 = PeriodRange::new(1111, 3222);
        let interval_2 = PeriodRange::new(999, 3222);
        let period_1 = pick_period_heuristic(interval_1);
        let period_2 = pick_period_heuristic(interval_2);

        assert_eq!(period_1, 2170);
        assert_eq!(period_2, 2110);
    }

    #[test]
    fn pick_period_round_bound() {
        let interval = PeriodRange::new(1000, 5000);
        let period = pick_period_heuristic(interval);

        assert_eq!(period, 3000);
    }

    #[test]
    fn pick_period_small() {
        let interval = PeriodRange::new(1000, 1001);
        let period = pick_period_heuristic(interval);

        assert_eq!(period, 1000);
    }

    #[test]
    fn pick_period_single() {
        let interval = PeriodRange::new(1000, 1000);
        let period = pick_period_heuristic(interval);

        assert_eq!(period, 1000);
    }

    #[test]
    fn pick_period_single_nonround() {
        let interval = PeriodRange::new(33333333, 33333333);
        let period = pick_period_heuristic(interval);

        assert_eq!(period, 33333333);
    }

    #[test]
    fn pick_period_tiebreak() {
        let interval = PeriodRange::new(90, 100);
        let period = pick_period_heuristic(interval);

        assert_eq!(period, 100);
    }

    #[test]
    fn pick_period_tiebreak_2() {
        let interval = PeriodRange::new(80, 90);
        let period = pick_period_heuristic(interval);

        assert_eq!(period, 80);
    }

    #[test]
    fn pick_period_out_of_bounds() {
        let interval = PeriodRange::new(22, 24);
        let period = pick_period_heuristic(interval);

        assert_eq!(period, 23);
    }

    #[test]
    fn pick_period_out_of_bounds_2() {
        let interval = PeriodRange::new(90, 99);
        let period = pick_period_heuristic(interval);

        assert_eq!(period, 90);
    }
}
