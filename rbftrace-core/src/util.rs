use std::process::Command;

pub fn run_cmd(cmd: String) -> String {
    let out = Command::new("sh").arg("-c").arg(cmd).output().expect("Failed to run command");
    return String::from_utf8_lossy(&out.stdout).to_string();
}

pub fn run_cmd_opt(cmd: String) -> Option<String> {
    match Command::new("sh").arg("-c").arg(cmd).output() {
        Ok(out) => { 
            let s = String::from_utf8_lossy(&out.stdout).to_string();
            if s.is_empty() {
                None
            }
            else {
                Some(s)
            }
        },
        Err(_) => None 
    }
}
