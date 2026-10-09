//! The HTTP endpoint that takes the values of the charts' series and the
//! rows of the tables, so anything on the network can push them to the
//! panel:
//!
//! - `PUT /series/NAME` with a JSON array of numbers replaces the series;
//! - `POST /series/NAME` with a number, or an array of them, adds to its
//!   end, the oldest going once there are more than `MAX_VALUES`;
//! - `GET /series/NAME` gives it back; `DELETE /series/NAME` empties it;
//! - `PUT /tables/NAME` with a JSON array of objects, column to value,
//!   replaces a table's rows, and `POST` adds to their end, the oldest
//!   going past `MAX_ROWS`; `GET` counts them and `DELETE` empties them;
//! - `GET /` lists the series and the tables there are, with each table's
//!   columns, as JSON.
//!
//! With a token set, a request must carry it as `Authorization: Bearer
//! TOKEN`, or it is a 401.
//!
//! It listens on its own thread and answers each request on another, so a
//! slow client stalls neither the frames nor the next request. The body is
//! read and parsed before the snapshot is locked.

use crate::config::HttpConfig;
use crate::data::{log_changed, records, Shared, MAX_ROWS};
use std::collections::BTreeMap;
use std::io::Read;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::Duration;
use tiny_http::{Header, Method, Request, Response, Server};

/// The most values a series keeps.
pub const MAX_VALUES: usize = 1024;

/// The largest request body read, in bytes.
const MAX_BODY: usize = 256 * 1024;

/// Listens on its own thread until `stop` is set, for the series in
/// `names` and the rows of `tables`. An address that cannot be bound is tried again every second: at
/// a reload the server before this one may still be letting go of it.
pub fn spawn(cfg: HttpConfig, names: Vec<String>, tables: Tables, shared: Shared, stop: Arc<AtomicBool>) {
    thread::spawn(move || {
        let mut last_err = None;
        let server = loop {
            match Server::http(&cfg.listen) {
                Ok(server) => break server,
                Err(e) => log_changed(&mut last_err, "http", anyhow::anyhow!("listening on {}: {e}", cfg.listen)),
            }
            thread::sleep(Duration::from_secs(1));
            if stop.load(Ordering::SeqCst) {
                return;
            }
        };
        eprintln!("panel-ddp: http: listening on {}", server.server_addr());
        serve(&server, Arc::new(Allowed { names, tables, token: cfg.token }), &shared, &stop);
    });
}

/// The tables that take rows, by data name: the columns their tiles read,
/// each "text" or "numbers".
pub type Tables = BTreeMap<String, BTreeMap<String, &'static str>>;

/// What a request may do: the series and the tables' data that exist, and
/// the token it must carry, if any.
struct Allowed {
    names: Vec<String>,
    tables: Tables,
    token: Option<String>,
}

impl Allowed {
    /// Whether the `Authorization` header a request came with, if any, lets
    /// it in. The comparison takes the same time wherever the first
    /// difference is, so the time taken does not give the token away.
    fn lets_in(&self, authorization: Option<&str>) -> bool {
        let Some(token) = &self.token else { return true };
        let Some((scheme, given)) = authorization.and_then(|a| a.trim().split_once(' ')) else { return false };
        let (token, given) = (token.as_bytes(), given.trim().as_bytes());
        scheme.eq_ignore_ascii_case("bearer")
            && token.len() == given.len()
            && token.iter().zip(given).fold(0, |acc, (a, b)| acc | (a ^ b)) == 0
    }
}

/// Answers requests until `stop` is set, each on a thread of its own.
fn serve(server: &Server, allowed: Arc<Allowed>, shared: &Shared, stop: &AtomicBool) {
    while !stop.load(Ordering::SeqCst) {
        match server.recv_timeout(Duration::from_secs(1)) {
            Ok(Some(request)) => {
                let (allowed, shared) = (Arc::clone(&allowed), Arc::clone(shared));
                thread::spawn(move || handle(request, &allowed, &shared));
            }
            Ok(None) => {}
            Err(e) => {
                eprintln!("panel-ddp: http: {e}");
                thread::sleep(Duration::from_secs(1));
            }
        }
    }
}

fn handle(mut request: Request, allowed: &Allowed, shared: &Shared) {
    let authorization =
        request.headers().iter().find(|h| h.field.equiv("Authorization")).map(|h| h.value.as_str().to_string());
    if !allowed.lets_in(authorization.as_deref()) {
        let challenge = Header::from_bytes("WWW-Authenticate", "Bearer").expect("a valid header");
        let refusal = Response::from_string("a bearer token is needed\n").with_status_code(401).with_header(challenge);
        let _ = request.respond(refusal);
        return;
    }
    let mut body = String::new();
    let read = request.as_reader().take(MAX_BODY as u64 + 1).read_to_string(&mut body);
    let (status, text) = match read {
        Err(_) => (400, "the body is not UTF-8 text\n".to_string()),
        Ok(_) if body.len() > MAX_BODY => (413, format!("the body is over {MAX_BODY} bytes\n")),
        Ok(_) => answer(request.method(), request.url(), &body, allowed, shared),
    };
    let _ = request.respond(Response::from_string(text).with_status_code(status));
}

/// The status and the text a request is answered with, after doing what it
/// asks.
fn answer(method: &Method, url: &str, body: &str, allowed: &Allowed, shared: &Shared) -> (u16, String) {
    let path = url.split('?').next().unwrap_or_default();
    let named = |prefix: &str| path.strip_prefix(prefix).filter(|n| !n.is_empty() && !n.contains('/'));
    if path == "/" {
        return match method {
            Method::Get => (200, format!("{:#}\n", serde_json::json!({"series": allowed.names, "tables": allowed.tables}))),
            _ => (405, "GET\n".into()),
        };
    }
    match (named("/series/"), named("/tables/")) {
        (Some(name), _) if allowed.names.iter().any(|n| n == name) => series(method, name, body, shared),
        (_, Some(name)) if allowed.tables.contains_key(name) => table(method, name, body, shared),
        (Some(name), _) => (404, format!("no chart draws a series named {name}\n")),
        (_, Some(name)) => (404, format!("no table has data named {name}\n")),
        _ => (404, "the series are at /series/NAME and the tables' rows at /tables/NAME; GET / lists them\n".into()),
    }
}

/// A request for a chart's series.
fn series(method: &Method, name: &str, body: &str, shared: &Shared) -> (u16, String) {
    match method {
        Method::Get => {
            let values = shared.lock().unwrap().pushed.get(name).cloned().unwrap_or_default();
            (200, format!("{}\n", serde_json::json!(*values)))
        }
        Method::Delete => {
            shared.lock().unwrap().pushed.remove(name);
            (200, "0 values\n".into())
        }
        Method::Put | Method::Post => {
            let new = match values(body) {
                Ok(v) => v,
                Err(e) => return (400, format!("{e}\n")),
            };
            let mut snap = shared.lock().unwrap();
            let mut all = match (method, snap.pushed.get(name)) {
                (Method::Post, Some(old)) => old.to_vec(),
                _ => Vec::new(),
            };
            all.extend(new);
            all.drain(..all.len().saturating_sub(MAX_VALUES));
            let count = all.len();
            snap.pushed.insert(name.to_string(), Arc::new(all));
            (200, format!("{count} values\n"))
        }
        _ => (405, "GET, PUT, POST or DELETE\n".into()),
    }
}

/// A request for a table's rows.
fn table(method: &Method, name: &str, body: &str, shared: &Shared) -> (u16, String) {
    match method {
        Method::Get => {
            let count = shared.lock().unwrap().tables.get(name).map_or(0, |rows| rows.len());
            (200, format!("{count} rows\n"))
        }
        Method::Delete => {
            shared.lock().unwrap().tables.remove(name);
            (200, "0 rows\n".into())
        }
        Method::Put | Method::Post => {
            let json = serde_json::from_str(body).map_err(|_| "the body is not JSON".to_string());
            let new = match json.and_then(|json| records(&json)) {
                Ok(rows) => rows,
                Err(e) => return (400, format!("{e}\n")),
            };
            let mut snap = shared.lock().unwrap();
            let mut all = match (method, snap.tables.get(name)) {
                (Method::Post, Some(old)) => old.to_vec(),
                _ => Vec::new(),
            };
            all.extend(new);
            all.drain(..all.len().saturating_sub(MAX_ROWS));
            let count = all.len();
            snap.tables.insert(name.to_string(), Arc::new(all));
            (200, format!("{count} rows\n"))
        }
        _ => (405, "GET, PUT, POST or DELETE\n".into()),
    }
}

/// The numbers in a body: a JSON number, or an array of them.
fn values(body: &str) -> Result<Vec<f64>, String> {
    let wrong = || "the body is a JSON number or an array of numbers".to_string();
    match serde_json::from_str(body).map_err(|_| wrong())? {
        serde_json::Value::Number(n) => Ok(vec![n.as_f64().ok_or_else(wrong)?]),
        serde_json::Value::Array(a) => a.iter().map(|v| v.as_f64().ok_or_else(wrong)).collect(),
        _ => Err(wrong()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn allowed(names: &[&str], tables: &[&str]) -> Allowed {
        let columns: BTreeMap<String, &'static str> = [("room".to_string(), "text"), ("history".to_string(), "numbers")].into();
        Allowed {
            names: names.iter().map(|s| s.to_string()).collect(),
            tables: tables.iter().map(|s| (s.to_string(), columns.clone())).collect(),
            token: None,
        }
    }

    #[test]
    fn tables_take_rows() {
        use crate::data::Value;
        let shared = Shared::default();
        let allowed = allowed(&[], &["rooms"]);
        let ask = |method: Method, body: &str| answer(&method, "/tables/rooms", body, &allowed, &shared);
        let put = r#"[{"room": "Kitchen", "temp": 22.8, "history": [21, 22.8]}, {"room": "Office", "temp": 23.1}]"#;
        assert_eq!(ask(Method::Put, put), (200, "2 rows\n".into()));
        assert_eq!(ask(Method::Post, r#"{"room": "Hall"}"#), (200, "3 rows\n".into()));
        assert_eq!(ask(Method::Get, ""), (200, "3 rows\n".into()));
        let rows = shared.lock().unwrap().tables["rooms"].clone();
        assert_eq!(rows[0]["temp"], Value::Text("22.8".into()));
        assert_eq!(rows[0]["history"], Value::Series(vec![21.0, 22.8]));
        assert_eq!(rows[2]["room"], Value::Text("Hall".into()));
        assert_eq!(ask(Method::Put, "[1, 2]").0, 400);
        assert_eq!(ask(Method::Put, "not json"), (400, "the body is not JSON\n".into()));
        assert_eq!(ask(Method::Get, ""), (200, "3 rows\n".into()), "a refused body changes nothing");
        assert_eq!(answer(&Method::Put, "/tables/other", "[]", &allowed, &shared).0, 404);
        assert_eq!(answer(&Method::Put, "/series/rooms", "[1]", &allowed, &shared).0, 404, "a table is not a series");
        assert_eq!(ask(Method::Put, "[]"), (200, "0 rows\n".into()));
        let (status, listing) = answer(&Method::Get, "/", "", &allowed, &shared);
        let listing: serde_json::Value = serde_json::from_str(&listing).unwrap();
        assert_eq!(status, 200);
        assert_eq!(listing, serde_json::json!({"series": [], "tables": {"rooms": {"history": "numbers", "room": "text"}}}));
        assert_eq!(answer(&Method::Put, "/", "[]", &allowed, &shared).0, 405);
    }

    fn pushed(shared: &Shared, name: &str) -> Vec<f64> {
        shared.lock().unwrap().pushed.get(name).map(|v| v.to_vec()).unwrap_or_default()
    }

    #[test]
    fn puts_replace_and_posts_add() {
        let shared = Shared::default();
        let allowed = allowed(&["power"], &[]);
        let ask = |method: Method, url: &str, body: &str| answer(&method, url, body, &allowed, &shared);
        assert_eq!(ask(Method::Put, "/series/power", "[1, 2.5]"), (200, "2 values\n".into()));
        assert_eq!(ask(Method::Post, "/series/power", "4"), (200, "3 values\n".into()));
        assert_eq!(ask(Method::Post, "/series/power?x=1", "[5]").0, 200);
        assert_eq!(pushed(&shared, "power"), [1.0, 2.5, 4.0, 5.0]);
        assert_eq!(ask(Method::Get, "/series/power", ""), (200, "[1.0,2.5,4.0,5.0]\n".into()));
        assert_eq!(ask(Method::Put, "/series/power", "[9]").0, 200);
        assert_eq!(pushed(&shared, "power"), [9.0], "a put starts over");
        assert_eq!(ask(Method::Put, "/series/power", "[1, \"two\"]").0, 400);
        assert_eq!(ask(Method::Put, "/series/power", "{}").0, 400);
        assert_eq!(pushed(&shared, "power"), [9.0], "a refused body changes nothing");
        assert_eq!(ask(Method::Put, "/series/other", "[1]").0, 404, "not a series a tile draws");
        assert_eq!(ask(Method::Put, "/elsewhere", "[1]").0, 404);
        assert_eq!(ask(Method::Patch, "/series/power", "[1]").0, 405);
        assert_eq!(ask(Method::Delete, "/series/power", "").0, 200);
        assert!(pushed(&shared, "power").is_empty());
    }

    #[test]
    fn a_token_is_asked_for_only_when_set() {
        let allowed = |token: Option<&str>| Allowed { names: Vec::new(), tables: Tables::new(), token: token.map(str::to_string) };
        assert!(allowed(None).lets_in(None));
        assert!(allowed(None).lets_in(Some("Bearer anything")));
        let with = allowed(Some("s3cret"));
        assert!(with.lets_in(Some("Bearer s3cret")));
        assert!(with.lets_in(Some("bearer  s3cret ")));
        for wrong in [None, Some(""), Some("s3cret"), Some("Bearer"), Some("Bearer s3cre"), Some("Bearer s3cretx"), Some("Basic s3cret")] {
            assert!(!with.lets_in(wrong), "{wrong:?}");
        }
    }

    #[test]
    fn keeps_the_newest_values() {
        let shared = Shared::default();
        let names = allowed(&["n"], &[]);
        let many = serde_json::json!((0..MAX_VALUES + 10).collect::<Vec<_>>()).to_string();
        answer(&Method::Put, "/series/n", &many, &names, &shared);
        answer(&Method::Post, "/series/n", "-1", &names, &shared);
        let kept = pushed(&shared, "n");
        assert_eq!(kept.len(), MAX_VALUES);
        assert_eq!((kept[0], kept[MAX_VALUES - 1]), (11.0, -1.0));
    }

    #[test]
    fn serves_over_a_socket_until_stopped() {
        let server = Server::http("127.0.0.1:0").unwrap();
        let url = format!("http://{}/series/power", server.server_addr());
        let shared = Shared::default();
        let stop = Arc::new(AtomicBool::new(false));
        let thread = {
            let (shared, stop) = (Arc::clone(&shared), Arc::clone(&stop));
            let allowed = Allowed { token: Some("s3cret".into()), ..allowed(&["power"], &[]) };
            thread::spawn(move || serve(&server, Arc::new(allowed), &shared, &stop))
        };
        let status = |r: Result<ureq::Response, ureq::Error>| r.unwrap_err().into_response().unwrap().status();
        assert_eq!(status(ureq::put(&url).send_string("[3, 1, 2]")), 401, "no token");
        assert_eq!(status(ureq::put(&url).set("Authorization", "Bearer wrong!").send_string("[3]")), 401);
        assert!(pushed(&shared, "power").is_empty(), "nothing is taken without the token");
        let put = ureq::put(&url).set("Authorization", "Bearer s3cret").send_string("[3, 1, 2]");
        assert_eq!(put.unwrap().into_string().unwrap(), "3 values\n");
        assert_eq!(pushed(&shared, "power"), [3.0, 1.0, 2.0]);
        assert_eq!(status(ureq::post(&url).set("Authorization", "bearer s3cret").send_string("nope")), 400);
        stop.store(true, Ordering::SeqCst);
        thread.join().unwrap();
        assert!(ureq::get(&url).call().is_err(), "the listener goes with its server");
    }
}
