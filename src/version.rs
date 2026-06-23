use std::cmp::Ordering;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Version<'a> {
    base: &'a str,
    suffixes: Vec<Suffix>,
    revision: u64,
}

/// A single `_suffix` component. `rank` is upstream's `suffix_value` weight
/// (`alpha=-4, beta=-3, pre=-2, rc=-1, p=0`); `number` is the trailing count.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Suffix {
    rank: i64,
    number: i64,
}

/// Upstream pads a missing suffix position with `("p", "-1")` so that, e.g.,
/// `1 < 1_p0`. See `portage.versions.vercmp`.
const IMPLICIT_SUFFIX: Suffix = Suffix {
    rank: 0,
    number: -1,
};

pub fn vercmp(left: &str, right: &str) -> Ordering {
    Version::parse(left).cmp(&Version::parse(right))
}

impl<'a> Version<'a> {
    pub fn parse(input: &'a str) -> Self {
        let (without_revision, revision) = strip_revision(input);
        let (base, suffixes) = split_suffixes(without_revision);
        Self {
            base,
            suffixes,
            revision,
        }
    }
}

impl Ord for Version<'_> {
    fn cmp(&self, other: &Self) -> Ordering {
        compare_base(self.base, other.base)
            .then_with(|| compare_suffixes(&self.suffixes, &other.suffixes))
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

fn split_suffixes(input: &str) -> (&str, Vec<Suffix>) {
    let mut parts = input.split('_');
    let base = parts.next().unwrap_or(input);
    let suffixes = parts.map(parse_suffix).collect();
    (base, suffixes)
}

/// Parses one `_suffix` token into its upstream rank/number. Mirrors
/// `suffix_value` and the `^(alpha|beta|rc|pre|p)(\d*)$` regexp; `pre` is
/// checked before `p` so `pre1` is not mis-read as `p`.
fn parse_suffix(token: &str) -> Suffix {
    for (name, rank) in [
        ("alpha", -4),
        ("beta", -3),
        ("pre", -2),
        ("rc", -1),
        ("p", 0),
    ] {
        if let Some(rest) = token.strip_prefix(name)
            && rest.chars().all(|c| c.is_ascii_digit())
        {
            let number = if rest.is_empty() {
                0
            } else {
                rest.parse().unwrap_or(i64::MAX)
            };
            return Suffix { rank, number };
        }
    }
    // Unknown suffix token (upstream would reject the version outright). Treat
    // it as a neutral `p0` so vercmp stays total on lenient input.
    Suffix { rank: 0, number: 0 }
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

fn compare_suffixes(left: &[Suffix], right: &[Suffix]) -> Ordering {
    let max = left.len().max(right.len());
    for index in 0..max {
        let l = left.get(index).copied().unwrap_or(IMPLICIT_SUFFIX);
        let r = right.get(index).copied().unwrap_or(IMPLICIT_SUFFIX);
        let ordering = l.rank.cmp(&r.rank).then_with(|| l.number.cmp(&r.number));
        if ordering != Ordering::Equal {
            return ordering;
        }
    }
    Ordering::Equal
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
