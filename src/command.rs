//! Commands as a source: each `sources.commands` entry is run again and
//! again, and what it prints, JSON, becomes a table's rows or a chart's
//! series, as if it had been pushed. `sqlite3 -json`, `duckdb -json` and a
//! `psql` query that aggregates to JSON all print rows this way, so a
//! database needs nothing linked in.
//!
//! Each command has its own thread, keeps the last good result, and is
//! killed when it outlasts its timeout or the run ends.

use crate::config::CommandConfig;
use crate::data::{log_changed, nap, records, Record, Shared, Value, MAX_ROWS};
use crate::http::MAX_VALUES;
use anyhow::{anyhow, bail, Context, Result};
use std::collections::BTreeMap;
use std::io::Read;
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};

/// The most a command's output is read, in bytes.
const MAX_OUTPUT: u64 = 1024 * 1024;

/// What a command's output is for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Feed {
    /// The rows of the table whose `data` is this name.
    Table(String),
    /// The series of this name.
    Series(String),
}

/// Starts a thread for each command, running it now and then every
/// `every` seconds until `stop` is set. `columns` says, for each table, what
/// its tiles read of each column.
pub fn spawn(
    commands: &[CommandConfig],
    columns: &BTreeMap<String, BTreeMap<String, &'static str>>,
    shared: &Shared,
    stop: &Arc<AtomicBool>,
) {
    for cmd in commands.iter().cloned() {
        let (shared, stop) = (Arc::clone(shared), Arc::clone(stop));
        let columns = match &cmd.feed {
            Feed::Table(name) => columns.get(name.as_str()).cloned().unwrap_or_default(),
            Feed::Series(_) => BTreeMap::new(),
        };
        thread::spawn(move || {
            let mut last_err = None;
            loop {
                match run(&cmd, &stop).and_then(|out| store(&cmd, &columns, &out, &shared)) {
                    Ok(()) => last_err = None,
                    Err(e) => log_changed(&mut last_err, "command", e.context(cmd.run.join(" "))),
                }
                if !nap(&stop, Duration::from_secs(cmd.every.max(1))) {
                    break;
                }
            }
        });
    }
}

/// Runs the command to its end and gives what it printed. It is killed,
/// and that is an error, when it takes longer than its timeout or `stop`
/// is set; so is ending with anything but success.
fn run(cmd: &CommandConfig, stop: &AtomicBool) -> Result<String> {
    let (program, args) = cmd.run.split_first().context("no command")?;
    let mut child = Command::new(program)
        .args(args)
        .current_dir(&cmd.dir)
        .envs(cmd.env.iter().map(|(k, v)| (k, v)))
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .with_context(|| format!("starting {program}"))?;
    // Both pipes are read as the command runs, or a full one would stall it.
    let read = |pipe: Option<Box<dyn Read + Send>>| {
        thread::spawn(move || {
            let mut text = Vec::new();
            if let Some(pipe) = pipe {
                let _ = pipe.take(MAX_OUTPUT + 1).read_to_end(&mut text);
            }
            text
        })
    };
    let out = read(child.stdout.take().map(|p| Box::new(p) as Box<dyn Read + Send>));
    let err = read(child.stderr.take().map(|p| Box::new(p) as Box<dyn Read + Send>));
    let deadline = Instant::now() + Duration::from_secs(cmd.timeout.max(1));
    let status = loop {
        if let Some(status) = child.try_wait().context("waiting")? {
            break status;
        }
        let late = Instant::now() >= deadline;
        if late || stop.load(Ordering::SeqCst) {
            let _ = child.kill();
            let _ = child.wait();
            bail!(if late { format!("still running after {} s; stopped", cmd.timeout) } else { "stopped".into() });
        }
        thread::sleep(Duration::from_millis(25));
    };
    let (out, err) = (out.join().unwrap_or_default(), err.join().unwrap_or_default());
    if !status.success() {
        let said = String::from_utf8_lossy(&err);
        bail!("{status}: {}", said.lines().find(|l| !l.trim().is_empty()).unwrap_or("it said nothing").trim());
    }
    if out.len() as u64 > MAX_OUTPUT {
        bail!("it printed more than {MAX_OUTPUT} bytes");
    }
    String::from_utf8(out).map_err(|_| anyhow!("its output is not UTF-8"))
}

/// Puts what a command printed where its feed says.
fn store(cmd: &CommandConfig, columns: &BTreeMap<String, &'static str>, out: &str, shared: &Shared) -> Result<()> {
    // Nothing printed is no rows: `sqlite3 -json` prints nothing for none.
    let json: serde_json::Value =
        if out.trim().is_empty() { serde_json::json!([]) } else { serde_json::from_str(out).context("its output is not JSON")? };
    match &cmd.feed {
        Feed::Table(name) => {
            let mut rows = records(&json).map_err(|e| anyhow!(e))?;
            rows.truncate(MAX_ROWS);
            for row in &mut rows {
                as_series(row, columns);
            }
            shared.lock().unwrap().tables.insert(name.clone(), Arc::new(rows));
        }
        Feed::Series(name) => {
            let mut values = series(&json, cmd.column.as_deref()).map_err(|e| anyhow!(e))?;
            values.drain(..values.len().saturating_sub(MAX_VALUES));
            shared.lock().unwrap().pushed.insert(name.clone(), Arc::new(values));
        }
    }
    Ok(())
}

/// The numbers in text that lists them: a JSON array, or what a database
/// prints for an array, `{1,2,3}` or `[1, 2, 3]`.
fn listed(text: &str) -> Option<Vec<f64>> {
    let inner = text.trim().trim_start_matches(['[', '{']).trim_end_matches([']', '}']);
    if inner.trim().is_empty() {
        return Some(Vec::new());
    }
    inner.split(',').map(|n| n.trim().parse::<f64>().ok().filter(|v| v.is_finite())).collect()
}

/// Turns the columns a chart draws into numbers where a database gave them
/// as text: `sqlite3 -json` prints a JSON array in a column as a string.
fn as_series(row: &mut Record, columns: &BTreeMap<String, &'static str>) {
    for (column, holds) in columns {
        if *holds != "numbers" {
            continue;
        }
        if let Some(Value::Text(text)) = row.get(column) {
            if let Some(values) = listed(text) {
                row.insert(column.clone(), Value::Series(values));
            }
        }
    }
}

/// A series from a command's JSON: an array of numbers as it is, or of
/// rows, taking `column` from each, or their only column when they have
/// one. A number alone is a series of one. Nulls are left out.
pub fn series(json: &serde_json::Value, column: Option<&str>) -> Result<Vec<f64>, String> {
    use serde_json::Value as Json;
    let number = |v: &Json| match v {
        Json::Number(n) => n.as_f64().filter(|v| v.is_finite()).map(Some).ok_or(()),
        Json::String(s) => s.trim().parse::<f64>().ok().filter(|v| v.is_finite()).map(Some).ok_or(()),
        Json::Null => Ok(None),
        _ => Err(()),
    };
    let items = match json {
        Json::Array(items) => items.as_slice(),
        other => std::slice::from_ref(other),
    };
    let mut values = Vec::with_capacity(items.len());
    for (i, item) in items.iter().enumerate() {
        let value = match (item, column) {
            (Json::Object(row), Some(column)) => {
                row.get(column).ok_or_else(|| format!("row {}: no column {column}", i + 1))?
            }
            (Json::Object(row), None) if row.len() == 1 => row.values().next().expect("one value"),
            (Json::Object(_), None) => return Err(format!("row {}: several columns; say which with column", i + 1)),
            (value, _) => value,
        };
        match number(value) {
            Ok(Some(v)) => values.push(v),
            Ok(None) => {}
            Err(()) => return Err(format!("row {}: not a number", i + 1)),
        }
    }
    Ok(values)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn command(script: &str, feed: Feed) -> CommandConfig {
        CommandConfig {
            run: vec!["sh".into(), "-c".into(), script.into()],
            feed,
            column: None,
            every: 60,
            timeout: 5,
            env: vec![("PANEL_TEST_VALUE".into(), "7".into())],
            dir: std::env::temp_dir(),
        }
    }

    #[test]
    fn a_series_from_numbers_or_rows() {
        assert_eq!(series(&json!([1, 2.5, "3", null]), None).unwrap(), [1.0, 2.5, 3.0]);
        assert_eq!(series(&json!(4), None).unwrap(), [4.0]);
        assert_eq!(series(&json!([{"w": 1}, {"w": 2}]), None).unwrap(), [1.0, 2.0], "the only column");
        assert_eq!(series(&json!([{"at": "x", "w": 1}]), Some("w")).unwrap(), [1.0]);
        assert!(series(&json!([{"at": "x", "w": 1}]), None).unwrap_err().contains("several columns"));
        assert!(series(&json!([{"w": 1}]), Some("v")).unwrap_err().contains("no column v"));
        assert!(series(&json!(["one"]), None).unwrap_err().contains("row 1: not a number"));
        assert!(series(&json!([]), None).unwrap().is_empty());
    }

    #[test]
    fn lists_in_text_become_numbers() {
        assert_eq!(listed("[1, 2.5,3]"), Some(vec![1.0, 2.5, 3.0]));
        assert_eq!(listed("{4,5}"), Some(vec![4.0, 5.0]));
        assert_eq!(listed("[]"), Some(vec![]));
        assert_eq!(listed("12 ms"), None);
        let mut row = Record::from([("h".to_string(), Value::Text("[1,2]".into())), ("n".to_string(), Value::Text("[1,2]".into()))]);
        as_series(&mut row, &BTreeMap::from([("h".to_string(), "numbers"), ("n".to_string(), "text")]));
        assert_eq!(row["h"], Value::Series(vec![1.0, 2.0]));
        assert_eq!(row["n"], Value::Text("[1,2]".into()), "only where a chart draws the column");
    }

    #[test]
    fn runs_a_command_and_stores_what_it_prints() {
        let shared = Shared::default();
        let stop = AtomicBool::new(false);
        let cmd = command(r#"echo "[{\"name\": \"api\", \"ms\": $PANEL_TEST_VALUE, \"h\": \"[1,2]\"}]""#, Feed::Table("t".into()));
        let columns = BTreeMap::from([("h".to_string(), "numbers")]);
        store(&cmd, &columns, &run(&cmd, &stop).unwrap(), &shared).unwrap();
        let rows = shared.lock().unwrap().tables["t"].clone();
        assert_eq!(rows[0]["ms"], Value::Text("7".into()), "its environment is set");
        assert_eq!(rows[0]["h"], Value::Series(vec![1.0, 2.0]));

        let cmd = command("echo '[3, 1, 2]'", Feed::Series("s".into()));
        store(&cmd, &BTreeMap::new(), &run(&cmd, &stop).unwrap(), &shared).unwrap();
        assert_eq!(*shared.lock().unwrap().pushed["s"], [3.0, 1.0, 2.0]);

        let cmd = command("true", Feed::Table("t".into()));
        store(&cmd, &columns, &run(&cmd, &stop).unwrap(), &shared).unwrap();
        assert!(shared.lock().unwrap().tables["t"].is_empty(), "nothing printed is no rows");
    }

    #[test]
    fn failures_are_errors_and_keep_what_there_was() {
        let stop = AtomicBool::new(false);
        let failed = format!("{:#}", run(&command("echo oops >&2; exit 3", Feed::Series("s".into())), &stop).unwrap_err());
        assert!(failed.contains("oops"), "{failed}");
        let missing = CommandConfig { run: vec!["no-such-program-here".into()], ..command("", Feed::Series("s".into())) };
        assert!(format!("{:#}", run(&missing, &stop).unwrap_err()).contains("starting no-such-program-here"));
        let slow = CommandConfig { timeout: 1, ..command("sleep 20", Feed::Series("s".into())) };
        let started = Instant::now();
        assert!(format!("{:#}", run(&slow, &stop).unwrap_err()).contains("still running after 1 s"));
        assert!(started.elapsed() < Duration::from_secs(5), "it is killed, not waited for");
        let shared = Shared::default();
        let cmd = command("echo not json", Feed::Series("s".into()));
        assert!(store(&cmd, &BTreeMap::new(), "not json", &shared).is_err());
        assert!(shared.lock().unwrap().pushed.is_empty());
    }
}
