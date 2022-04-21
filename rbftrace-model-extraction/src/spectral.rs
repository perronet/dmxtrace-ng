//! This modules contains a model extractor for self-suspending periodic tasks.

use rbftrace_core::{
    trace::{TraceEvent}, 
    time::Time, time::Period, model::PeriodicSelfSuspendingTask
};
use crate::{TaskModelExtractor, job::{JobExtractor}};
use rbftrace_core::model::Job;

use ringbuffer::{RingBuffer, AllocRingBuffer, RingBufferWrite, RingBufferExt};
use realfft::RealFftPlanner;

// TODO only for debug
use serde::{Serialize, Serializer};
use std::{collections::BTreeMap, path::Path, fs::OpenOptions};

use std::f32::consts::PI;

pub struct SpectralExtractionParams {
    pub max_signal_len: usize,
    pub window_size: usize,
    pub fft_filter_cutoff: f32,
}

impl Default for SpectralExtractionParams {
    fn default() -> Self {
        Self { 
            max_signal_len: 1_000_000,
            window_size: 1000,
            fft_filter_cutoff: 0.5,
        }
    }
}

pub struct SpectralExtractor {
    max_signal_len: usize,
    fft_filter_cutoff: f32,

    job_history: AllocRingBuffer<Job>,
    still_periodic: bool,
    job_detector: JobExtractor,

    current_model: Option<PeriodicSelfSuspendingTask>,

    min_gap: Time, // Used for sampling frequency
    wcet: Time,
}

impl SpectralExtractor {
    pub fn new(max_signal_len: usize, window_size: usize, fft_filter_cutoff: f32) -> Self {
        let history_size = window_size.next_power_of_two();
        let job_history = AllocRingBuffer::with_capacity(history_size);
        
        Self {
            max_signal_len: max_signal_len.next_power_of_two(),
            fft_filter_cutoff,
            job_history,
            still_periodic: false,
            job_detector: JobExtractor::new(),
            current_model: None,
            min_gap: Time::zero(),
            wcet: Time::zero(),
        }
    }

    fn extract(&mut self) {
        if self.job_history.len() > 1 {
            // Extract period
            let period = self.fft();
            let mut model = PeriodicSelfSuspendingTask::default();

            if period == Time::zero() {
                self.still_periodic = false;
                self.current_model = None;
                return;
            }
            self.still_periodic = true;
            model.period = period;

            // TODO Extract self-suspensions




            self.current_model = Some(model);
        }
    }

    fn fft(&mut self) -> Period {
        /* Pick the resolution (i.e. sampling frequency) for the signal based
           on the minimum observed interarrival time */
        let mut closest_lower_mag = (self.min_gap.to_ns() as f32).log10().floor() as u32;
        closest_lower_mag -= 1; // Need enough samples when two arrivals have the MIT
        let mut resolution = Time::from(10u64.pow(closest_lower_mag));
        if resolution > Time::from_s(1.0) {
            resolution = Time::from_s(1.0); // Max resolution
        }
        assert!(self.min_gap >= resolution);
        assert!(resolution >= Time::from_us(10.0));
        assert!(resolution <= Time::from_s(1.0));

        let first_arr = self.job_history.get(0).unwrap().arrived_at;
        let trace_delta_ns = self.job_history.back().unwrap().arrived_at - first_arr;
        let mut signal_len = ((trace_delta_ns.to_ns()/resolution.to_ns())+1) as usize;
        signal_len = signal_len.min(self.max_signal_len); // The signal must not be too big to process

        /* Build signal by truncating to desired resolution */
        let mut signal = Vec::with_capacity(signal_len);
        let mut prev_peak_idx = 0;
        signal.push(1f32); // First peak
        for job in self.job_history.iter().skip(1) {
            let arr_truncated = (job.arrived_at - first_arr).truncate(resolution).to_ns();
            let peak_idx = (arr_truncated / resolution.to_ns()) as usize;
            let delta = peak_idx - prev_peak_idx;

            if peak_idx > signal_len {
                signal_len = signal.len();
                break;
            }

            // Generate cosine between the two peaks
            for idx in prev_peak_idx+1..peak_idx+1 {
                signal.push( (2f32*PI*((idx-prev_peak_idx) as f32 / delta as f32)).cos() ); // signal[idx] = ...
            }
            prev_peak_idx = peak_idx;
        }
        assert_eq!(signal_len, signal.len());

        // TODO Debug: dump signal
        // let file1 = OpenOptions::new().write(true).truncate(true).open("../../rbf-trace-experiments/testing/fft/signal.yaml").unwrap();
        // serde_yaml::to_writer(file1, &signal).unwrap();

        // FFT
        let sampling_freq = (1f64/resolution.to_s()).round() as u32;
        assert!(sampling_freq >= 1);
        let mut real_planner = RealFftPlanner::<f32>::new();
        let r2c = real_planner.plan_fft_forward(signal_len);
        let mut spectrum = r2c.make_output_vec();
        assert_eq!(spectrum.len(), signal_len/2+1);
        r2c.process(&mut signal, &mut spectrum).unwrap();

        // Process FFT result
        let mut fft_result = Vec::<(f32, f32)>::new(); // Frequency => Power
        let mut max_power = 0f32; // Used for normalization
        let mut spikes: Vec<Time> = Vec::new(); // Candidate periods

        // Skip frequency 0
        for i in 1..spectrum.len() {
            let freq_bin = i as f32 * sampling_freq as f32 / signal_len as f32; // Compute frequency bin
            let power_norm = spectrum[i].norm_sqr();
            fft_result.push((freq_bin, power_norm));
            max_power = max_power.max(power_norm);
        }
        // Normalize powers between 0 and 1
        for i in 0..fft_result.len() {
            fft_result[i].1 = fft_result[i].1/max_power as f32;
        }
        // Find spikes
        for i in 0..fft_result.len() {
            if fft_result[i].1 >= self.fft_filter_cutoff {
                // println!("Spike {:#?} sec : {:#?}", 1f32/fft_result[i].0, fft_result[i].1); // TODO show spikes
                spikes.push(Time::from_s((1f32/fft_result[i].0) as f64));
            }
        }

        // TODO Debug: dump transform
        // let mut dump = Vec::new();
        // for (fr, fr_val) in fft_result {
        //     dump.push((fr, fr_val));
        // }
        // let file2 = OpenOptions::new().write(true).truncate(true).open("../../rbf-trace-experiments/testing/fft/transform.yaml").unwrap();
        // serde_yaml::to_writer(file2, &dump).unwrap();

        /* Check if trace is non-periodic */
        if spikes.len() == 0 {
            return Time::zero();
        }
        if spikes.len() > 1 {
            // Safety check: possible aliasing
            let leftmost_spike = spikes[0];
            for i in 1..(5.min(spikes.len())) { // Look at the next 4 spikes
                let ratio = (leftmost_spike.to_ns() as f32 / spikes[i].to_ns() as f32).round() as u32;
                if ratio != (i+1) as u32 {
                    return Time::zero();
                }
            }
        }

        spikes[0].round(resolution)
    }

    fn push_job(&mut self, job: Job) {
        if !self.job_history.is_empty() {
            let last_gap = job.arrived_at - self.job_history.get(-1).unwrap().arrived_at;
            if self.min_gap > Time::zero() {
                self.min_gap = self.min_gap.min(last_gap);
            } else {
                self.min_gap = last_gap;
            }
        }
        self.job_history.push(job);
    }
}

impl TaskModelExtractor for SpectralExtractor {
    type Model = PeriodicSelfSuspendingTask;
    type Params = SpectralExtractionParams;

    fn from_params(params: &Self::Params) -> Self {
        Self::new(params.max_signal_len, params.window_size, params.fft_filter_cutoff)
    }

    fn is_matching(&self) -> bool {
        self.still_periodic
    }

    /// Returns true if the model could have changed.
    fn push_event(&mut self, event: TraceEvent) -> bool {
        let maybe_job = self.job_detector.push_event(&event);

        if let Some(job) = maybe_job {
            self.push_job(job)
        }

        maybe_job.is_some()
    }

    /// Triggers the model extraction and returns the model.
    fn extract_model(&mut self) -> Option<Self::Model> {
        self.extract();
        self.current_model.clone()
    }
}

#[cfg(test)]
mod test_period {
    use rbftrace_core::{time::Time, trace::{Trace, TraceEvent}, model::PeriodicTask};
    use crate::spectral::{SpectralExtractor, TaskModelExtractor};

    const MAX_SIGNAL_LEN: usize = 1_000_000;
    const WINDOW_SIZE: usize = 1_000;
    const FFT_FILTER_CUTOFF: f32 = 0.5;

    #[test]
    fn periodic_no_ss_perfect() {
        let trace = Trace::from([
            TraceEvent::activation(0, Time::from_s(5.0)),
            TraceEvent::dispatch(0, Time::from_s(5.0)),
            TraceEvent::deactivation(0, Time::from_s(7.0)),
            
            TraceEvent::activation(0, Time::from_s(15.0)),
            TraceEvent::dispatch(0, Time::from_s(15.0)),
            TraceEvent::deactivation(0, Time::from_s(18.0)),
            
            TraceEvent::activation(0, Time::from_s(25.0)),
            TraceEvent::dispatch(0,Time::from_s(25.0)),
            TraceEvent::deactivation(0, Time::from_s(26.0)),
            
            TraceEvent::activation(0, Time::from_s(35.0)),
            TraceEvent::dispatch(0, Time::from_s(35.0)),
            TraceEvent::deactivation(0, Time::from_s(37.0)),
            
            TraceEvent::activation(0, Time::from_s(45.0)),
            TraceEvent::dispatch(0, Time::from_s(45.0)),
            TraceEvent::deactivation(0, Time::from_s(48.0)),
            
            TraceEvent::activation(0, Time::from_s(55.0) ),
            TraceEvent::dispatch(0,Time::from_s(55.0) ),
            TraceEvent::deactivation(0, Time::from_s(56.0)),
            
            TraceEvent::activation(0, Time::from_s(65.0)),
            TraceEvent::dispatch(0, Time::from_s(65.0)),
            TraceEvent::deactivation(0, Time::from_s(67.0)),
            
            TraceEvent::activation(0, Time::from_s(75.0)),
            TraceEvent::dispatch(0, Time::from_s(75.0)),
            TraceEvent::deactivation(0, Time::from_s(78.0)),
            
            TraceEvent::activation(0, Time::from_s(85.0) ),
            TraceEvent::dispatch(0,Time::from_s(85.0) ),
            TraceEvent::deactivation(0, Time::from_s(86.0)),
            
            TraceEvent::activation(0, Time::from_s(95.0) ),
            TraceEvent::dispatch(0,Time::from_s(95.0) ),
            TraceEvent::deactivation(0, Time::from_s(96.0)),
            
            TraceEvent::activation(0, Time::from_s(105.0) ),
            TraceEvent::dispatch(0,Time::from_s(105.0) ),
            TraceEvent::deactivation(0, Time::from_s(106.0))
        ]);

        let mut extractor = SpectralExtractor::new(MAX_SIGNAL_LEN, WINDOW_SIZE, FFT_FILTER_CUTOFF);
        for event in trace.events() {
            extractor.push_event(*event);
        }
        let model = extractor.extract_model();

        assert_eq!(
            model.unwrap().period,
            Time::from_s(10.0),
        );
        assert!(extractor.is_matching());
    }

    #[test]
    fn periodic_no_ss_jitter() {
        let trace = Trace::from([
            TraceEvent::activation(0, Time::from_ms(5.5)),
            TraceEvent::dispatch(0, Time::from_ms(5.5)),
            TraceEvent::deactivation(0, Time::from_ms(7.5)),
            
            TraceEvent::activation(0, Time::from_ms(15.3)),
            TraceEvent::dispatch(0, Time::from_ms(15.3)),
            TraceEvent::deactivation(0, Time::from_ms(18.3)),
            
            TraceEvent::activation(0, Time::from_ms(25.0)),
            TraceEvent::dispatch(0,Time::from_ms(25.0)),
            TraceEvent::deactivation(0, Time::from_ms(26.0)),
            
            TraceEvent::activation(0, Time::from_ms(35.5)),
            TraceEvent::dispatch(0, Time::from_ms(35.5)),
            TraceEvent::deactivation(0, Time::from_ms(37.5)),
            
            TraceEvent::activation(0, Time::from_ms(45.3)),
            TraceEvent::dispatch(0, Time::from_ms(45.3)),
            TraceEvent::deactivation(0, Time::from_ms(48.3)),
            
            TraceEvent::activation(0, Time::from_ms(55.0) ),
            TraceEvent::dispatch(0,Time::from_ms(55.0) ),
            TraceEvent::deactivation(0, Time::from_ms(56.0)),
            
            TraceEvent::activation(0, Time::from_ms(65.5)),
            TraceEvent::dispatch(0, Time::from_ms(65.5)),
            TraceEvent::deactivation(0, Time::from_ms(67.5)),
            
            TraceEvent::activation(0, Time::from_ms(75.3)),
            TraceEvent::dispatch(0, Time::from_ms(75.3)),
            TraceEvent::deactivation(0, Time::from_ms(78.3)),
            
            TraceEvent::activation(0, Time::from_ms(85.0) ),
            TraceEvent::dispatch(0,Time::from_ms(85.0) ),
            TraceEvent::deactivation(0, Time::from_ms(86.0)),
            
            TraceEvent::activation(0, Time::from_ms(95.0) ),
            TraceEvent::dispatch(0,Time::from_ms(95.0) ),
            TraceEvent::deactivation(0, Time::from_ms(96.0)),
            
            TraceEvent::activation(0, Time::from_ms(105.0) ),
            TraceEvent::dispatch(0,Time::from_ms(105.0) ),
            TraceEvent::deactivation(0, Time::from_ms(106.0))
        ]);

        let mut extractor = SpectralExtractor::new(MAX_SIGNAL_LEN, WINDOW_SIZE, FFT_FILTER_CUTOFF);
        for event in trace.events() {
            extractor.push_event(*event);
        }
        let model = extractor.extract_model();

        assert_eq!(
            model.unwrap().period,
            Time::from_ms(10.0),
        );
        assert!(extractor.is_matching());
    }

    #[test]
    fn periodic_ss() {
        let trace = Trace::from([
            TraceEvent::activation(0, Time::from_s(5.5)),
            TraceEvent::dispatch(0, Time::from_s(5.5)),
            TraceEvent::deactivation(0, Time::from_s(7.5)),

            // SS
            TraceEvent::activation(0, Time::from_ms(5550.0)),
            TraceEvent::dispatch(0, Time::from_ms(5550.0)),
            TraceEvent::deactivation(0, Time::from_ms(5600.0)),
            
            TraceEvent::activation(0, Time::from_s(15.3)),
            TraceEvent::dispatch(0, Time::from_s(15.3)),
            TraceEvent::deactivation(0, Time::from_s(18.3)),
            
            TraceEvent::activation(0, Time::from_s(25.0)),
            TraceEvent::dispatch(0,Time::from_s(25.0)),
            TraceEvent::deactivation(0, Time::from_s(26.0)),

            // SS
            TraceEvent::activation(0, Time::from_ms(25070.0)),
            TraceEvent::dispatch(0, Time::from_ms(25070.0)),
            TraceEvent::deactivation(0, Time::from_ms(25100.0)),
            
            TraceEvent::activation(0, Time::from_s(35.5)),
            TraceEvent::dispatch(0, Time::from_s(35.5)),
            TraceEvent::deactivation(0, Time::from_s(37.5)),
            
            TraceEvent::activation(0, Time::from_s(45.3)),
            TraceEvent::dispatch(0, Time::from_s(45.3)),
            TraceEvent::deactivation(0, Time::from_s(48.3)),
            
            TraceEvent::activation(0, Time::from_s(55.0) ),
            TraceEvent::dispatch(0,Time::from_s(55.0) ),
            TraceEvent::deactivation(0, Time::from_s(56.0)),

            // SS
            TraceEvent::activation(0, Time::from_ms(55100.0)),
            TraceEvent::dispatch(0, Time::from_ms(55100.0)),
            TraceEvent::deactivation(0, Time::from_ms(55200.0)),
            
            TraceEvent::activation(0, Time::from_s(65.5)),
            TraceEvent::dispatch(0, Time::from_s(65.5)),
            TraceEvent::deactivation(0, Time::from_s(67.5)),

            // SS
            TraceEvent::activation(0, Time::from_ms(65550.0)),
            TraceEvent::dispatch(0, Time::from_ms(65550.0)),
            TraceEvent::deactivation(0, Time::from_ms(65650.0)),
            
            TraceEvent::activation(0, Time::from_s(75.3)),
            TraceEvent::dispatch(0, Time::from_s(75.3)),
            TraceEvent::deactivation(0, Time::from_s(78.3)),
            
            TraceEvent::activation(0, Time::from_s(85.0) ),
            TraceEvent::dispatch(0,Time::from_s(85.0) ),
            TraceEvent::deactivation(0, Time::from_s(86.0)),
            
            TraceEvent::activation(0, Time::from_s(95.0) ),
            TraceEvent::dispatch(0,Time::from_s(95.0) ),
            TraceEvent::deactivation(0, Time::from_s(96.0)),

            // SS
            TraceEvent::activation(0, Time::from_ms(95055.0)),
            TraceEvent::dispatch(0, Time::from_ms(95055.0)),
            TraceEvent::deactivation(0, Time::from_ms(95100.0)),
            
            TraceEvent::activation(0, Time::from_s(105.0) ),
            TraceEvent::dispatch(0,Time::from_s(105.0) ),
            TraceEvent::deactivation(0, Time::from_s(106.0))
        ]);

        let mut extractor = SpectralExtractor::new(MAX_SIGNAL_LEN, WINDOW_SIZE, FFT_FILTER_CUTOFF);
        for event in trace.events() {
            extractor.push_event(*event);
        }
        let period = extractor.extract_model().unwrap().period;
        let error = Time::from_s(0.05);

        assert!(period <= Time::from_s(10.0) + error && period >= Time::from_s(10.0) - error);
        assert!(extractor.is_matching());
    }

    #[test]
    fn periodic_ss_burst() {
        let trace = Trace::from([
            // Burst
            TraceEvent::activation(0, Time::from_ms(0.5)),
            TraceEvent::dispatch(0, Time::from_ms(0.5)),
            TraceEvent::deactivation(0, Time::from_ms(0.6)),

            TraceEvent::activation(0, Time::from_ms(1.5)),
            TraceEvent::dispatch(0, Time::from_ms(1.5)),
            TraceEvent::deactivation(0, Time::from_ms(1.6)),
            
            TraceEvent::activation(0, Time::from_ms(2.5)),
            TraceEvent::dispatch(0, Time::from_ms(2.5)),
            TraceEvent::deactivation(0, Time::from_ms(2.6)),

            // Burst
            TraceEvent::activation(0, Time::from_ms(10.5)),
            TraceEvent::dispatch(0, Time::from_ms(10.5)),
            TraceEvent::deactivation(0, Time::from_ms(10.6)),

            TraceEvent::activation(0, Time::from_ms(11.5)),
            TraceEvent::dispatch(0, Time::from_ms(11.5)),
            TraceEvent::deactivation(0, Time::from_ms(11.6)),
            
            TraceEvent::activation(0, Time::from_ms(12.5)),
            TraceEvent::dispatch(0, Time::from_ms(12.5)),
            TraceEvent::deactivation(0, Time::from_ms(12.6)),

            // Burst
            TraceEvent::activation(0, Time::from_ms(20.5)),
            TraceEvent::dispatch(0, Time::from_ms(20.5)),
            TraceEvent::deactivation(0, Time::from_ms(20.6)),

            TraceEvent::activation(0, Time::from_ms(21.5)),
            TraceEvent::dispatch(0, Time::from_ms(21.5)),
            TraceEvent::deactivation(0, Time::from_ms(21.6)),
            
            TraceEvent::activation(0, Time::from_ms(22.5)),
            TraceEvent::dispatch(0, Time::from_ms(22.5)),
            TraceEvent::deactivation(0, Time::from_ms(22.6)),

            // Burst
            TraceEvent::activation(0, Time::from_ms(30.5)),
            TraceEvent::dispatch(0, Time::from_ms(30.5)),
            TraceEvent::deactivation(0, Time::from_ms(30.6)),

            TraceEvent::activation(0, Time::from_ms(31.5)),
            TraceEvent::dispatch(0, Time::from_ms(31.5)),
            TraceEvent::deactivation(0, Time::from_ms(31.6)),
            
            TraceEvent::activation(0, Time::from_ms(32.5)),
            TraceEvent::dispatch(0, Time::from_ms(32.5)),
            TraceEvent::deactivation(0, Time::from_ms(32.6)),

            // Burst
            TraceEvent::activation(0, Time::from_ms(41.5)),
            TraceEvent::dispatch(0, Time::from_ms(41.5)),
            TraceEvent::deactivation(0, Time::from_ms(41.6)),

            TraceEvent::activation(0, Time::from_ms(42.5)),
            TraceEvent::dispatch(0, Time::from_ms(42.5)),
            TraceEvent::deactivation(0, Time::from_ms(42.6)),
            
            TraceEvent::activation(0, Time::from_ms(44.5)),
            TraceEvent::dispatch(0, Time::from_ms(44.5)),
            TraceEvent::deactivation(0, Time::from_ms(44.6)),

            // Burst
            TraceEvent::activation(0, Time::from_ms(51.5)),
            TraceEvent::dispatch(0, Time::from_ms(51.5)),
            TraceEvent::deactivation(0, Time::from_ms(51.6)),

            TraceEvent::activation(0, Time::from_ms(52.5)),
            TraceEvent::dispatch(0, Time::from_ms(52.5)),
            TraceEvent::deactivation(0, Time::from_ms(52.6)),
            
            TraceEvent::activation(0, Time::from_ms(52.5)),
            TraceEvent::dispatch(0, Time::from_ms(52.5)),
            TraceEvent::deactivation(0, Time::from_ms(52.6)),

            // Burst
            TraceEvent::activation(0, Time::from_ms(61.5)),
            TraceEvent::dispatch(0, Time::from_ms(61.5)),
            TraceEvent::deactivation(0, Time::from_ms(61.6)),

            TraceEvent::activation(0, Time::from_ms(62.5)),
            TraceEvent::dispatch(0, Time::from_ms(62.5)),
            TraceEvent::deactivation(0, Time::from_ms(62.6)),
            
            TraceEvent::activation(0, Time::from_ms(63.5)),
            TraceEvent::dispatch(0, Time::from_ms(63.5)),
            TraceEvent::deactivation(0, Time::from_ms(63.6)),
        ]);

        let mut extractor = SpectralExtractor::new(MAX_SIGNAL_LEN, WINDOW_SIZE, FFT_FILTER_CUTOFF);
        for event in trace.events() {
            extractor.push_event(*event);
        }
        let period = extractor.extract_model().unwrap().period;
        let error = Time::from_ms(1.0);

        assert!(period <= Time::from_ms(10.0) + error && period >= Time::from_ms(10.0) - error);
        assert!(extractor.is_matching()); 
    }

    // Shorter trace causes aliasing
    #[test]
    fn periodic_ss_burst_aliasing() {
        let trace = Trace::from([
            // Burst
            TraceEvent::activation(0, Time::from_ms(0.5)),
            TraceEvent::dispatch(0, Time::from_ms(0.5)),
            TraceEvent::deactivation(0, Time::from_ms(0.6)),

            TraceEvent::activation(0, Time::from_ms(1.5)),
            TraceEvent::dispatch(0, Time::from_ms(1.5)),
            TraceEvent::deactivation(0, Time::from_ms(1.6)),
            
            TraceEvent::activation(0, Time::from_ms(2.5)),
            TraceEvent::dispatch(0, Time::from_ms(2.5)),
            TraceEvent::deactivation(0, Time::from_ms(2.6)),

            // Burst
            TraceEvent::activation(0, Time::from_ms(10.5)),
            TraceEvent::dispatch(0, Time::from_ms(10.5)),
            TraceEvent::deactivation(0, Time::from_ms(10.6)),

            TraceEvent::activation(0, Time::from_ms(11.5)),
            TraceEvent::dispatch(0, Time::from_ms(11.5)),
            TraceEvent::deactivation(0, Time::from_ms(11.6)),
            
            TraceEvent::activation(0, Time::from_ms(12.5)),
            TraceEvent::dispatch(0, Time::from_ms(12.5)),
            TraceEvent::deactivation(0, Time::from_ms(12.6)),

            // Burst
            TraceEvent::activation(0, Time::from_ms(20.5)),
            TraceEvent::dispatch(0, Time::from_ms(20.5)),
            TraceEvent::deactivation(0, Time::from_ms(20.6)),

            TraceEvent::activation(0, Time::from_ms(21.5)),
            TraceEvent::dispatch(0, Time::from_ms(21.5)),
            TraceEvent::deactivation(0, Time::from_ms(21.6)),
            
            TraceEvent::activation(0, Time::from_ms(22.5)),
            TraceEvent::dispatch(0, Time::from_ms(22.5)),
            TraceEvent::deactivation(0, Time::from_ms(22.6)),

            // Burst
            TraceEvent::activation(0, Time::from_ms(30.5)),
            TraceEvent::dispatch(0, Time::from_ms(30.5)),
            TraceEvent::deactivation(0, Time::from_ms(30.6)),

            TraceEvent::activation(0, Time::from_ms(31.5)),
            TraceEvent::dispatch(0, Time::from_ms(31.5)),
            TraceEvent::deactivation(0, Time::from_ms(31.6)),
            
            TraceEvent::activation(0, Time::from_ms(32.5)),
            TraceEvent::dispatch(0, Time::from_ms(32.5)),
            TraceEvent::deactivation(0, Time::from_ms(32.6)),
        ]);

        let mut extractor = SpectralExtractor::new(MAX_SIGNAL_LEN, WINDOW_SIZE, FFT_FILTER_CUTOFF);
        for event in trace.events() {
            extractor.push_event(*event);
        }
        let period = extractor.extract_model().unwrap().period;
        let error = Time::from_ms(1.0);

        assert!(period <= Time::from_ms(10.0) + error && period >= Time::from_ms(10.0) - error);
        assert!(extractor.is_matching()); 
    }

    #[test]
    pub fn fail_on_sporadic(){
        // [    0  9809 10970 18269 23135 31576 33085 35973 42330 45267 49278 57180]
        let trace = Trace::from([
            TraceEvent::activation(0, Time::from_ms(1.0) ),
            TraceEvent::dispatch(0, Time::from_ms(1.0) ),
            TraceEvent::deactivation(0, Time::from_ms(1.1) ),

            TraceEvent::activation(0, Time::from_ms(9809.0) ),
            TraceEvent::dispatch(0, Time::from_ms(9809.0) ),
            TraceEvent::deactivation(0, Time::from_ms(9809.1) ),

            TraceEvent::activation(0, Time::from_ms(10970.0) ),
            TraceEvent::dispatch(0, Time::from_ms(10970.0) ),
            TraceEvent::deactivation(0, Time::from_ms(10970.1) ),

            TraceEvent::activation(0, Time::from_ms(18269.0) ),
            TraceEvent::dispatch(0, Time::from_ms(18269.0) ),
            TraceEvent::deactivation(0, Time::from_ms(18269.1) ),

            TraceEvent::activation(0, Time::from_ms(23135.0) ),
            TraceEvent::dispatch(0, Time::from_ms(23135.0) ),
            TraceEvent::deactivation(0, Time::from_ms(23135.1) ),

            TraceEvent::activation(0, Time::from_ms(31576.0) ),
            TraceEvent::dispatch(0, Time::from_ms(31576.0) ),
            TraceEvent::deactivation(0, Time::from_ms(31576.1) ),

            TraceEvent::activation(0, Time::from_ms(33085.0) ),
            TraceEvent::dispatch(0, Time::from_ms(33085.0) ),
            TraceEvent::deactivation(0, Time::from_ms(33085.1) ),

            TraceEvent::activation(0, Time::from_ms(35973.0) ),
            TraceEvent::dispatch(0, Time::from_ms(35973.0) ),
            TraceEvent::deactivation(0, Time::from_ms(35973.1) ),

            TraceEvent::activation(0, Time::from_ms(42330.0) ),
            TraceEvent::dispatch(0, Time::from_ms(42330.0) ),
            TraceEvent::deactivation(0, Time::from_ms(42330.1) ),

            TraceEvent::activation(0, Time::from_ms(45267.0) ),
            TraceEvent::dispatch(0, Time::from_ms(45267.0) ),
            TraceEvent::deactivation(0, Time::from_ms(45267.1) ),

            TraceEvent::activation(0, Time::from_ms(49278.0) ),
            TraceEvent::dispatch(0, Time::from_ms(49278.0) ),
            TraceEvent::deactivation(0, Time::from_ms(49278.1) ),

            TraceEvent::activation(0, Time::from_ms(57180.0) ),
            TraceEvent::dispatch(0, Time::from_ms(57180.0) ),
            TraceEvent::deactivation(0, Time::from_ms(57180.1) ),
        ]);

        let mut extractor = SpectralExtractor::new(MAX_SIGNAL_LEN, WINDOW_SIZE, FFT_FILTER_CUTOFF);
        for event in trace.events() {
            extractor.push_event(*event);
        }
        let model = extractor.extract_model();

        assert_eq!(
            model,
            None,
        );
        assert!(!extractor.is_matching());
    }

    // Test signal size over the limit
    #[test]
    fn periodic_ss_long() {
        let mut trace = Trace::new();
        for s in 0..MAX_SIGNAL_LEN {
            trace.push(TraceEvent::activation(0, Time::from_s(s as f64)));
            trace.push(TraceEvent::dispatch(0, Time::from_s(s as f64)));
            trace.push(TraceEvent::deactivation(0, Time::from_s(s as f64)));
        }
        // To higher the sampling rate
        let t = Time::from_s((MAX_SIGNAL_LEN-1) as f64) + Time::from_ms(10f64);
        trace.push(TraceEvent::activation(0, t));
        trace.push(TraceEvent::dispatch(0, t));
        trace.push(TraceEvent::deactivation(0, t + Time::from_ms(10f64)));
        
        // Signal will be cropped
        let max_len = MAX_SIGNAL_LEN/4;
        let mut extractor = SpectralExtractor::new(max_len, WINDOW_SIZE, FFT_FILTER_CUTOFF);
        for event in trace.events() {
            extractor.push_event(*event);
        }
        let model = extractor.extract_model();

        assert_eq!(
            model.unwrap().period,
            Time::from_s(1.0),
        );
        assert!(extractor.is_matching());
    }
}

// TODO
#[cfg(test)]
mod test_self_suspensions {

}