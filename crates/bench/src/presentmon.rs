use crate::error::BenchError;

#[derive(Debug, Clone, PartialEq)]
pub struct FramePacing {
    pub frame_time_ms_mean: f64,
    pub dropped_frames: u64,
    pub frame_count: u64,
}

/// Parses a PresentMon CSV export. Frame pacing stays empty when PresentMon
/// is not installed and no CSV was captured.
pub fn parse_presentmon_csv(csv: &str) -> Result<FramePacing, BenchError> {
    let mut lines = csv.lines().filter(|line| !line.trim().is_empty());
    let header = lines
        .next()
        .ok_or_else(|| BenchError::Parse("PresentMon CSV is empty".to_string()))?;
    let columns = split_csv_line(header);
    let frame_index = column_index(&columns, "MsBetweenPresents")?;
    let dropped_index = columns
        .iter()
        .position(|name| name.eq_ignore_ascii_case("Dropped"));

    let mut frame_count = 0u64;
    let mut frame_sum = 0.0;
    let mut dropped = 0u64;
    for line in lines {
        let fields = split_csv_line(line);
        let Some(raw_frame) = fields.get(frame_index) else {
            continue;
        };
        let frame_time: f64 = raw_frame.parse().map_err(|error| {
            BenchError::Parse(format!("invalid MsBetweenPresents '{raw_frame}': {error}"))
        })?;
        if !frame_time.is_finite() {
            return Err(BenchError::Parse(
                "MsBetweenPresents is not finite".to_string(),
            ));
        }
        frame_sum += frame_time;
        frame_count += 1;
        if let Some(index) = dropped_index {
            if fields
                .get(index)
                .is_some_and(|value| value == "1" || value.eq_ignore_ascii_case("true"))
            {
                dropped += 1;
            }
        }
    }
    if frame_count == 0 {
        return Err(BenchError::Parse(
            "PresentMon CSV has no frame rows".to_string(),
        ));
    }
    Ok(FramePacing {
        frame_time_ms_mean: frame_sum / frame_count as f64,
        dropped_frames: dropped,
        frame_count,
    })
}

pub fn frame_times_from_offsets(offsets_ms: &[u64]) -> Option<f64> {
    let mut previous = None;
    let mut count = 0u64;
    let mut sum = 0.0;
    for offset in offsets_ms {
        if let Some(start) = previous {
            if *offset >= start {
                sum += (*offset - start) as f64;
                count += 1;
            }
        }
        previous = Some(*offset);
    }
    if count == 0 {
        None
    } else {
        Some(sum / count as f64)
    }
}

fn column_index(columns: &[String], name: &str) -> Result<usize, BenchError> {
    columns
        .iter()
        .position(|column| column.eq_ignore_ascii_case(name))
        .ok_or_else(|| BenchError::Parse(format!("PresentMon CSV is missing {name}")))
}

fn split_csv_line(line: &str) -> Vec<String> {
    let mut fields = Vec::new();
    let mut current = String::new();
    let mut in_quotes = false;
    for character in line.chars() {
        match character {
            '"' => in_quotes = !in_quotes,
            ',' if !in_quotes => {
                fields.push(std::mem::take(&mut current));
            }
            _ => current.push(character),
        }
    }
    fields.push(current);
    fields
}

#[cfg(test)]
mod tests {
    use super::{frame_times_from_offsets, parse_presentmon_csv};

    #[test]
    fn parses_frame_times_and_dropped_frames() {
        let csv = "\
Application,MsBetweenPresents,Dropped
app.exe,16.6,0
app.exe,16.8,1
app.exe,16.7,0
";
        let pacing = parse_presentmon_csv(csv).expect("csv");
        assert_eq!(pacing.frame_count, 3);
        assert_eq!(pacing.dropped_frames, 1);
        let expected = (16.6 + 16.8 + 16.7) / 3.0;
        let delta = (pacing.frame_time_ms_mean - expected).abs();
        assert!(delta < 1e-9);
    }

    #[test]
    fn offsets_are_the_timestamp_fallback() {
        assert_eq!(frame_times_from_offsets(&[0, 20, 50]), Some(25.0));
        assert!(frame_times_from_offsets(&[10]).is_none());
    }
}
