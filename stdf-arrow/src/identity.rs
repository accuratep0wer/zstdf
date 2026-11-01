//! Coordinate identity, resolved before a completed attempt is fragmented.
use crate::context::PartResult;

pub const IDENTITY_VERSION: &str = "coordinate-v1";

/// No trimming or case folding: wafer identifiers must consist of A-Z / 0-9.
pub fn wafer_key(wafer: Option<&str>, x: Option<i16>, y: Option<i16>) -> Option<String> {
    let wafer = wafer.filter(|s| {
        !s.is_empty()
            && s.bytes()
                .all(|b| b.is_ascii_uppercase() || b.is_ascii_digit())
    })?;
    let (x, y) = (x.filter(|n| *n > 0)?, y.filter(|n| *n > 0)?);
    Some(serde_json::json!(["wafer-prr", wafer, x, y]).to_string())
}

pub fn part_merge_key(part: &PartResult) -> Option<String> {
    if let Some(key) = wafer_key(part.wafer_id.as_deref(), part.x_coord, part.y_coord) {
        return Some(key);
    }
    // Fall back as a complete tuple. Never mix one PRR axis with one PTR axis.
    if part.lot_id.is_empty() {
        return None;
    }
    let mut coordinates = CoordinateCandidates::default();
    for test in &part.tests {
        if test.test_type != "PTR" {
            continue;
        }
        let Some(name) = test.test_txt.as_deref() else {
            continue;
        };
        coordinates.observe(name, test.result);
    }
    coordinates.lot_key(&part.lot_id)
}

/// Constant-space PTR coordinate resolver shared by conversion and traceability.
#[derive(Default)]
pub struct CoordinateCandidates {
    x: Candidate,
    y: Candidate,
}
impl CoordinateCandidates {
    pub fn observe(&mut self, name: &str, value: Option<f32>) {
        if let Some((axis, rank)) = coordinate_axis(name) {
            let target = if axis == b'x' {
                &mut self.x
            } else {
                &mut self.y
            };
            target.observe(rank, positive_integer(value));
        }
    }
    pub fn lot_key(&self, lot: &str) -> Option<String> {
        if lot.is_empty() {
            return None;
        }
        Some(serde_json::json!(["lot-ptr", lot, self.x.value()?, self.y.value()?]).to_string())
    }
}

fn positive_integer(value: Option<f32>) -> Option<i64> {
    let value = f64::from(value?);
    (value.is_finite() && value > 0.0 && value.fract() == 0.0 && value < 2_f64.powi(63))
        .then_some(value as i64)
}

/// Prefer explicit axis tokens (including camelCase) over a bare substring.
/// Names containing both axes without a unique token are ambiguous.
fn coordinate_axis(name: &str) -> Option<(u8, u8)> {
    if !name.bytes().any(|b| matches!(b, b'x' | b'X' | b'y' | b'Y')) {
        return None;
    }
    let chars: Vec<char> = name.chars().collect();
    let mut tokens = String::with_capacity(name.len() + 8);
    for (i, &ch) in chars.iter().enumerate() {
        if i > 0
            && ch.is_ascii_uppercase()
            && (chars[i - 1].is_ascii_lowercase()
                || (chars[i - 1].is_ascii_uppercase()
                    && chars.get(i + 1).is_some_and(char::is_ascii_lowercase)))
        {
            tokens.push(' ');
        }
        tokens.push(ch.to_ascii_lowercase());
    }
    let mut explicit_x = false;
    let mut explicit_y = false;
    for token in tokens.split(|ch: char| !ch.is_ascii_alphabetic()) {
        explicit_x |= token == "x";
        explicit_y |= token == "y";
    }
    let lower = name.to_ascii_lowercase();
    explicit_x |= [
        "coordx",
        "xcoord",
        "coordinatex",
        "positionx",
        "xposition",
        "locationx",
        "xlocation",
        "diex",
        "xdie",
        "waferx",
        "xwafer",
    ]
    .iter()
    .any(|pattern| lower.contains(pattern));
    explicit_y |= [
        "coordy",
        "ycoord",
        "coordinatey",
        "positiony",
        "yposition",
        "locationy",
        "ylocation",
        "diey",
        "ydie",
        "wafery",
        "ywafer",
    ]
    .iter()
    .any(|pattern| lower.contains(pattern));
    match (explicit_x, explicit_y) {
        (true, false) => return Some((b'x', 2)),
        (false, true) => return Some((b'y', 2)),
        (true, true) => return None,
        _ => {}
    }
    match (tokens.contains('x'), tokens.contains('y')) {
        (true, false) => Some((b'x', 1)),
        (false, true) => Some((b'y', 1)),
        _ => None,
    }
}

#[derive(Default)]
struct Candidate {
    rank: u8,
    coordinate: Option<i64>,
    invalid: bool,
}
impl Candidate {
    fn observe(&mut self, rank: u8, value: Option<i64>) {
        if rank > self.rank {
            *self = Self {
                rank,
                coordinate: value,
                invalid: value.is_none(),
            };
        } else if rank == self.rank && value != self.coordinate {
            self.invalid = true;
        }
    }
    fn value(&self) -> Option<i64> {
        if self.invalid {
            None
        } else {
            self.coordinate
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::context::TestResult;

    fn part() -> PartResult {
        PartResult {
            part_sequence: 1,
            lot_id: "LOT1".into(),
            wafer_id: Some("W1".into()),
            part_id: "P1".into(),
            head_num: 1,
            site_num: 0,
            x_coord: Some(3),
            y_coord: Some(4),
            hard_bin: 1,
            soft_bin: 1,
            part_pass: true,
            test_time_ms: None,
            tests: vec![],
        }
    }
    fn test(name: &str, value: f32) -> TestResult {
        TestResult {
            test_num: 1,
            test_txt: Some(name.into()),
            test_type: "PTR".into(),
            result: Some(value),
            test_pass: Some(true),
            lo_limit: None,
            hi_limit: None,
            units: None,
        }
    }
    fn fallback_part() -> PartResult {
        let mut p = part();
        p.wafer_id = Some("bad-wafer".into());
        p.tests = vec![test("coordinate_x", 10.0), test("CoordinateY", 20.0)];
        p
    }
    #[test]
    fn primary_requires_uppercase_alphanumeric_wafer_and_positive_prr() {
        assert!(wafer_key(Some("AZ019"), Some(1), Some(i16::MAX)).is_some());
        for wafer in [
            None,
            Some(""),
            Some("w1"),
            Some("W-1"),
            Some("W_1"),
            Some("W1 "),
            Some("W�1"),
            Some("Ｗ1"),
        ] {
            assert!(wafer_key(wafer, Some(1), Some(2)).is_none());
        }
        for n in [None, Some(0), Some(-1), Some(i16::MIN)] {
            assert!(wafer_key(Some("W1"), n, Some(1)).is_none());
            assert!(wafer_key(Some("W1"), Some(1), n).is_none());
        }
    }
    #[test]
    fn valid_prr_wins_even_when_ptrs_disagree() {
        let mut p = part();
        p.tests = fallback_part().tests;
        assert_eq!(part_merge_key(&p).unwrap(), r#"["wafer-prr","W1",3,4]"#);
    }
    #[test]
    fn any_invalid_primary_component_falls_back_as_a_whole() {
        let mut p = fallback_part();
        let expected = Some(r#"["lot-ptr","LOT1",10,20]"#.to_owned());
        assert_eq!(part_merge_key(&p), expected);
        p.wafer_id = Some("W1".into());
        p.x_coord = Some(0);
        assert_eq!(part_merge_key(&p), expected);
        p.x_coord = Some(3);
        p.y_coord = Some(-1);
        assert_eq!(part_merge_key(&p), expected);
    }
    #[test]
    fn recognizes_variable_names_and_case_without_swapping_axes() {
        for name in [
            "coordinate_x",
            "COORDINATE_X",
            "DieX",
            "xCoord",
            "waferx",
            "X_INDEX",
            "coordinatex",
        ] {
            assert_eq!(coordinate_axis(name).map(|v| v.0), Some(b'x'), "{name}");
        }
        for name in [
            "coordinate_y",
            "COORDINATE_Y",
            "DieY",
            "yCoord",
            "wafery",
            "Y_INDEX",
            "coordinateyindex",
        ] {
            assert_eq!(coordinate_axis(name).map(|v| v.0), Some(b'y'), "{name}");
        }
        for name in ["xy", "X_Y", "temperature"] {
            assert_eq!(coordinate_axis(name), None);
        }
    }
    #[test]
    fn fallback_rejects_missing_fractional_zero_negative_and_nonfinite_values() {
        for value in [
            0.0,
            -1.0,
            1.5,
            f32::NAN,
            f32::INFINITY,
            f32::NEG_INFINITY,
            f32::MAX,
        ] {
            for index in [0, 1] {
                let mut p = fallback_part();
                p.tests[index].result = Some(value);
                assert!(part_merge_key(&p).is_none(), "{value}");
            }
        }
        let mut p = fallback_part();
        p.tests.pop();
        assert!(part_merge_key(&p).is_none());
        let mut p = fallback_part();
        p.lot_id.clear();
        assert!(part_merge_key(&p).is_none());
    }
    #[test]
    fn explicit_axis_outranks_substrings_and_conflicts_do_not_pick_first() {
        let mut p = fallback_part();
        p.tests.push(test("Vmax", 100.0));
        let expected = part_merge_key(&p);
        p.tests.reverse();
        assert_eq!(part_merge_key(&p), expected);
        p.tests.push(test("Die_X", 11.0));
        assert!(part_merge_key(&p).is_none());
        p.tests.reverse();
        assert!(part_merge_key(&p).is_none());
    }
    #[test]
    fn equal_repeated_coordinates_are_allowed_but_non_ptr_is_not_used() {
        let mut p = fallback_part();
        p.tests.push(test("Die_X", 10.0));
        assert!(part_merge_key(&p).is_some());
        p.tests[1].test_type = "MPR".into();
        assert!(part_merge_key(&p).is_none());
    }
    #[test]
    fn wafer_and_lot_namespaces_do_not_collide() {
        let mut p = fallback_part();
        p.lot_id = "W1".into();
        assert_ne!(
            part_merge_key(&p),
            wafer_key(Some("W1"), Some(10), Some(20))
        );
    }
}
