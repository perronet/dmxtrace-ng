use std::{collections::HashMap};

use rbftrace_core::{time::*};
// use rbftrace_core::model::scalar::*;
use crate::arrival::{
    arr::Arrival,
    arrival_subset::ArrivalSequenceSubset,
};

use rbftrace_core::model::ScalarTaskModel;

#[derive(Debug, Copy, Clone, PartialEq, Default)]
pub struct MatchedModels {
    pub pjitter_offset: Option<ScalarTaskModel>,
    pub pjitter: Option<ScalarTaskModel>,
    pub sporadic: Option<ScalarTaskModel>,
}

pub struct ScalarMM {
    /*** Saved state ***/
    /// Subset of the most relevant arrivals seen so far. Can hold at maximum buf_size arrivals
    /// Internally holds the current feasible period ranges for each task
    /// The most "relevant" arrivals are the ones which change dramatically the period range
    /// (i.e. observations that are likely to invalidate our past observations)
    arrival_buffers: HashMap<Pid, ArrivalSequenceSubset>,

    /*** Output ***/
    /// All models that can plausibly explain the trace
    pub matched_models: HashMap<Pid, MatchedModels>,
    /// The picked model out of all plausible models
    pub chosen_models: HashMap<Pid, Option<ScalarTaskModel>>,

    /*** Tunables ***/
    /// Maximum jitter for matching a periodic with jitter model
    jitter_bound: Jitter,
    /// Size of each of the arrival buffers
    buf_size: usize,
} 

impl ScalarMM {

    pub fn update_internal_state(&mut self, pid: Pid, arrival: Arrival) {
        let buf_size = self.buf_size;
        let jitter_bound = self.jitter_bound;
            let arr_seq_ref = self.arrival_buffers
                                                         .entry(pid)
                                                         .or_insert_with(move || ArrivalSequenceSubset::new(pid, buf_size, jitter_bound));
            arr_seq_ref.add_arrival(arrival); // Internally computes the feasible period range
    }

    pub fn extract_model(&self, pid: Pid) -> Option<ScalarTaskModel>{
        let arr_seq = self.arrival_buffers.get(&pid)?;

        let mut matched = MatchedModels {
            sporadic: match_sporadic(arr_seq),
            pjitter_offset: match_pjitter_offset(arr_seq, self.jitter_bound),
            ..Default::default()
        };

        if let Some(m) = matched.pjitter_offset {
            matched.pjitter = m.pjo_to_pj();
        }
        // Pick a model out of the matched ones and return it
        disambiguate_model(&matched)
    }


    pub fn new(jitter_bound: Jitter, buf_size: usize) -> Self {
        let mut j = jitter_bound;
        let mut buf = buf_size;
        if jitter_bound.is_zero() {
            j = Time::from_ms(1.5)
        }

        if buf_size == 0 {
            buf = 1_000
        }

        ScalarMM {
            arrival_buffers: HashMap::new(),
            matched_models: HashMap::new(),
            chosen_models: HashMap::new(),

            jitter_bound: j,
            buf_size: buf,
        }
    }
}

/*** Matching functions for each model ***/

fn match_sporadic(arr_seq: &ArrivalSequenceSubset) -> Option<ScalarTaskModel> {
    let mit = arr_seq.min_interarrival;
    let wcet = arr_seq.wcet;
    
    if mit > Time::zero() {
        return Some(ScalarTaskModel::sporadic(wcet, mit));
    }

    None
}

fn match_pjitter_offset(arr_seq: &ArrivalSequenceSubset, jitter_bound: Jitter) -> Option<ScalarTaskModel> {
    let period_range = arr_seq.t_interval;
    if period_range.is_empty || arr_seq.last_arrival.is_none() {
        return None;
    }
    let period = crate::period_selection::pick_period_heuristic(period_range);
    
    assert!(period_range.contains(period));

    if let Some((jitter, offset)) = fit_period(&arr_seq.arrivals, period, jitter_bound) {
        return Some(ScalarTaskModel::periodic_jitter_offset(arr_seq.wcet, period, jitter, offset));
    }

    None
}

fn disambiguate_model(models: &MatchedModels) -> Option<ScalarTaskModel> {
    /* Pick the most precise model */
    if models.pjitter_offset.is_some() {
        return models.pjitter_offset;
    } else if models.pjitter.is_some() {
        return models.pjitter;
    } else if models.sporadic.is_some() {
        return models.sporadic;
    }
    None
}

/// Checks if the period fits in the trace, returns the jitter and offset for the best fit
fn fit_period(arrivals: &[Arrival], p: Period, j_bound: Jitter) -> Option<(Jitter, Offset)> {
    let mut jitter: i64; // Can be negative, in which case the period doesn't fit
    let mut max_jitter: Jitter = Time::zero();
    let mut max_error: u64 = 0;
    let mut n_tries: u32 = 1;
    let mut t: i64;
    let mut fit_found = false;

    let mut arrivals_sorted = arrivals.to_vec();
    arrivals_sorted.sort_by(|a, b| a.idx.cmp(&b.idx));

    let mut first_arr_jitter: u64 = 0;
    let first_arr_idx= arrivals_sorted[0].idx;
    let first_arr= arrivals_sorted[0].instant;

    while !fit_found && n_tries <= 2 {
        let t_0 = first_arr.to_ns() as i64 - first_arr_jitter as i64; // Starting point
        for arr in &arrivals_sorted {
            let k = arr.idx - first_arr_idx; // There might have been prior observations, but we start from 0
            t = t_0 + (k as i64)*(p.to_ns() as i64); // t is a "periodic point" in the trace
            jitter = arr.instant.to_ns() as i64 - t;

            // If the jitter was negative, this will prevent us from trying again with first_arr_jitter > JITTER_BOUND
            if jitter.abs() as u64 > j_bound.to_ns() { // Too much jitter
                return None;
            }
            if n_tries > 1 && jitter < 0 { // Period is too big
                return None;
            }

            if jitter > max_jitter.to_ns() as i64 {
                max_jitter = Time::from_ns(jitter as u64);
            } 
            if jitter < 0 {
                max_error = max_error.max(jitter.abs() as u64);
            }
        }

        if max_error > 0 && n_tries < 2 { // Period is too big, try again with first_arr_jitter = max_error
            first_arr_jitter = max_error;
            n_tries += 1;
            max_error = 0;
            max_jitter = Time::zero();
        } else {
            fit_found = true;
        }
    }

    if fit_found {
        assert!(max_jitter <= j_bound);
        assert!(first_arr_jitter <= max_jitter.to_ns());

        let offset = first_arr.to_ns() as i64 - first_arr_jitter as i64;
        // Underflow. This only happens in dummy traces, we assume that the system has been running long enough to avoid this issue.
        if offset < 0 { panic!("Offset underflow."); }

        return Some((max_jitter, Time::from_ns(offset as u64) as Offset));
    }

    None
}