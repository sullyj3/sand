use std::time::Duration;

use crate::sand::{
    duration::DurationExt,
    timer::{TimerInfoForClient, TimerState},
};

pub fn display_timer_info(mut timers: Vec<TimerInfoForClient>) -> String {
    if timers.len() == 0 {
        return "There are currently no timers.\n".into();
    };

    timers.sort_by(TimerInfoForClient::cmp_by_next_due);
    let (running, paused): (Vec<_>, Vec<_>) = timers
        .iter()
        .partition(|ti| ti.state == TimerState::Running);

    let first_column_width = {
        let max_id = timers
            .iter()
            .map(|ti| ti.id)
            .max()
            .expect("timers.len() != 0");
        max_id.to_string().len()
    };
    let mut output = String::new();
    if running.len() > 0 {
        display_timer_info_table(&mut output, first_column_width, &running);
        if paused.len() > 0 {
            output.push_str("\n");
        }
    }
    if paused.len() > 0 {
        display_timer_info_table(&mut output, first_column_width, &paused);
    }

    output
}

/// Display a table of timer information. For use by `sand ls`
///
/// Used separately for running and paused timers.
fn display_timer_info_table(
    output: &mut String,
    first_column_width: usize,
    timers: &[&TimerInfoForClient],
) -> () {
    for &timer in timers {
        output.push_str(&display_timerinfo(timer, first_column_width));
        output.push('\n');
    }
}

// TODO rename
pub fn display_timerinfo(timer_info: &TimerInfoForClient, first_column_width: usize) -> String {
    let remaining: String =
        Duration::from_millis(timer_info.remaining_millis).format_colon_separated();
    let id = timer_info.id;
    let play_pause = match timer_info.state {
        TimerState::Paused => " ⏸ ",
        TimerState::Running => " ▶ ",
    };
    format!(
        "{play_pause} │ {:>width$} │ {remaining}",
        id.to_string(),
        width = first_column_width
    )
}
