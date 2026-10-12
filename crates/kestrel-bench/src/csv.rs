//! PresentMon's CSV → frames. Columns are found by name: their set and order changed between 2.x releases.
//! PresentMon writes a UTF-8 BOM and `NA` where a value doesn't apply.

/// One presented frame.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Frame {
    /// Milliseconds since the previous present (`MsBetweenPresents`): the frame time.
    pub ms: f64,
    /// `MsGPUBusy`: how long the GPU worked on this frame.
    pub gpu_busy_ms: Option<f64>,
    /// `MsPCLatency`: NVIDIA Reflex PC latency, when the game reports it.
    pub pc_latency_ms: Option<f64>,
    /// `PresentMode`, e.g. "Hardware: Independent Flip" or "Composed: Flip".
    pub present_mode: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CsvError {
    /// No header line at all.
    Empty,
    MissingColumn(&'static str),
}

pub fn parse(text: &str) -> Result<Vec<Frame>, CsvError> {
    let text = text.strip_prefix('\u{feff}').unwrap_or(text);
    let mut lines = text.lines().filter(|line| !line.trim().is_empty());
    let header = lines.next().ok_or(CsvError::Empty)?;
    let columns: Vec<&str> = header.split(',').map(str::trim).collect();
    let find = |name: &str| columns.iter().position(|c| c.eq_ignore_ascii_case(name));
    let ms_column =
        find("MsBetweenPresents").ok_or(CsvError::MissingColumn("MsBetweenPresents"))?;
    let gpu_column = find("MsGPUBusy");
    let latency_column = find("MsPCLatency");
    let mode_column = find("PresentMode");

    let number = |fields: &[&str], column: Option<usize>| -> Option<f64> {
        fields
            .get(column?)?
            .trim()
            .parse::<f64>()
            .ok()
            .filter(|v| v.is_finite())
    };
    let mut frames = Vec::new();
    for line in lines {
        let fields: Vec<&str> = line.split(',').collect();
        let Some(ms) = number(&fields, Some(ms_column)).filter(|ms| *ms > 0.0) else {
            continue;
        };
        frames.push(Frame {
            ms,
            gpu_busy_ms: number(&fields, gpu_column).filter(|v| *v >= 0.0),
            pc_latency_ms: number(&fields, latency_column).filter(|v| *v > 0.0),
            present_mode: mode_column
                .and_then(|c| fields.get(c))
                .map(|mode| mode.trim().to_string())
                .unwrap_or_default(),
        });
    }
    Ok(frames)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// PresentMon 2.6.0's default columns (verified against its source and test files).
    const HEADER: &str = "Application,ProcessID,SwapChainAddress,PresentRuntime,SyncInterval,PresentFlags,AllowsTearing,PresentMode,TimeInMs,MsBetweenSimulationStart,MsBetweenPresents,MsBetweenDisplayChange,MsInPresentAPI,MsRenderPresentLatency,MsUntilDisplayed,MsPCLatency,CPUStartTimeInMs,MsBetweenAppStart,MsCPUBusy,MsCPUWait,MsGPULatency,MsGPUTime,MsGPUBusy,MsGPUWait,MsAnimationError,AnimationTime,MsFlipDelay,MsAllInputToPhotonLatency,MsClickToPhotonLatency";

    fn row(ms: &str, gpu: &str, latency: &str, mode: &str) -> String {
        HEADER
            .split(',')
            .map(|column| match column {
                "Application" => "VALORANT-Win64-Shipping.exe",
                "PresentMode" => mode,
                "MsBetweenPresents" => ms,
                "MsGPUBusy" => gpu,
                "MsPCLatency" => latency,
                _ => "NA",
            })
            .collect::<Vec<_>>()
            .join(",")
    }

    #[test]
    fn parses_frames_by_column_name() {
        let text = format!(
            "\u{feff}{HEADER}\n{}\r\n{}\n",
            row("2.5", "2.3", "9.8", "Hardware: Independent Flip"),
            row("3.0", "NA", "NA", "Composed: Flip")
        );
        let frames = parse(&text).unwrap();
        assert_eq!(frames.len(), 2);
        assert_eq!(
            frames[0],
            Frame {
                ms: 2.5,
                gpu_busy_ms: Some(2.3),
                pc_latency_ms: Some(9.8),
                present_mode: "Hardware: Independent Flip".into()
            }
        );
        assert_eq!(
            frames[1],
            Frame {
                ms: 3.0,
                gpu_busy_ms: None,
                pc_latency_ms: None,
                present_mode: "Composed: Flip".into()
            }
        );
    }

    #[test]
    fn reordered_and_missing_optional_columns_still_parse() {
        let frames = parse("PresentMode,MsBetweenPresents\nHardware: Legacy Flip,4.0\n").unwrap();
        assert_eq!(
            frames,
            vec![Frame {
                ms: 4.0,
                gpu_busy_ms: None,
                pc_latency_ms: None,
                present_mode: "Hardware: Legacy Flip".into()
            }]
        );
    }

    #[test]
    fn rows_without_a_usable_frame_time_are_skipped() {
        let text = format!(
            "{HEADER}\n{}\n{}\n{}\n{}\n{}\n",
            row("NA", "1", "1", "x"),
            row("0", "1", "1", "x"),
            row("-2", "1", "1", "x"),
            row("abc", "1", "1", "x"),
            row("5", "1", "1", "x")
        );
        let frames = parse(&text).unwrap();
        assert_eq!(frames.len(), 1);
        assert_eq!(frames[0].ms, 5.0);
    }

    #[test]
    fn a_missing_frame_time_column_or_empty_text_is_an_error() {
        assert_eq!(
            parse("Application,PresentMode\nx,y\n"),
            Err(CsvError::MissingColumn("MsBetweenPresents"))
        );
        assert_eq!(parse(""), Err(CsvError::Empty));
        assert_eq!(parse("\u{feff}\n\n"), Err(CsvError::Empty));
        assert_eq!(parse(&format!("{HEADER}\n")), Ok(Vec::new()));
    }
}
