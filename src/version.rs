use std::cmp::Ordering;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Version<'a> {
    base: &'a str,
    suffix: Option<Suffix<'a>>,
    revision: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct Suffix<'a> {
    kind: &'a str,
    number: u64,
}

pub fn vercmp(left: &str, right: &str) -> Ordering {
    Version::parse(left).cmp(&Version::parse(right))
}

impl<'a> Version<'a> {
    pub fn parse(input: &'a str) -> Self {
        let (without_revision, revision) = strip_revision(input);
        let (base, suffix) = strip_suffix(without_revision);
        Self {
            base,
            suffix,
            revision,
        }
    }
}

impl Ord for Version<'_> {
    fn cmp(&self, other: &Self) -> Ordering {
        compare_base(self.base, other.base)
            .then_with(|| compare_suffix(&self.suffix, &other.suffix))
            .then_with(|| self.revision.cmp(&other.revision))
    }
}

impl PartialOrd for Version<'_> {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

fn strip_revision(input: &str) -> (&str, u64) {
    if let Some((base, revision)) = input.rsplit_once("-r")
        && !base.is_empty()
        && revision.chars().all(|c| c.is_ascii_digit())
    {
        return (base, revision.parse().unwrap_or(u64::MAX));
    }
    (input, 0)
}

fn strip_suffix(input: &str) -> (&str, Option<Suffix<'_>>) {
    let Some((base, suffix)) = input.split_once('_') else {
        return (input, None);
    };
    let split = suffix
        .find(|c: char| c.is_ascii_digit())
        .unwrap_or(suffix.len());
    let (kind, number) = suffix.split_at(split);
    let number = if number.is_empty() {
        0
    } else {
        number.parse().unwrap_or(u64::MAX)
    };
    (base, Some(Suffix { kind, number }))
}

fn compare_base(left: &str, right: &str) -> Ordering {
    let left_segments = split_segments(left);
    let right_segments = split_segments(right);
    let max = left_segments.len().max(right_segments.len());

    for index in 0..max {
        match compare_segment_at(&left_segments, &right_segments, index) {
            Ordering::Equal => {}
            ordering => return ordering,
        }
    }

    Ordering::Equal
}

fn compare_segment_at(left: &[Segment<'_>], right: &[Segment<'_>], index: usize) -> Ordering {
    match (left.get(index), right.get(index)) {
        (Some(left_segment), Some(right_segment)) => compare_segment_pair(
            *left_segment,
            *right_segment,
            index + 1 < left.len(),
            index + 1 < right.len(),
        ),
        (Some(_), None) => Ordering::Greater,
        (None, Some(_)) => Ordering::Less,
        (None, None) => Ordering::Equal,
    }
}

fn compare_segment_pair(
    left: Segment<'_>,
    right: Segment<'_>,
    left_has_more: bool,
    right_has_more: bool,
) -> Ordering {
    let numeric = compare_numeric(left.number, right.number);
    if numeric != Ordering::Equal {
        return numeric;
    }
    match (left.letter, right.letter, left_has_more, right_has_more) {
        (Some(_), None, _, true) => Ordering::Less,
        (None, Some(_), true, _) => Ordering::Greater,
        (Some(left), Some(right), _, _) => left.cmp(&right),
        (Some(_), None, _, _) => Ordering::Greater,
        (None, Some(_), _, _) => Ordering::Less,
        (None, None, _, _) => Ordering::Equal,
    }
}

#[derive(Debug, Clone, Copy)]
struct Segment<'a> {
    number: &'a str,
    letter: Option<char>,
}

fn split_segments(input: &str) -> Vec<Segment<'_>> {
    input
        .split('.')
        .map(|segment| {
            let split = segment
                .find(|c: char| !c.is_ascii_digit())
                .unwrap_or(segment.len());
            let (number, letter) = segment.split_at(split);
            Segment {
                number,
                letter: letter.chars().next(),
            }
        })
        .collect()
}

fn compare_numeric(left: &str, right: &str) -> Ordering {
    let left_trimmed = left.trim_start_matches('0');
    let right_trimmed = right.trim_start_matches('0');
    let left_normalized = if left_trimmed.is_empty() {
        "0"
    } else {
        left_trimmed
    };
    let right_normalized = if right_trimmed.is_empty() {
        "0"
    } else {
        right_trimmed
    };

    left_normalized
        .len()
        .cmp(&right_normalized.len())
        .then_with(|| left_normalized.cmp(right_normalized))
        .then_with(|| right.len().cmp(&left.len()))
}

fn compare_suffix(left: &Option<Suffix<'_>>, right: &Option<Suffix<'_>>) -> Ordering {
    let left_rank = left.as_ref().map_or(4, |suffix| suffix_rank(suffix.kind));
    let right_rank = right.as_ref().map_or(4, |suffix| suffix_rank(suffix.kind));
    left_rank.cmp(&right_rank).then_with(|| {
        let left_number = left.as_ref().map_or(0, |suffix| suffix.number);
        let right_number = right.as_ref().map_or(0, |suffix| suffix.number);
        left_number.cmp(&right_number)
    })
}

fn suffix_rank(kind: &str) -> u8 {
    match kind {
        "pre" => 0,
        "alpha" => 1,
        "beta" => 2,
        "rc" => 3,
        "p" => 5,
        _ => 4,
    }
}

/// Returns true when `input` looks like a Portage version string
/// (starts with a digit, optional `_suffix`/`-rN`). Used by `dep` helpers
/// and cpv sorting. Wildcard atom tokens are intentionally rejected here;
/// see `atom.rs` for the wildcard-aware variant used during atom parsing.
pub fn is_version(input: &str) -> bool {
    if input.is_empty() {
        return false;
    }
    let without_revision = match input.rsplit_once("-r") {
        Some((base, rev)) if !base.is_empty() && rev.chars().all(|c| c.is_ascii_digit()) => base,
        _ => input,
    };
    without_revision
        .chars()
        .next()
        .is_some_and(|c| c.is_ascii_digit())
        && without_revision
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-'))
}

/// Splits a `category/package-version` string into its `cp` and optional
/// version, mirroring Portage's `catpkgsplit` boundary detection.
pub fn split_cpv(cpv: &str) -> (String, Option<String>) {
    for index in cpv.match_indices('-').map(|(index, _)| index).rev() {
        let (cp, version) = (&cpv[..index], &cpv[index + 1..]);
        if !cp.is_empty() && is_version(version) {
            return (cp.to_string(), Some(version.to_string()));
        }
    }
    (cpv.to_string(), None)
}

/// Orders two cpv strings by `cp` then version, matching Portage's
/// `cpv_sort_key` ordering semantics.
pub fn cpv_cmp(left: &str, right: &str) -> Ordering {
    let (left_cp, left_version) = split_cpv(left);
    let (right_cp, right_version) = split_cpv(right);
    left_cp
        .cmp(&right_cp)
        .then_with(|| match (&left_version, &right_version) {
            (Some(left), Some(right)) => vercmp(left, right),
            (None, None) => Ordering::Equal,
            (None, Some(_)) => Ordering::Less,
            (Some(_), None) => Ordering::Greater,
        })
}

/// Sorts cpv strings in place using [`cpv_cmp`].
pub fn sort_cpvs(values: &mut [String]) {
    values.sort_by(|left, right| cpv_cmp(left, right));
}
