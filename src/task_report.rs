use std::{error::Error, process::Command};

use anyhow::Result;
use chrono::{DateTime, Datelike, Local, MappedLocalTime, NaiveDate, NaiveDateTime, TimeZone};
use itertools::{join, Itertools};
use log::trace;
use task_hookrs::{annotation, date::Date, task::Task, uda::UDAValue};
use unicode_truncate::UnicodeTruncateStr;
use unicode_width::UnicodeWidthStr;
use uuid::Uuid;

const SECOND: i64 = 1;
const MINUTE: i64 = 60 * SECOND;
const HOUR: i64 = 60 * MINUTE;
const DAY: i64 = 24 * HOUR;
const WEEK: i64 = 7 * DAY;
const MONTH: i64 = 30 * DAY;
const YEAR: i64 = 365 * DAY;

static MISSING_ID_STRING: &str = "MIS";

pub struct TaskReportTable {
  pub labels: Vec<String>,
  pub columns: Vec<String>,
  pub tasks: Vec<Vec<String>>,
  pub virtual_tags: Vec<String>,
  pub max_description_width: usize,
  pub date_time_vague_precise: bool,
  pub date_time_vague_omit_0_remainder: bool,
}

// TODO: Replace clone, to_owned, etc. to be uniform
// I don't like the drift for newbies I'm introducing
impl TaskReportTable {
  pub fn new(data: &str, report: &str) -> Result<Self> {
    let virtual_tags = vec![
      "PROJECT",
      "BLOCKED",
      "UNBLOCKED",
      "BLOCKING",
      "DUE",
      "DUETODAY",
      "TODAY",
      "OVERDUE",
      "WEEK",
      "MONTH",
      "QUARTER",
      "YEAR",
      "ACTIVE",
      "SCHEDULED",
      "PARENT",
      "CHILD",
      "UNTIL",
      "WAITING",
      "ANNOTATED",
      "READY",
      "YESTERDAY",
      "TOMORROW",
      "TAGGED",
      "PENDING",
      "COMPLETED",
      "DELETED",
      "UDA",
      "ORPHAN",
      "PRIORITY",
      "PROJECT",
      "LATEST",
      "RECURRING",
      "INSTANCE",
      "TEMPLATE",
    ];

    let mut task_report_table = Self {
      labels: vec![],
      columns: vec![],
      tasks: vec![vec![]],
      virtual_tags: virtual_tags.iter().map(ToString::to_string).collect::<Vec<_>>(),
      max_description_width: 100,
      date_time_vague_precise: false,
      date_time_vague_omit_0_remainder: false,
    };
    task_report_table.export_headers(Some(data), report)?;
    Ok(task_report_table)
  }

  pub fn export_headers(&mut self, data: Option<&str>, report: &str) -> Result<()> {
    self.columns = vec![];
    self.labels = vec![];

    let data = if let Some(s) = data {
      s.to_string()
    } else {
      let output = Command::new("task")
        .arg("show")
        .arg("rc.defaultwidth=0")
        .arg(format!("report.{}.columns", report))
        .output()?;
      String::from_utf8_lossy(&output.stdout).into_owned()
    };

    for line in data.split('\n') {
      if line.starts_with(format!("report.{}.columns", report).as_str()) {
        let column_names = line.split_once(' ').unwrap().1;
        for column in column_names.split(',') {
          self.columns.push(column.to_string());
        }
      }
    }

    let output = Command::new("task")
      .arg("show")
      .arg("rc.defaultwidth=0")
      .arg(format!("report.{}.labels", report))
      .output()?;
    let data = String::from_utf8_lossy(&output.stdout);

    for line in data.split('\n') {
      if line.starts_with(format!("report.{}.labels", report).as_str()) {
        let label_names = line.split_once(' ').unwrap().1;
        for label in label_names.split(',') {
          self.labels.push(label.to_string());
        }
      }
    }

    if self.labels.is_empty() {
      for label in &self.columns {
        let label = label.split('.').collect::<Vec<&str>>()[0];
        let label = if label == "id" { "ID" } else { label };
        let mut c = label.chars();
        let label = match c.next() {
          None => String::new(),
          Some(f) => f.to_uppercase().collect::<String>() + c.as_str(),
        };
        if !label.is_empty() {
          self.labels.push(label);
        }
      }
    }
    let num_labels = self.labels.len();
    let num_columns = self.columns.len();
    assert!(num_labels == num_columns, "Must have the same number of labels (currently {}) and columns (currently {}). Compare their values as shown by \"task show report.{}.\" and fix your taskwarrior config.", num_labels, num_columns, report);

    Ok(())
  }

  pub fn generate_table(&mut self, tasks: &[Task]) {    
    // Reset task table to avoid old data hanging around
    self.tasks = vec![];
    
    if self.columns.is_empty() {
      return;
    }
    
    // Convenience closure
    let task_to_row = |task| -> Vec<String> {
      self.columns.iter().map(|column_name| self.get_string_attribute(column_name, task, tasks)).collect()
    };
    
    self.tasks = tasks.iter().map(|task| task_to_row(task)).collect();
  }

  pub fn simplify_table(&mut self) -> (Vec<Vec<String>>, Vec<String>) {
    let mut null_columns = match self.tasks.first() {
      Some(task) => vec![0; task.len()],
      // No tasks => easiest table :)
      None => return (vec![], vec![]),
    };

    for task in &self.tasks {
      for (i, s) in task.iter().enumerate() {
        // ?
        null_columns[i] += s.len();
      }
    }

    // filter out columns where everything is empty
    let mut tasks = vec![];
    for task in &self.tasks {
      let t = task.clone();
      let t: Vec<String> = t
        .iter()
        .enumerate()
        .filter(|&(i, _)| null_columns[i] != 0)
        .map(|(_, e)| e.clone())
        .collect();
      tasks.push(t);
    }

    // filter out header where all columns are empty
    let headers: Vec<String> = self
      .labels
      .iter()
      .enumerate()
      .filter(|&(i, _)| null_columns[i] != 0)
      .map(|(_, e)| e.clone())
      .collect();

    (tasks, headers)
  }

  pub fn truncate_unicode_string_with_limiter(unicode_str: &str, max_width: usize, limiter: &str) -> Result<(String, new_width, rest_iterator), Error> {

    return Err(());
  }

  pub fn get_string_attribute(&self, attribute: &str, task: &Task, tasks: &[Task]) -> String {
    // Format vague events that lay in the future, e.g. until
    let format_vague_until =
      |date: Option<&Date>| -> Option<String> { date.map(|date| self.vague_format_date_time_local_tz(&Local::now().naive_utc(), date)) };
    // Format vague events that lie in the past, e.g. since
    let format_vague_since =
      |date: Option<&Date>| -> Option<String> { date.map(|date| self.vague_format_date_time_local_tz(date, &Local::now().naive_utc())) };
    // Truncate a string and if so show it with Unicode
    let truncate_description = |description: &str, max_width: usize| -> String {
      let (truncated, _) = description.unicode_truncate(max_width);
      match truncated.len() == description.len() {
        true => description.to_owned(),
        false => format!("{}…", truncated),
      }
    };

    // TODO: Replace the Some with into
    let output: Option<String> = match attribute {
      "id" => task.id().map_or(MISSING_ID_STRING.to_owned(), |id| id.to_string()).into(),
      "scheduled.relative" | "scheduled.countdown" => format_vague_until(task.scheduled()),
      "scheduled" => task.scheduled().map(|date| Self::format_date(date)),
      "due.relative" => format_vague_until(task.due()),
      "due" => task.due().map(|date| Self::format_date(date)),
      "until.remaining" => format_vague_until(task.until()),
      "until" => task.until().map(|date| Self::format_date(date)),
      "entry.age" => format_vague_since(task.entry().into()),
      "entry" => Self::format_date(task.entry()).into(),
      "start.age" => format_vague_since(task.start()),
      "start" => task.start().map(|date| Self::format_date(date)),
      "end.age" => format_vague_since(task.end()),
      "end" => task.end().map(|date| Self::format_date(date)),
      // Returns initial letter, e.g. P for Pending
      "status.short" => task.status().to_string().chars().next().unwrap().to_string().into(),
      "status" => task.status().to_string().into(),
      "priority" => task.priority().map(|prio| prio.clone()),
      "project" => task.project().map(|proj| proj.to_string()),
      "depends.count" => task.depends().filter(|depe| !depe.is_empty()).map(|depe| depe.len().to_string()),
      "depends" => task.depends().filter(|depe| !depe.is_empty()).map(|depe| {
        // Create working copy of refs
        let mut tasks_to_find: Vec<&Uuid> = depe.iter().collect();
        // Worst case size
        let mut found_ids: Vec<u64> = Vec::with_capacity(tasks_to_find.len());

        // Iterate over all tasks, looking for a task with matching UUID
        for t in tasks.iter() {
          if tasks_to_find.contains(&t.uuid()) {
            found_ids.push(t.id().unwrap_or_default());
            // Task found, UUID can be removed for efficiency
            tasks_to_find.retain(|e| e != &t.uuid());
          }
        }

        // Sorting for visual purposes
        found_ids.sort();
        join(
          found_ids.iter().map(|id| {
            match id {
              // u64s default value that is not a valid Taskwarrior ID(they are 1-indiced).
              // This indicates a missing taskid and gets mapped to the corresponding string.
              0 => MISSING_ID_STRING.to_owned(),
              _ => id.to_string(),
            }
          }),
          " ",
        )
      }),
      "tags.count" => task
        .tags()
        .map(|tags| match tags.iter().filter(|t| !self.virtual_tags.contains(t)).count() {
          0 => String::new(),
          val => val.to_string(),
        }),
      "tags" => task.tags().map(|tags| tags.iter().filter(|t| !self.virtual_tags.contains(t)).join(",")),
      "recur" => task.recur().map(|recur| recur.clone()),
      "wait" => format_vague_since(task.wait()),
      "wait.remaining" => format_vague_until(task.wait()),
      // Only print annotations if there are more than 0
      "description.count" => task
        .annotations()
        .filter(|annotations| annotations.len() > 0)
        .map_or_else(
          || format!("{}", task.description()),
          |annotations| format!("{} [{}]", task.description(), annotations.len()),
        )
        .into(),
      "description.truncated_count" => {
        let anno_count_formatted = task
          .annotations()
          .filter(|anno| anno.len() > 0)
          .map_or(String::new(), |anno| format!("[{}]", anno.len()));

        // Account annotation length in max field width. Saturating sub is too be safe in an edge-case
        let available_width = self.max_description_width.saturating_sub(anno_count_formatted.len());
        let truncated = truncate_description(task.description(), available_width);

        if truncated.len() != task.description().len() {
          d = format!("{}…", d);
        }
        format!("{} {}", truncated, anno_count_formatted)
      }
      // Oversight: It will be one longer than truncated
      "description.truncated" => truncate_description(task.description(), self.max_description_width).into(),
      "description.desc" | "description" => task.description().clone().into(),
      "urgency" => task.urgency().map_or("0.00".to_owned(), |urg| format!("{:.2}", urg)).into(),
      // Catch-all, we probably caught an UDA
      uda_name => task.uda().get(uda_name).map(|uda_value| Self::format_uda_value(uda_value)),
    };

    // A None means that a field or UDA is not set. We map these to a simple empty String.
    output.unwrap_or(String::new())
  }

  pub fn format_uda_value(uda_value: &UDAValue) -> String {
    match uda_value {
      UDAValue::Str(str) => str.clone(),
      UDAValue::F64(num) => num.to_string(),
      UDAValue::U64(num) => num.to_string(),
    }
  }

  pub fn format_date(dt: &NaiveDateTime) -> String {
    let dt = Local.from_local_datetime(dt).unwrap();
    dt.format("%Y-%m-%d").to_string()
  }

  pub fn format_date_time(dt: NaiveDateTime) -> String {
    let dt = Local.from_local_datetime(&dt).unwrap();
    dt.format("%Y-%m-%d %H:%M:%S").to_string()
  }

  /// Formats two given time values for use by `vague_format_date_time`
  ///
  /// # Arguments
  ///
  /// * `negative` - Boolean denominating if a minus should be prefixed.
  /// * `major_time` - The bigger component of the two time components, e.g. in years.
  /// * `minor_time` - The smaller component of the two time components, e.g. in months.
  ///                  The remainder that only gets printed when `with_remainder` is true.
  /// * `major_suffix` - The time suffix for the major component, e.g. y for years.
  /// * `minor_suffix` - The time suffix for the minor component, e.g. mo for months.
  /// * `with_remainder` - Determines if `minor_time` and `minor_suffix` get appended as the remainder.
  /// * `omit_0_remainder` - Disables the remainder if it is 0, due to rounding.
  fn format_time_pair(
    negative: bool,
    major_time: i64,
    minor_time: i64,
    major_suffix: &'static str,
    minor_suffix: &'static str,
    mut with_remainder: bool,
    omit_0_remainder: bool,
  ) -> String {
    let minus = match negative {
      true => "-",
      false => "",
    };

    if omit_0_remainder && minor_time == 0 {
      with_remainder = false;
    }

    match with_remainder {
      true => format!("{minus}{major_time}{major_suffix}{minor_time}{minor_suffix}"),
      false => format!("{minus}{major_time}{major_suffix}"),
    }
  }

  pub fn vague_format_date_time(mut seconds: i64, with_remainder: bool, omit_0_remainder: bool) -> String {
    let negative = if seconds < 0 {
      seconds *= -1;
      true
    } else {
      false
    };
    let remainder = |major_time: i64, minor_time: i64| -> i64 { (seconds - major_time * (seconds / major_time)) / minor_time };

    if seconds >= YEAR {
      Self::format_time_pair(
        negative,
        seconds / YEAR,
        remainder(YEAR, MONTH),
        "y",
        "mo",
        with_remainder,
        omit_0_remainder,
      )
    } else if seconds >= 3 * MONTH {
      Self::format_time_pair(
        negative,
        seconds / MONTH,
        remainder(MONTH, WEEK),
        "mo",
        "w",
        with_remainder,
        omit_0_remainder,
      )
    } else if seconds >= 2 * WEEK {
      Self::format_time_pair(negative, seconds / WEEK, remainder(WEEK, DAY), "w", "d", with_remainder, omit_0_remainder)
    } else if seconds >= DAY {
      Self::format_time_pair(negative, seconds / DAY, remainder(DAY, HOUR), "d", "h", with_remainder, omit_0_remainder)
    } else if seconds >= HOUR {
      Self::format_time_pair(
        negative,
        seconds / HOUR,
        remainder(HOUR, MINUTE),
        "h",
        "min",
        with_remainder,
        omit_0_remainder,
      )
    } else if seconds >= MINUTE {
      Self::format_time_pair(
        negative,
        seconds / MINUTE,
        remainder(MINUTE, SECOND),
        "min",
        "s",
        with_remainder,
        omit_0_remainder,
      )
    } else {
      Self::format_time_pair(negative, seconds, 0, "s", "", false, false)
    }
  }

  /// Converts NaiveDateTime to DateTime<Local>
  fn ndt_to_dtl(from: &NaiveDateTime) -> DateTime<Local> {
    match Local.from_local_datetime(from) {
      MappedLocalTime::Single(to) => to,
      // This can happen if the `from` falls in e.g. Daylight Savings Timezone gap
      MappedLocalTime::Ambiguous(earlier_to, later_to) => {
        trace!(
          "Got ambiguous Local times, earlier: {:?} and later: {:?}. Choosing earlier",
          earlier_to,
          later_to
        );
        earlier_to
      }
      MappedLocalTime::None => {
        panic!("Got impossible time that fell in Local timezone gap from {:?}", from);
      }
    }
  }

  /// Format vague date_time while taking e.g. leap seconds or DST into account
  pub fn vague_format_date_time_local_tz(self: &Self, from_dt: &NaiveDateTime, to_dt: &NaiveDateTime) -> String {
    let to_dt = Self::ndt_to_dtl(to_dt);
    let from_dt = Self::ndt_to_dtl(from_dt);
    let seconds = (to_dt - from_dt).num_seconds();

    Self::vague_format_date_time(seconds, self.date_time_vague_precise, self.date_time_vague_omit_0_remainder)
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn test_option_format_date() {}

  #[test]
  fn test_format_time_pair() {
    assert_eq!(TaskReportTable::format_time_pair(true, 3, 1, "y", "mo", false, false), "-3y");
    assert_eq!(TaskReportTable::format_time_pair(true, 3, 1, "y", "mo", true, false), "-3y1mo");

    assert_eq!(TaskReportTable::format_time_pair(false, 11, 7, "h", "min", true, false), "11h7min");
    assert_eq!(TaskReportTable::format_time_pair(false, 11, 7, "h", "min", false, false), "11h");
  }

  #[test]
  fn test_format_vague_date_time() {
    assert_eq!(TaskReportTable::vague_format_date_time(YEAR + DAY, true, false), "1y0mo");
    assert_eq!(TaskReportTable::vague_format_date_time(YEAR + DAY, true, true), "1y");
    assert_eq!(TaskReportTable::vague_format_date_time(YEAR + MONTH, true, true), "1y1mo");
    assert_eq!(TaskReportTable::vague_format_date_time(YEAR + DAY, false, false), "1y");

    assert_eq!(TaskReportTable::vague_format_date_time(3 * MONTH + WEEK, true, false), "3mo1w");
    assert_eq!(TaskReportTable::vague_format_date_time(3 * MONTH + WEEK, false, false), "3mo");

    assert_eq!(TaskReportTable::vague_format_date_time(2 * WEEK + DAY, true, false), "2w1d");
    assert_eq!(TaskReportTable::vague_format_date_time(2 * WEEK + DAY, false, false), "2w");

    assert_eq!(TaskReportTable::vague_format_date_time(DAY + HOUR, true, false), "1d1h");
    assert_eq!(TaskReportTable::vague_format_date_time(DAY + HOUR, false, false), "1d");

    assert_eq!(TaskReportTable::vague_format_date_time(HOUR + MINUTE, true, false), "1h1min");
    assert_eq!(TaskReportTable::vague_format_date_time(HOUR + MINUTE, false, false), "1h");

    assert_eq!(TaskReportTable::vague_format_date_time(MINUTE + SECOND, true, false), "1min1s");
    assert_eq!(TaskReportTable::vague_format_date_time(MINUTE + SECOND, false, false), "1min");

    assert_eq!(TaskReportTable::vague_format_date_time(SECOND, true, false), "1s");
    assert_eq!(TaskReportTable::vague_format_date_time(SECOND, false, false), "1s");
  }
}
