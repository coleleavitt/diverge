use std::fmt;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct AtomParseOptions {
    pub allow_wildcard: bool,
    pub allow_repo: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Blocker {
    Weak,
    Strong,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Operator {
    Equal,
    EqualGlob,
    Greater,
    GreaterEqual,
    Less,
    LessEqual,
    Tilde,
}

impl Operator {
    pub fn as_portage_str(self) -> &'static str {
        match self {
            Self::Equal => "=",
            Self::EqualGlob => "=*",
            Self::Greater => ">",
            Self::GreaterEqual => ">=",
            Self::Less => "<",
            Self::LessEqual => "<=",
            Self::Tilde => "~",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SlotDependency {
    pub slot: Option<String>,
    pub sub_slot: Option<String>,
    pub operator: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Atom {
    pub blocker: Option<Blocker>,
    pub operator: Option<Operator>,
    pub category: String,
    pub package: String,
    pub version: Option<String>,
    pub slot: Option<SlotDependency>,
    pub use_deps: Option<String>,
    pub repo: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AtomError {
    Empty,
    NonAscii,
    InvalidBlocker,
    InvalidRepository,
    RepositoryNotAllowed,
    InvalidUseDependency,
    InvalidSlot,
    InvalidCategoryPackage,
    WildcardNotAllowed,
    InvalidVersion,
    VersionWithoutOperator,
    OperatorRequiresVersion,
}

impl fmt::Display for AtomError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let message = match self {
            Self::Empty => "empty atom",
            Self::NonAscii => "atom contains non-ASCII characters",
            Self::InvalidBlocker => "invalid blocker prefix",
            Self::InvalidRepository => "invalid repository qualifier",
            Self::RepositoryNotAllowed => "repository qualifier is not allowed",
            Self::InvalidUseDependency => "invalid USE dependency",
            Self::InvalidSlot => "invalid slot dependency",
            Self::InvalidCategoryPackage => "invalid category/package name",
            Self::WildcardNotAllowed => "wildcard atom is not allowed",
            Self::InvalidVersion => "invalid version component",
            Self::VersionWithoutOperator => "versioned package atom requires an operator",
            Self::OperatorRequiresVersion => "operator atom requires a version",
        };
        f.write_str(message)
    }
}

impl std::error::Error for AtomError {}

pub const DEPENDENCY_ATOM_OPTIONS: AtomParseOptions = AtomParseOptions {
    allow_wildcard: false,
    allow_repo: true,
};

impl Atom {
    pub fn parse(input: &str) -> Result<Self, AtomError> {
        Self::parse_with_options(input, AtomParseOptions::default())
    }

    pub fn parse_with_options(input: &str, options: AtomParseOptions) -> Result<Self, AtomError> {
        if input.is_empty() {
            return Err(AtomError::Empty);
        }
        if !input.is_ascii() || input.bytes().any(|b| b.is_ascii_control()) {
            return Err(AtomError::NonAscii);
        }

        let (blocker, rest) = strip_blocker(input)?;
        let (mut operator, rest) = strip_operator(rest);
        let (rest, use_deps) = strip_use_deps(rest)?;
        let (rest, repo) = strip_repo(rest, options.allow_repo)?;
        let (rest, slot) = strip_slot(rest)?;
        let (category, package_version) = split_category_package(rest)?;

        if category.contains('*') {
            if !options.allow_wildcard {
                return Err(AtomError::WildcardNotAllowed);
            }
            validate_wildcard_name(category)?;
        } else {
            validate_name(category)?;
        }

        let glob_version =
            matches!(operator, Some(Operator::Equal)) && package_version.ends_with('*');
        if glob_version {
            operator = Some(Operator::EqualGlob);
        }

        let (package, version) = if operator.is_some() {
            split_package_version(package_version, glob_version)?
        } else if looks_versioned(package_version) {
            return Err(AtomError::VersionWithoutOperator);
        } else {
            (package_version.to_string(), None)
        };

        if package.contains('*') {
            if !options.allow_wildcard {
                return Err(AtomError::WildcardNotAllowed);
            }
            validate_wildcard_name(&package)?;
        } else {
            validate_name(&package)?;
        }

        if operator.is_some() && version.is_none() {
            return Err(AtomError::OperatorRequiresVersion);
        }

        Ok(Self {
            blocker,
            operator,
            category: category.to_string(),
            package,
            version,
            slot,
            use_deps,
            repo,
        })
    }

    pub fn cp(&self) -> String {
        format!("{}/{}", self.category, self.package)
    }

    pub fn cpv(&self) -> String {
        match &self.version {
            Some(version) => format!("{}-{version}", self.cp()),
            None => self.cp(),
        }
    }

    pub fn slot(&self) -> Option<&str> {
        self.slot.as_ref().and_then(|slot| slot.slot.as_deref())
    }

    pub fn sub_slot(&self) -> Option<&str> {
        self.slot.as_ref().and_then(|slot| slot.sub_slot.as_deref())
    }

    pub fn slot_operator(&self) -> Option<&str> {
        self.slot.as_ref().and_then(|slot| slot.operator.as_deref())
    }

    /// Parses this atom's `[...]` USE-dependency group (if any) into typed
    /// tokens for [`crate::matching`]. Returns `None` when the atom has no USE
    /// deps. The stored text is already validated by [`Atom::parse_with_options`].
    pub fn parsed_use_deps(&self) -> Option<crate::matching::ParsedUseDeps> {
        let raw = self.use_deps.as_ref()?;
        let body = raw.trim_start_matches('[').trim_end_matches(']');
        let mut tokens = Vec::new();
        for token in body.split(',') {
            let mut flag = token;
            let negated = if let Some(rest) = flag.strip_prefix('!') {
                flag = rest;
                true
            } else if let Some(rest) = flag.strip_prefix('-') {
                flag = rest;
                true
            } else {
                false
            };
            // Strip conditional suffixes (`=`/`?`) that this matcher ignores.
            let mut default = None;
            loop {
                if let Some(rest) = flag.strip_suffix("(+)") {
                    default = Some(crate::matching::UseDefault::Enabled);
                    flag = rest;
                } else if let Some(rest) = flag.strip_suffix("(-)") {
                    default = Some(crate::matching::UseDefault::Disabled);
                    flag = rest;
                } else if let Some(rest) = flag.strip_suffix('=') {
                    flag = rest;
                } else if let Some(rest) = flag.strip_suffix('?') {
                    flag = rest;
                } else {
                    break;
                }
            }
            tokens.push(crate::matching::UseDepToken {
                name: flag.to_string(),
                negated,
                default,
            });
        }
        Some(crate::matching::ParsedUseDeps { tokens })
    }

    pub fn intersects(&self, other: &Self) -> bool {
        if self.cp() != other.cp() {
            return false;
        }
        if let (Some(left), Some(right)) = (&self.version, &other.version)
            && left != right
        {
            return false;
        }
        match (self.slot(), other.slot()) {
            (Some(left), Some(right)) => left == right,
            _ => true,
        }
    }
}

impl fmt::Display for Atom {
    /// Renders the atom back to its canonical string form, mirroring Portage's
    /// `Atom.__str__` ordering: blocker, operator, cpv, slot, repo, use deps.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.blocker {
            Some(Blocker::Strong) => f.write_str("!!")?,
            Some(Blocker::Weak) => f.write_str("!")?,
            None => {}
        }
        if let Some(op) = self.operator {
            // EqualGlob renders as a leading `=` with a trailing `*` on the cpv.
            f.write_str(if op == Operator::EqualGlob {
                "="
            } else {
                op.as_portage_str()
            })?;
        }
        write!(f, "{}", self.cpv())?;
        if matches!(self.operator, Some(Operator::EqualGlob)) {
            f.write_str("*")?;
        }
        if let Some(slot) = &self.slot {
            f.write_str(":")?;
            if let Some(name) = &slot.slot {
                f.write_str(name)?;
                if let Some(sub) = &slot.sub_slot {
                    write!(f, "/{sub}")?;
                }
            }
            if let Some(op) = &slot.operator {
                f.write_str(op)?;
            }
        }
        if let Some(repo) = &self.repo {
            write!(f, "::{repo}")?;
        }
        if let Some(use_deps) = &self.use_deps {
            f.write_str(use_deps)?;
        }
        Ok(())
    }
}

pub fn is_valid_atom(input: &str, options: AtomParseOptions) -> bool {
    Atom::parse_with_options(input, options).is_ok()
}

fn strip_blocker(input: &str) -> Result<(Option<Blocker>, &str), AtomError> {
    if let Some(rest) = input.strip_prefix("!!") {
        if rest.starts_with('!') {
            return Err(AtomError::InvalidBlocker);
        }
        Ok((Some(Blocker::Strong), rest))
    } else if let Some(rest) = input.strip_prefix('!') {
        if rest.starts_with('!') {
            return Err(AtomError::InvalidBlocker);
        }
        Ok((Some(Blocker::Weak), rest))
    } else {
        Ok((None, input))
    }
}

fn strip_operator(input: &str) -> (Option<Operator>, &str) {
    for (prefix, op) in [
        (">=", Operator::GreaterEqual),
        ("<=", Operator::LessEqual),
        ("=", Operator::Equal),
        (">", Operator::Greater),
        ("<", Operator::Less),
        ("~", Operator::Tilde),
    ] {
        if let Some(rest) = input.strip_prefix(prefix) {
            return (Some(op), rest);
        }
    }
    (None, input)
}

fn strip_use_deps(input: &str) -> Result<(&str, Option<String>), AtomError> {
    match (input.find('['), input.rfind(']')) {
        (None, None) => Ok((input, None)),
        (Some(open), Some(close)) if close == input.len() - 1 && open < close => {
            if input[..open].contains(']') || input[open + 1..close].contains('[') {
                return Err(AtomError::InvalidUseDependency);
            }
            let body = &input[open + 1..close];
            validate_use_deps(body)?;
            Ok((&input[..open], Some(format!("[{body}]"))))
        }
        _ => Err(AtomError::InvalidUseDependency),
    }
}

fn validate_use_deps(body: &str) -> Result<(), AtomError> {
    if body.is_empty() || body.starts_with(',') || body.ends_with(',') {
        return Err(AtomError::InvalidUseDependency);
    }

    let mut seen = Vec::new();
    for token in body.split(',') {
        if token.is_empty() {
            return Err(AtomError::InvalidUseDependency);
        }
        let mut flag = token;
        let mut bang = false;
        if let Some(rest) = flag.strip_prefix('!') {
            bang = true;
            flag = rest;
            if flag.starts_with('-') || flag.is_empty() {
                return Err(AtomError::InvalidUseDependency);
            }
        } else if let Some(rest) = flag.strip_prefix('-') {
            flag = rest;
            if flag.ends_with('?') || flag.ends_with('=') || flag.is_empty() {
                return Err(AtomError::InvalidUseDependency);
            }
        }

        let had_conditional_suffix = flag.ends_with('=') || flag.ends_with('?');
        if flag.ends_with("(+)") || flag.ends_with("(-)") {
            flag = &flag[..flag.len() - 3];
        }
        if flag.ends_with('=') || flag.ends_with('?') {
            flag = &flag[..flag.len() - 1];
        }
        if flag.ends_with("(+)") || flag.ends_with("(-)") {
            flag = &flag[..flag.len() - 3];
        }

        if bang && !had_conditional_suffix {
            return Err(AtomError::InvalidUseDependency);
        }

        if flag.is_empty() || !is_valid_flag(flag) {
            return Err(AtomError::InvalidUseDependency);
        }
        if seen.contains(&flag) {
            return Err(AtomError::InvalidUseDependency);
        }
        seen.push(flag);
    }
    Ok(())
}

fn strip_repo(input: &str, allow_repo: bool) -> Result<(&str, Option<String>), AtomError> {
    let Some(index) = input.find("::") else {
        return Ok((input, None));
    };
    if !allow_repo {
        return Err(AtomError::RepositoryNotAllowed);
    }
    let (left, repo) = input.split_at(index);
    let repo = &repo[2..];
    if repo.is_empty() || repo.contains(':') || !is_valid_repo(repo) {
        return Err(AtomError::InvalidRepository);
    }
    Ok((left, Some(repo.to_string())))
}

fn strip_slot(input: &str) -> Result<(&str, Option<SlotDependency>), AtomError> {
    let Some(index) = input.find(':') else {
        return Ok((input, None));
    };
    let (left, slot_text) = input.split_at(index);
    let slot_text = &slot_text[1..];
    if slot_text.is_empty() || slot_text.contains(':') {
        return Err(AtomError::InvalidSlot);
    }
    Ok((left, Some(parse_slot(slot_text)?)))
}

fn parse_slot(input: &str) -> Result<SlotDependency, AtomError> {
    if input == "=" || input == "*" {
        return Ok(SlotDependency {
            slot: None,
            sub_slot: None,
            operator: Some(input.to_string()),
        });
    }

    let (input, operator) = if let Some(slot) = input.strip_suffix('=') {
        (slot, Some("=".to_string()))
    } else if let Some(slot) = input.strip_suffix('*') {
        (slot, Some("*".to_string()))
    } else {
        (input, None)
    };

    if input.is_empty() || input.ends_with('/') {
        return Err(AtomError::InvalidSlot);
    }

    let mut parts = input.split('/');
    let slot = parts.next().ok_or(AtomError::InvalidSlot)?;
    let sub_slot = parts.next();
    if parts.next().is_some() || slot.is_empty() || !is_valid_slot_part(slot) {
        return Err(AtomError::InvalidSlot);
    }
    if let Some(sub_slot) = sub_slot {
        if sub_slot.is_empty() || !is_valid_slot_part(sub_slot) || operator.as_deref() == Some("*")
        {
            return Err(AtomError::InvalidSlot);
        }
        return Ok(SlotDependency {
            slot: Some(slot.to_string()),
            sub_slot: Some(sub_slot.to_string()),
            operator,
        });
    }

    Ok(SlotDependency {
        slot: Some(slot.to_string()),
        sub_slot: None,
        operator,
    })
}

fn split_category_package(input: &str) -> Result<(&str, &str), AtomError> {
    let mut parts = input.split('/');
    let category = parts.next().ok_or(AtomError::InvalidCategoryPackage)?;
    let package = parts.next().ok_or(AtomError::InvalidCategoryPackage)?;
    if parts.next().is_some() || category.is_empty() || package.is_empty() {
        return Err(AtomError::InvalidCategoryPackage);
    }
    Ok((category, package))
}

fn split_package_version(input: &str, glob: bool) -> Result<(String, Option<String>), AtomError> {
    let candidate = if glob {
        input.strip_suffix('*').ok_or(AtomError::InvalidVersion)?
    } else {
        input
    };
    for index in candidate.match_indices('-').map(|(index, _)| index).rev() {
        let package = &candidate[..index];
        let version = &candidate[index + 1..];
        if !package.is_empty() && is_version(version) {
            return Ok((package.to_string(), Some(version.to_string())));
        }
    }
    Err(AtomError::OperatorRequiresVersion)
}

fn looks_versioned(input: &str) -> bool {
    input
        .match_indices('-')
        .map(|(index, _)| &input[index + 1..])
        .any(is_version)
}

fn is_version(input: &str) -> bool {
    if input.is_empty() {
        return false;
    }
    let without_revision = match input.rsplit_once("-r") {
        Some((base, rev)) if !base.is_empty() && rev.chars().all(|c| c.is_ascii_digit()) => base,
        Some(_) => input,
        None => input,
    };
    let starts_like_version = without_revision
        .chars()
        .next()
        .is_some_and(|c| c.is_ascii_digit() || c == '*');
    starts_like_version
        && without_revision
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-' | '*'))
}

fn validate_name(input: &str) -> Result<(), AtomError> {
    if input.is_empty()
        || matches!(input.as_bytes()[0], b'.' | b'+' | b'-')
        || input
            .bytes()
            .any(|b| !(b.is_ascii_alphanumeric() || matches!(b, b'_' | b'+' | b'-' | b'.')))
    {
        return Err(AtomError::InvalidCategoryPackage);
    }
    Ok(())
}

fn validate_wildcard_name(input: &str) -> Result<(), AtomError> {
    if input.is_empty()
        || input.contains("**")
        || input
            .bytes()
            .any(|b| !(b.is_ascii_alphanumeric() || matches!(b, b'_' | b'+' | b'-' | b'.' | b'*')))
    {
        return Err(AtomError::InvalidCategoryPackage);
    }
    Ok(())
}

fn is_valid_repo(input: &str) -> bool {
    input
        .bytes()
        .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'_' | b'-' | b'.'))
}

fn is_valid_flag(input: &str) -> bool {
    input
        .bytes()
        .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'_' | b'+' | b'-' | b'@'))
}

fn is_valid_slot_part(input: &str) -> bool {
    input
        .bytes()
        .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'_' | b'+' | b'-' | b'.'))
}
