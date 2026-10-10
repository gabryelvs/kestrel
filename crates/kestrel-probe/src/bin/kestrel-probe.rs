//! Prints Kestrel's full report as JSON. Used by CI and to regenerate the UI's sample data.
fn main() {
    let pretty = std::env::args().any(|a| a == "--pretty");
    let report = kestrel_probe::collect_report(env!("CARGO_PKG_VERSION"));
    let json = if pretty {
        serde_json::to_string_pretty(&report)
    } else {
        serde_json::to_string(&report)
    };
    println!("{}", json.expect("report serializes"));
}
