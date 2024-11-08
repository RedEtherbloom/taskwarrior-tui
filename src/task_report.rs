use std::{error::Error, process::Command};

use anyhow::Result;
use chrono::{DateTime, Datelike, Local, MappedLocalTime, NaiveDate, NaiveDateTime, TimeZone};
use itertools::join;
use log::trace;
use task_hookrs::{task::Task, uda::UDAValue};
use unicode_truncate::UnicodeTruncateStr;
use unicode_width::UnicodeWidthStr;

const SECOND: i64 = 1;
const MINUTE: i64 = 60 * SECOND;
const HOUR: i64 = 60 * MINUTE;
const DAY: i64 = 24 * HOUR;
const WEEK: i64 = 7 * DAY;
const MONTH: i64 = 30 * DAY;
const YEAR: i64 = 365 * DAY;

pub struct TaskReportTable {
  pub labels: Vec<String>,
  pub columns: Vec<String>,
  pub tasks: Vec<Vec<String>>,
  pub virtual_tags: Vec<String>,
  pub description_width: usize,
  pub date_time_vague_precise: bool,
  pub date_time_vague_omit_0_remainder: bool,
}

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
      description_width: 100,
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
    self.tasks = vec![];

    // get all tasks as their string representation
    for task in tasks {
      if self.columns.is_empty() {
        break;
      }
      let mut item = vec![];
      for name in &self.columns {
        let s = self.get_string_attribute(name, task, tasks);
        item.push(s);
      }
      self.tasks.push(item);
    }
  }

  pub fn simplify_table(&mut self) -> (Vec<Vec<String>>, Vec<String>) {
    // find which columns are empty
    if self.tasks.is_empty() {
      return (vec![], vec![]);
    }

    let mut null_columns = vec![0; self.tasks[0].len()];

    for task in &self.tasks {
      for (i, s) in task.iter().enumerate() {
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

  pub fn get_string_attribute(&self, attribute: &str, task: &Task, tasks: &[Task]) -> String {
    match attribute {
      "id" => task.id().unwrap_or_default().to_string(),
      "scheduled.relative" => match task.scheduled() {
        Some(v) => self.vague_format_date_time_local_tz(Local::now().naive_utc(), **v),
        None => "".to_string(),
      },
      "scheduled.countdown" => match task.scheduled() {
        Some(v) => self.vague_format_date_time_local_tz(Local::now().naive_utc(), **v),
        None => "".to_string(),
      },
      "scheduled" => match task.scheduled() {
        Some(v) => Self::format_date(**v),
        None => "".to_string(),
      },
      "due.relative" => match task.due() {
        Some(v) => self.vague_format_date_time_local_tz(Local::now().naive_utc(), **v),
        None => "".to_string(),
      },
      "due" => match task.due() {
        Some(v) => Self::format_date(**v),
        None => "".to_string(),
      },
      "until.remaining" => match task.until() {
        Some(v) => self.vague_format_date_time_local_tz(Local::now().naive_utc(), **v),
        None => "".to_string(),
      },
      "until" => match task.until() {
        Some(v) => Self::format_date(**v),
        None => "".to_string(),
      },
      "entry.age" => self.vague_format_date_time_local_tz(**task.entry(), Local::now().naive_utc()),
      "entry" => Self::format_date(NaiveDateTime::new(task.entry().date(), task.entry().time())),
      "start.age" => match task.start() {
        Some(v) => self.vague_format_date_time_local_tz(**v, Local::now().naive_utc()),
        None => "".to_string(),
      },
      "start" => match task.start() {
        Some(v) => Self::format_date(**v),
        None => "".to_string(),
      },
      "end.age" => match task.end() {
        Some(v) => self.vague_format_date_time_local_tz(**v, Local::now().naive_utc()),
        None => "".to_string(),
      },
      "end" => match task.end() {
        Some(v) => Self::format_date(**v),
        None => "".to_string(),
      },
      "status.short" => task.status().to_string().chars().next().unwrap().to_string(),
      "status" => task.status().to_string(),
      "priority" => match task.priority() {
        Some(p) => p.clone(),
        None => "".to_string(),
      },
      "project" => match task.project() {
        Some(p) => p.to_string(),
        None => "".to_string(),
      },
      "depends.count" => match task.depends() {
        Some(v) => {
          if v.is_empty() {
            "".to_string()
          } else {
            format!("{}", v.len())
          }
        }
        None => "".to_string(),
      },
      "depends" => match task.depends() {
        Some(v) => {
          if v.is_empty() {
            "".to_string()
          } else {
            let mut dt = vec![];
            for u in v {
              if let Some(t) = tasks.iter().find(|t| t.uuid() == u) {
                dt.push(t.id().unwrap());
              }
            }
            join(dt.iter().map(ToString::to_string), " ")
          }
        }
        None => "".to_string(),
      },
      "tags.count" => match task.tags() {
        Some(v) => {
          let t = v.iter().filter(|t| !self.virtual_tags.contains(t)).count();
          if t == 0 {
            "".to_string()
          } else {
            t.to_string()
          }
        }
        None => "".to_string(),
      },
      "tags" => match task.tags() {
        Some(v) => v.iter().filter(|t| !self.virtual_tags.contains(t)).cloned().collect::<Vec<_>>().join(","),
        None => "".to_string(),
      },
      "recur" => match task.recur() {
        Some(v) => v.clone(),
        None => "".to_string(),
      },
      "wait" => match task.wait() {
        Some(v) => self.vague_format_date_time_local_tz(**v, Local::now().naive_utc()),
        None => "".to_string(),
      },
      "wait.remaining" => match task.wait() {
        Some(v) => self.vague_format_date_time_local_tz(Local::now().naive_utc(), **v),
        None => "".to_string(),
      },
      "description.count" => {
        let c = if let Some(a) = task.annotations() {
          format!("[{}]", a.len())
        } else {
          Default::default()
        };
        format!("{} {}", task.description(), c)
      }
      "description.truncated_count" => {
        let c = if let Some(a) = task.annotations() {
          format!("[{}]", a.len())
        } else {
          Default::default()
        };
        let d = task.description().to_string();
        let mut available_width = self.description_width;
        if self.description_width >= c.len() {
          available_width = self.description_width - c.len();
        }
        let (d, _) = d.unicode_truncate(available_width);
        let mut d = d.to_string();
        if d != *task.description() {
          d = format!("{}\u{2026}", d);
        }
        format!("{}{}", d, c)
      }
      "description.truncated" => {
        let d = task.description().to_string();
        let available_width = self.description_width;
        let (d, _) = d.unicode_truncate(available_width);
        let mut d = d.to_string();
        if d != *task.description() {
          d = format!("{}\u{2026}", d);
        }
        d
      }
      "description.desc" | "description" => task.description().to_string(),
      "urgency" => match &task.urgency() {
        Some(f) => format!("{:.2}", *f),
        None => "0.00".to_string(),
      },
      s => {
        let u = &task.uda();
        let v = u.get(s);
        if v.is_none() {
          return "".to_string();
        }
        match v.unwrap() {
          UDAValue::Str(s) => s.to_string(),
          UDAValue::F64(f) => f.to_string(),
          UDAValue::U64(u) => u.to_string(),
        }
      }
    }
  }

  pub fn format_date_time(dt: NaiveDateTime) -> String {
    let dt = Local.from_local_datetime(&dt).unwrap();
    dt.format("%Y-%m-%d %H:%M:%S").to_string()
  }

  pub fn format_date(dt: NaiveDateTime) -> String {
    let offset = Local.offset_from_utc_datetime(&dt);
    let dt = DateTime::<Local>::from_naive_utc_and_offset(dt, offset);
    dt.format("%Y-%m-%d").to_string()
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
    omit_0_remainder: bool
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
      Self::format_time_pair(negative, seconds / YEAR, remainder(YEAR, MONTH), "y", "mo", with_remainder, omit_0_remainder)
    } else if seconds >= 3 * MONTH {
      Self::format_time_pair(negative, seconds / MONTH, remainder(MONTH, WEEK), "mo", "w", with_remainder, omit_0_remainder)
    } else if seconds >= 2 * WEEK {
      Self::format_time_pair(negative, seconds / WEEK, remainder(WEEK, DAY), "w", "d", with_remainder, omit_0_remainder)
    } else if seconds >= DAY {
      Self::format_time_pair(negative, seconds / DAY, remainder(DAY, HOUR), "d", "h", with_remainder, omit_0_remainder)
    } else if seconds >= HOUR {
      Self::format_time_pair(negative, seconds / HOUR, remainder(HOUR, MINUTE), "h", "min", with_remainder, omit_0_remainder)
    } else if seconds >= MINUTE {
      Self::format_time_pair(negative, seconds / MINUTE, remainder(MINUTE, SECOND), "min", "s", with_remainder, omit_0_remainder)
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
  pub fn vague_format_date_time_local_tz(self: &Self, from_dt: NaiveDateTime, to_dt: NaiveDateTime) -> String {
    let to_dt = Self::ndt_to_dtl(&to_dt);
    let from_dt = Self::ndt_to_dtl(&from_dt);
    let seconds = (to_dt - from_dt).num_seconds();

    Self::vague_format_date_time(seconds, self.date_time_vague_precise, self.date_time_vague_omit_0_remainder)
  }
}

#[cfg(test)]
mod tests {
  use super::*;

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
