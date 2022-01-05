#![allow(non_upper_case_globals)]
#![allow(non_camel_case_types)]
#![allow(non_snake_case)]
include!(concat!(env!("OUT_DIR"), "/bindings.rs"));

use std::ffi::CString;
use std::mem::MaybeUninit;
use std::ptr;

// TODO should check for errors

/***** BINARY PARSER FUNCTIONS *****/

pub fn init_recorders(tracefs: *mut tracefs_instance, cpu_cnt: i32) -> *mut recorder_data {
    unsafe {
       return rbftrace_create_recorders(tracefs, cpu_cnt);
    }
}

pub fn read_stream_raw(recorders: *mut recorder_data, cpu_cnt: i32) -> Option<rbftrace_event_raw> {
    unsafe {
        let mut event = MaybeUninit::<rbftrace_event_raw>::uninit();
        let ret = rbftrace_read_stream(recorders, cpu_cnt, event.as_mut_ptr());

        if ret > 0 {
            Some(event.assume_init())
        } else {
            None
        }
    }
}

pub fn stop_recorder_threads(recorders: *mut recorder_data, cpu_cnt: i32) {
    unsafe {
        rbftrace_stop_threads(recorders, cpu_cnt);
    }
}

pub fn wait_recorder_threads(recorders: *mut recorder_data, cpu_cnt: i32) {
    unsafe {
        rbftrace_wait_threads(recorders, cpu_cnt);
    }
}

/***** LIBTRACEFS *****/

/* Enable */

pub fn start_tracing(tracefs: *mut tracefs_instance) {
    unsafe {
        tracefs_instance_file_write(tracefs, c_str("tracing_on").as_ptr(), c_str("1").as_ptr());
    }
}

pub fn set_events(tracefs: *mut tracefs_instance) {
    unsafe {
        tracefs_instance_file_write(tracefs, c_str("events/sched/sched_wakeup/enable").as_ptr(), c_str("1").as_ptr());
        tracefs_instance_file_write(tracefs, c_str("events/sched/sched_wakeup_new/enable").as_ptr(), c_str("1").as_ptr());
        tracefs_instance_file_write(tracefs, c_str("events/sched/sched_switch/enable").as_ptr(), c_str("1").as_ptr());
        tracefs_instance_file_write(tracefs, c_str("events/sched/sched_process_exit/enable").as_ptr(), c_str("1").as_ptr());
    }
}

pub fn set_pids(tracefs: *mut tracefs_instance, pids: &Vec<u32>) {
    let mut pids_str = String::new();
    for pid in pids {
        pids_str.push_str(&pid.to_string());
        pids_str.push_str(" ");
    }

    unsafe {
        tracefs_instance_file_write(tracefs, c_str("set_event_pid").as_ptr(), c_str(&pids_str).as_ptr());
    }
}

pub fn set_buffer_size(tracefs: *mut tracefs_instance, bufsize: u32) {
    unsafe {
        tracefs_instance_file_write(tracefs, c_str("buffer_size_kb").as_ptr(), c_str(&bufsize.to_string()).as_ptr());
    }
}

pub fn set_event_fork(tracefs: *mut tracefs_instance) {
    unsafe {
        tracefs_instance_file_write(tracefs, c_str("options/event-fork").as_ptr(), c_str("1").as_ptr());
    }
}

pub fn set_monotonic_clock(tracefs: *mut tracefs_instance) {
    unsafe {
        tracefs_instance_file_write(tracefs, c_str("trace_clock").as_ptr(), c_str("mono").as_ptr());
    }
}

pub fn create_tracefs() -> *mut tracefs_instance {
    unsafe {
        return tracefs_instance_create(ptr::null());
    }
}

/* Disable */

pub fn stop_tracing(tracefs: *mut tracefs_instance) {
    unsafe {
        tracefs_instance_file_write(tracefs, c_str("tracing_on").as_ptr(), c_str("0").as_ptr());
    }
}

pub fn clear_events(tracefs: *mut tracefs_instance) {
    unsafe {
        tracefs_instance_file_clear(tracefs, c_str("set_event").as_ptr());
    }
}

pub fn clear_pids(tracefs: *mut tracefs_instance) {
    unsafe {
        tracefs_instance_file_clear(tracefs, c_str("set_event_pid").as_ptr());
    }
}

pub fn clear_trace(tracefs: *mut tracefs_instance) {
    unsafe {
        tracefs_instance_file_clear(tracefs, c_str("trace").as_ptr());
    }
}

pub fn clear_event_fork(tracefs: *mut tracefs_instance) {
    unsafe {
        tracefs_instance_file_write(tracefs, c_str("options/event-fork").as_ptr(), c_str("0").as_ptr());
    }
}

pub fn destroy_tracefs(tracefs: *mut tracefs_instance) {
    unsafe {
        tracefs_instance_destroy(tracefs);
    }
}

/* Read */

pub fn get_tracing_dir() -> String {
    unsafe {
        let dir = tracefs_tracing_dir();
        return (*dir).to_string();
    }
}

pub fn get_clock(tracefs: *mut tracefs_instance) -> String {
    unsafe {
        let clock_str = tracefs_get_clock(tracefs);
        return (*clock_str).to_string();
    }
}

pub fn get_buffer_size(tracefs: *mut tracefs_instance) -> u32 {
    unsafe {
        let mut bufsize: std::os::raw::c_longlong = 0;
        tracefs_instance_file_read_number(tracefs, c_str("buffer_size_kb").as_ptr(), &mut bufsize);

        return bufsize as u32;
    }
}

pub fn get_event_id(tracefs: *mut tracefs_instance, event: &str) -> u16 {
    let path = format!("events/sched/{}/id", event);
    
    unsafe {
        let mut id: std::os::raw::c_longlong = 0;
        tracefs_instance_file_read_number(tracefs, c_str(&path).as_ptr(), &mut id);
        
        return id as u16;
    }
}

/***** HELPERS *****/

fn c_str(s: &str) -> CString {
    CString::new(s).unwrap()
}
