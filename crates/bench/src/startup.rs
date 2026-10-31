use std::process::Command;
use std::time::{Duration, Instant};

use crate::error::BenchError;

/// Times process creation, then stops the child so a measurement does not
/// leave the target running.
pub fn time_command(program: &str, args: &[&str]) -> Result<Duration, BenchError> {
    let started = Instant::now();
    let mut child = Command::new(program)
        .args(args)
        .spawn()
        .map_err(|error| BenchError::Message(format!("failed to launch {program}: {error}")))?;
    let elapsed = started.elapsed();
    let _ = child.kill();
    let _ = child.wait();
    Ok(elapsed)
}

#[cfg(all(test, windows))]
mod tests {
    use super::time_command;

    #[test]
    fn cold_start_timer_measures_a_real_process() {
        let elapsed = time_command("cmd", &["/c", "exit", "0"]).expect("launch");
        assert!(elapsed.as_millis() < 10_000);
    }
}
