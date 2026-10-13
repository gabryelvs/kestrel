//! The beeps that pace a run (spec "What the owner sees", 3), through Windows' Beep. Silent off Windows.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Cue {
    /// One long beep: the capture starts.
    Start,
    /// One short beep at 30 s: switch from the walk to the bots.
    Switch,
    /// Two short beeps: the run is done.
    End,
    /// A short tick, for the fallback countdown.
    Tick,
}

/// (frequency in Hz, duration in ms) for each tone of a cue.
pub fn tones(cue: Cue) -> &'static [(u32, u32)] {
    match cue {
        Cue::Start => &[(880, 600)],
        Cue::Switch => &[(1175, 150)],
        Cue::End => &[(988, 150), (988, 150)],
        Cue::Tick => &[(660, 80)],
    }
}

/// Plays a cue on its own thread (Beep blocks while it sounds).
pub fn play(cue: Cue) {
    std::thread::spawn(move || {
        for (i, &(hz, ms)) in tones(cue).iter().enumerate() {
            if i > 0 {
                std::thread::sleep(std::time::Duration::from_millis(90));
            }
            sound(hz, ms);
        }
    });
}

#[cfg(windows)]
fn sound(hz: u32, ms: u32) {
    // SAFETY: Beep takes two numbers and plays a tone; it touches nothing else.
    unsafe { windows_sys::Win32::System::Diagnostics::Debug::Beep(hz, ms) };
}

#[cfg(not(windows))]
fn sound(_hz: u32, ms: u32) {
    std::thread::sleep(std::time::Duration::from_millis(u64::from(ms)));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_start_is_one_long_tone_and_the_end_two_short_ones() {
        assert_eq!(tones(Cue::Start).len(), 1);
        assert!(tones(Cue::Start)[0].1 >= 500);
        assert_eq!(tones(Cue::Switch).len(), 1);
        assert_eq!(tones(Cue::End).len(), 2);
        assert!(tones(Cue::End).iter().all(|&(_, ms)| ms < 300));
    }
}
