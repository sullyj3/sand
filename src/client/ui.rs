use std::fmt::{Display, Write};

use crossterm::style::Stylize;

use crate::sand::{
    duration::DurationExt,
    message::{TimerInfo, TimerState},
};

#[derive(Debug)]
struct TableConfig<'a> {
    status_column_width: usize,
    id_column_width: usize,
    remaining_column_width: usize,
    message_column_width: usize,
    gap: &'a str,
}

// TODO this needs a complete rework
pub fn ls(mut timers: Vec<TimerInfo>) -> impl Display {
    if timers.len() == 0 {
        return "There are currently no timers.\n".to_owned();
    };

    // TODO sorting and partitioning maybe shouldn't be the UI's
    // responsibility. Move to caller
    timers.sort_by(|t1, t2| {
        TimerInfo::cmp_by_next_due(t1, t2).then_with(|| TimerInfo::cmp_by_id(t1, t2))
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

    // TODO this is incorrect when timer is elapsed
    // need to actually compute the columns first, rather than trying to
    // predict in advance and duplicating computation
    let remaining_header = "Remaining";
    let remaining_column_width = {
        let widest_remaining_duration = timers
            .iter()
            .map(|ti| ti.remaining)
            .max()
            .expect("timers.len() != 0")
            .format_colon_separated()
            .len();
        widest_remaining_duration.max(remaining_header.len())
    };

    let message_header = "Message";
    let message_column_width = {
        let widest_message = timers
            .iter()
            .map(|ti| ti.message.as_ref().map(|s| s.len()).unwrap_or(0))
            .max()
            .expect("timers.len() != 0");
        widest_message.max(message_header.len())
    };

    let gap = "  ";
    let table_config = TableConfig {
        status_column_width,
        id_column_width,
        remaining_column_width,
        message_column_width,
        gap,
    };

    // Stylize doesn't seem to support formatting with padding,
    // so we have to pre-pad
    let id_header_padded = format!("{:<id_column_width$}", id_header);
    let remaining_header_padded = format!("{:<remaining_column_width$}", remaining_header);
    let message_header_padded = format!("{:<message_column_width$}", message_header);

    // Header
    write!(
        output,
        "{status_header}{gap}{}{gap}{}{gap}{}\n",
        id_header_padded.underlined(),
        remaining_header_padded.underlined(),
        message_header_padded.underlined(),
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

fn timers_table_row(output: &mut impl Write, timer_info: &TimerInfo, table_config: &TableConfig) {
    let remaining: String = if let TimerState::Elapsed = timer_info.state {
        "Elapsed".to_owned()
    } else {
        timer_info.remaining.format_colon_separated()
    };
    let id = timer_info.id;
    let play_pause = match timer_info.state {
        TimerState::Paused => " ⏸ ",
        TimerState::Running => " ▶ ",
        TimerState::Elapsed => " ⏹ ",
    };
    let &TableConfig {
        status_column_width,
        id_column_width,
        remaining_column_width,
        message_column_width,
        gap,
    } = table_config;
    let message: &str = timer_info.message.as_deref().unwrap_or("");
    write!(output,
        "{play_pause:>status_column_width$}{gap}{:>id_column_width$}{gap}{remaining:>remaining_column_width$}{gap}{message:<message_column_width$}\n",
        // need the string conversion first for the padding to work
        id.to_string(),
    ).unwrap();
}

pub fn next_due(timer: &TimerInfo) -> impl Display {
    format!(
        "Timer {}: {} left\n",
        timer.id,
        timer.remaining.format_colon_separated()
    )
}
