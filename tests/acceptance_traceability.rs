use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::PathBuf;

fn contract_id(value: &str) -> Option<String> {
    let id = value.trim().to_ascii_uppercase();
    let mut chars = id.chars();
    if !chars.next().is_some_and(|ch| ch.is_ascii_alphabetic()) {
        return None;
    }
    if !chars.next().is_some_and(|ch| ch.is_ascii_digit()) {
        return None;
    }
    if !chars.all(|ch| ch.is_ascii_alphanumeric()) {
        return None;
    }
    Some(id)
}

fn sensor_override(expectation: &str) -> Option<Option<String>> {
    let (_, suffix) = expectation.split_once("Sensor: ")?;
    let value = suffix
        .split(|ch: char| ch == '.' || ch == ' ' || ch == ')')
        .next()
        .expect("sensor marker has a value");
    if value.eq_ignore_ascii_case("manual") {
        Some(None)
    } else {
        Some(Some(
            contract_id(value).unwrap_or_else(|| panic!("invalid sensor id {value:?}")),
        ))
    }
}

#[test]
fn acceptance_rows_and_test_sensors_remain_traceable() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let acceptance = fs::read_to_string(root.join("ACCEPTANCE.md")).expect("read ACCEPTANCE.md");
    let tests =
        fs::read_to_string(root.join("tests/acceptance.rs")).expect("read tests/acceptance.rs");

    let mut rows = BTreeMap::<String, String>::new();
    for line in acceptance.lines().filter(|line| line.starts_with('|')) {
        let cells: Vec<_> = line.split('|').map(str::trim).collect();
        let Some(id) = cells.get(1).and_then(|cell| contract_id(cell)) else {
            continue;
        };
        let expectation = cells.get(3).copied().unwrap_or_default();
        assert!(
            rows.insert(id.clone(), expectation.to_owned()).is_none(),
            "duplicate acceptance row {id}"
        );
    }

    let mut sensors = BTreeMap::<String, String>::new();
    for line in tests.lines().map(str::trim) {
        let Some(function) = line
            .strip_prefix("fn ")
            .and_then(|line| line.split('(').next())
        else {
            continue;
        };
        let Some(id) = function.split('_').next().and_then(contract_id) else {
            continue;
        };
        assert!(
            sensors.insert(id.clone(), function.to_owned()).is_none(),
            "duplicate acceptance test id {id}"
        );
    }

    let mut referenced = BTreeSet::new();
    for (row, expectation) in &rows {
        let sensor = match sensor_override(expectation) {
            Some(sensor) => sensor,
            None => Some(row.clone()),
        };
        if let Some(sensor) = sensor {
            assert!(
                sensors.contains_key(&sensor),
                "acceptance row {row} names missing sensor {sensor}"
            );
            referenced.insert(sensor);
        }
    }

    let orphaned: Vec<_> = sensors
        .keys()
        .filter(|id| !referenced.contains(*id))
        .collect();
    assert!(
        orphaned.is_empty(),
        "acceptance tests without a row or explicit sensor reference: {orphaned:?}"
    );
}
