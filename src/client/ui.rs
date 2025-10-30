use std::fmt::{Display, Write};
use std::time::Duration;

use crossterm::style::Stylize;

use crate::sand::{
    duration::DurationExt,
    timer::{TimerInfoForClient, TimerState},
};

#[derive(Debug)]
struct TableConfig<'a> {
    status_column_width: usize,
    id_column_width: usize,
    remaining_column_width: usize,
    gap: &'a str,
}

pub fn ls(mut timers: Vec<TimerInfoForClient>) -> impl Display {
    if timers.len() == 0 {
        return "There are currently no timers.\n".to_owned();
    };

    // TODO sorting and partitioning maybe shouldn't be the UI's
    // responsibility. Move to caller
    timers.sort_by(|t1, t2| {
        TimerInfoForClient::cmp_by_next_due(t1, t2)
            .then_with(|| TimerInfoForClient::cmp_by_id(t1, t2))
    });
    let (running, paused): (Vec<_>, Vec<_>) = timers
        .iter()
        .partition(|ti| ti.state == TimerState::Running);

    let mut output = String::new();

    // statuses are just a single emoji
    let status_header = "   ";
    let status_column_width = status_header.len();

    let id_header = "ID";
    let id_column_width = {
        let max_id = timers
            .iter()
            .map(|ti| ti.id)
            .max()
            .expect("timers.len() != 0");
        let max_id_len = max_id.to_string().len();
        max_id_len.max(id_header.len())
    };

    let remaining_header = "Remaining";
    let remaining_column_width = {
        let max_remaining = timers
            .iter()
            .map(|ti| ti.remaining_millis)
            .max()
            .expect("timers.len() != 0");
        let max_remaining_len = Duration::from_millis(max_remaining)
            .format_colon_separated()
            .len();
        max_remaining_len.max(remaining_header.len())
    };

    let gap = "  ";
    let table_config = TableConfig {
        status_column_width,
        id_column_width,
        remaining_column_width,
        gap,
    };

    // Stylize doesn't seem to support formatting with padding,
    // so we have to pre-pad
    let id_header_padded = format!("{:<id_column_width$}", id_header);
    let remaining_header_padded = format!("{:<remaining_column_width$}", remaining_header);

    // Header
    write!(
        output,
        "{status_header}{gap}{}{gap}{}\n",
        id_header_padded.underlined(),
        remaining_header_padded.underlined(),
    )
    .unwrap();

    if running.len() > 0 {
        for timer in running {
            timers_table_row(&mut output, timer, &table_config);
        }
        if paused.len() > 0 {
            output.push_str("\n");
        }
    }
    if paused.len() > 0 {
        for timer in paused {
            timers_table_row(&mut output, timer, &table_config);
        }
    }

    output
}

fn timers_table_row(
    output: &mut impl Write,
    timer_info: &TimerInfoForClient,
    table_config: &TableConfig,
) {
    let remaining: String =
        Duration::from_millis(timer_info.remaining_millis).format_colon_separated();
    let id = timer_info.id;
    let play_pause = match timer_info.state {
        TimerState::Paused => " ⏸ ",
        TimerState::Running => " ▶ ",
    };
    let &TableConfig {
        status_column_width,
        id_column_width,
        remaining_column_width,
        gap,
    } = table_config;
    write!(output,
        "{play_pause:>status_column_width$}{gap}{:>id_column_width$}{gap}{remaining:>remaining_column_width$}\n",
        // need the string conversion first for the padding to work
        id.to_string(),
    ).unwrap();
}

pub fn next_due(timer: &TimerInfoForClient) -> impl Display {
    format!(
        "Timer {}: {} left\n",
        timer.id,
        Duration::from_millis(timer.remaining_millis).format_colon_separated()
    )
}
