use std::collections::VecDeque;
use serde::{Serialize, Deserialize};

use crate::time::*;

mod sparse_map;
use sparse_map::{SparseMap};

#[derive(Debug, Clone, Copy, PartialEq, PartialOrd, Serialize, Deserialize)]
pub struct Point {
    pub delta: Duration,
    pub cost: Cost, 
}

impl Point {
    pub fn new(delta: Duration, cost: Cost) -> Self {
        Point {
            delta: delta,
            cost: cost,
        }
    }
}

/* The "curve" map maps distance to total cost. 
It answers the question: What is the minimum distance to observe AT MOST a total cost of c?
The distance is *exclusive*, meaning that:
- Distance 0 is considered to be 0.
- Distance 1 is considered to be a single arrival. */
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RbfCurve {
    last_arrivals_window: VecDeque<(Time, Cost)>,
    window_size: usize,
    pub curve: SparseMap,
    pub wcet: Cost,
    pub pid: Pid,
    pub prio: Priority,
}

impl RbfCurve {
    pub fn add_arrival(&mut self, instant: Time, cost: Cost) {
        
        let arrival: (Time, Cost) = (instant, cost);
        let t = instant;
        let mut curr_observed_tot_cost = 0;

        // sanity check: the arrival times must be monotonic
        assert!(t >= self.last_arrivals_window.back().unwrap_or(&arrival).0);
        // add to treat the observed_gap = 0 case
        self.last_arrivals_window.push_back(arrival);
        // look at all arrival times in the sliding window, in order
        // from most recent to oldest
        for arr in self.last_arrivals_window.iter().rev() {
            // Compute the separation from the current arrival t to the arrival
            // of the (i + 1)-th preceding job.
            // So if i=0, we are looking at two adjacent jobs.
            let observed_gap = t - arr.0 + 1;
            curr_observed_tot_cost += arr.1;

            // we have not yet seen a distance of length "observed_gap" -> first sample
            // add new distance only if the observed total cost is bigger than the one observed in the nearest preceding gap
            if curr_observed_tot_cost > self.get(observed_gap) {
                let p = Point::new(observed_gap, curr_observed_tot_cost);
                self.curve.add(p);
            }
        }

        // trim sliding window if necessary
        if self.last_arrivals_window.len() > self.window_size {
            self.last_arrivals_window.pop_front();
        }

        // update WCET
        self.wcet = self.wcet.max(cost); // TODO could just return the cost for key 0
    }

    pub fn add_arrivals(&mut self, arrivals: &Vec<(Time, Cost)>) {
        for (t, c) in arrivals {
            self.add_arrival(*t, *c);
        }
    }

    // Returns the lower nearest cost to delta (we only store the steps)
    pub fn get(&self, delta: Duration) -> Cost {
        return self.curve.get(delta);
    }

    pub fn sum(&mut self, other: &RbfCurve) {
        // Cloning the first curve because we would need to mutate it while iterating
        let curve_1_clone = self.curve.clone();
        let mut curve_1 = curve_1_clone.into_iter();
        let mut curve_2 = other.curve.into_iter();
        let mut last_cost_1 = 0;
        let mut last_cost_2 = 0;
        let mut point_1 = curve_1.next();
        let mut point_2 = curve_2.next();

        // Add the point with the lowest delta
        while let (Some(p_1), Some(p_2)) = (point_1, point_2) {
            if p_1.delta == p_2.delta {
                self.curve.insert(Point::new(p_1.delta, p_1.cost + p_2.cost));

                last_cost_1 = p_1.cost + p_2.cost;
                last_cost_2 = p_1.cost + p_2.cost;
                point_1 = curve_1.next();
                point_2 = curve_2.next();
            } else if p_1.delta < p_2.delta {
                self.curve.insert(Point::new(p_1.delta, p_1.cost + last_cost_2));

                last_cost_1 = p_1.cost;
                point_1 = curve_1.next();
            } else {
                self.curve.insert(Point::new(p_2.delta, p_2.cost + last_cost_1));

                last_cost_2 = p_2.cost;
                point_2 = curve_2.next();
            }
        }

        // Edge case: in the last step, both iterators stepped before exiting the loop
        if let Some(p_1) = point_1 {
            self.curve.insert(Point::new(p_1.delta, p_1.cost + last_cost_2));
            last_cost_1 = p_1.cost;
        }
        if let Some(p_2) = point_2 {
            self.curve.insert(Point::new(p_2.delta, p_2.cost + last_cost_1));
            last_cost_2 = p_2.cost;
        }
        // Add any other remaining points
        for p_1 in curve_1 {
            self.curve.insert(Point::new(p_1.delta, p_1.cost + last_cost_2));
        }
        for p_2 in curve_2 {
            self.curve.insert(Point::new(p_2.delta, p_2.cost + last_cost_1));
        }
    }

    pub fn print_curve(&self) {
        for point in &self.curve {
            print!("[{} : {}] ", point.delta, point.cost);
        }
        print!("\n");
    }


    pub fn new(pid: Pid, window_size: usize) -> Self {
        let mut curve = SparseMap::new(window_size);
        curve.add(Point::new(0, 0));
        RbfCurve { 
            last_arrivals_window: VecDeque::with_capacity(window_size+1),
            window_size: window_size,
            curve: curve,
            wcet: 0,
            pid: pid,
            prio: 0,
        }
    }
}

impl<T> From<T> for RbfCurve 
where T: AsRef<[(Time, Cost)]> {
    fn from(trace: T) -> RbfCurve {
        let mut ret = RbfCurve::new(1, 1000);
        ret.add_arrivals(&Vec::from(trace.as_ref()));
        ret
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty() {
        let rbf = RbfCurve::from([]);
        let extracted_curve: Vec<Point> = rbf.curve.into_iter().collect();
        let ground_truth = [p(0, 0)];

        assert_eq!(extracted_curve, ground_truth);
    }

    #[test]
    fn periodic() {
        let rbf = RbfCurve::from([(0, 5), (5, 5), (10, 5), (15, 5), (20, 5)]);
        let extracted_curve: Vec<Point> = rbf.curve.into_iter().collect();
        let ground_truth = [p(0, 0), p(1, 5), p(6, 10), p(11, 15), p(16, 20), p(21, 25)];

        assert_eq!(extracted_curve, ground_truth);
    }

    #[test]
    fn periodic_var_cost() {
        let rbf = RbfCurve::from([(0, 1), (5, 6), (10, 5), (15, 50), (20, 5)]);
        let extracted_curve: Vec<Point> = rbf.curve.into_iter().collect();
        let ground_truth = [p(0, 0), p(1, 50), p(6, 55), p(11, 61), p(16, 66), p(21, 67)];

        assert_eq!(extracted_curve, ground_truth);
    }

    #[test]
    fn bursty() {
        let rbf = RbfCurve::from([(0, 10), (1, 10), (2, 10), (20, 10), (21, 10), (22, 10)]);
        let extracted_curve: Vec<Point> = rbf.curve.into_iter().collect();
        let ground_truth = [p(0, 0), p(1, 10), p(2, 20), p(3, 30), p(21, 40), p(22, 50), p(23, 60)];

        assert_eq!(extracted_curve, ground_truth);
    }

    #[test]
    fn far_spikes() {
        let rbf = RbfCurve::from([(4, 90), (5, 90), (50, 100)]);
        let extracted_curve: Vec<Point> = rbf.curve.into_iter().collect();
        let ground_truth = [p(0, 0), p(1, 100), p(2, 180), p(46, 190), p(47, 280)];

        assert_eq!(extracted_curve, ground_truth);
    }

    #[test]
    fn sum_empty() {
        let mut rbf1 = RbfCurve::new(1, 1000);
        let rbf2 = RbfCurve::new(1, 1000);
        
        rbf1.sum(&rbf2);
        let rbf1_curve: Vec<Point> = rbf1.curve.into_iter().collect();

        assert_eq!(
            rbf1_curve,
            [p(0, 0)]
        );
    }

    #[test]
    fn sum_empty_2() {
        let mut rbf1 = RbfCurve::new(1, 1000);
        let mut rbf2 = RbfCurve::new(1, 1000);
        let curve2 = [p(0, 0), p(1, 100), p(2, 180), p(46, 190), p(47, 280)];
        for p in curve2 {
            rbf2.curve.insert(p);
        }

        rbf1.sum(&rbf2);
        let rbf1_vec: Vec<Point> = rbf1.curve.into_iter().collect();
        let rbf2_vec: Vec<Point> = rbf2.curve.into_iter().collect();

        assert_eq!(
            rbf1_vec, 
            rbf2_vec
        );
    }

    #[test]
    fn sum_empty_3() {
        let mut rbf1 = RbfCurve::new(1, 1000);
        let rbf2 = RbfCurve::new(1, 1000);
        let curve1 = [p(0, 0), p(1, 100), p(2, 180), p(46, 190), p(47, 280)];
        for p in curve1 {
            rbf1.curve.insert(p);
        }

        rbf1.sum(&rbf2);
        let rbf1_vec: Vec<Point> = rbf1.curve.into_iter().collect();

        assert_eq!(
            rbf1_vec, 
            curve1
        );
    }

    #[test]
    fn sum_double() {
        let mut rbf1 = RbfCurve::new(1, 1000);
        let mut rbf2 = RbfCurve::new(1, 1000);
        let curve1 = [p(0, 0), p(5, 10), p(10, 20), p(20, 30), p(50, 40)];
        let curve2 = [p(0, 0), p(5, 10), p(10, 20), p(20, 30), p(50, 40)];
        for p in curve1 {
            rbf1.curve.insert(p);
        }
        for p in curve2 {
            rbf2.curve.insert(p);
        }

        rbf1.sum(&rbf2);
        let rbf1_vec: Vec<Point> = rbf1.curve.into_iter().collect();

        assert_eq!(
            rbf1_vec, 
            [p(0, 0), p(5, 20), p(10, 40), p(20, 60), p(50, 80)]
        );
    }

    #[test]
    fn sum() {
        let mut rbf1 = RbfCurve::new(1, 1000);
        let mut rbf2 = RbfCurve::new(1, 1000);
        let curve1 = [p(0, 0), p(5, 5), p(15, 10), p(25, 15)];
        let curve2 = [p(0, 0), p(10, 5), p(20, 10), p(30, 15)];
        for p in curve1 {
            rbf1.curve.insert(p);
        }
        for p in curve2 {
            rbf2.curve.insert(p);
        }

        rbf1.sum(&rbf2);
        let rbf1_vec: Vec<Point> = rbf1.curve.into_iter().collect();

        assert_eq!(
            rbf1_vec, 
            [p(0, 0), p(5, 5), p(10, 10), p(15, 15), p(20, 20), p(25, 25), p(30, 30)]
        );
    }

    #[test]
    fn sum_var_cost() {
        let mut rbf1 = RbfCurve::new(1, 1000);
        let mut rbf2 = RbfCurve::new(1, 1000);
        let curve1 = [p(0, 0), p(5, 5), p(15, 10)];
        let curve2 = [p(0, 0), p(10, 5), p(20, 11)];
        for p in curve1 {
            rbf1.curve.insert(p);
        }
        for p in curve2 {
            rbf2.curve.insert(p);
        }

        rbf1.sum(&rbf2);
        let rbf1_vec: Vec<Point> = rbf1.curve.into_iter().collect();

        assert_eq!(
            rbf1_vec, 
            [p(0, 0), p(5, 5), p(10, 10), p(15, 15), p(20, 21)]
        );
    }

    #[test]
    fn sum_last_step() {
        let mut rbf1 = RbfCurve::new(1, 1000);
        let mut rbf2 = RbfCurve::new(1, 1000);
        let curve1 = [p(0, 0), p(5, 5), p(20, 10), p(30, 11), p(31, 12)];
        let curve2 = [p(0, 0), p(5, 10)];
        for p in curve1 {
            rbf1.curve.insert(p);
        }
        for p in curve2 {
            rbf2.curve.insert(p);
        }

        rbf1.sum(&rbf2);
        let rbf1_vec: Vec<Point> = rbf1.curve.into_iter().collect();

        assert_eq!(
            rbf1_vec, 
            [p(0, 0), p(5, 15), p(20, 25), p(30, 26), p(31, 27)]
        );
    }

    /* Support */

    fn p(delta: Duration, cost: Cost) -> Point {
        Point::new(delta, cost)
    }
}
