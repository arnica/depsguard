// Version parsing and comparison utilities.

/// Parse a semantic version string into (major, minor, patch).
pub fn parse_semver(version: &str) -> Option<(u64, u64, u64)> {
    let version = version.trim();
    let mut parts = version.splitn(3, '.');
    let major: u64 = parts.next()?.parse().ok()?;
    let minor: u64 = parts.next()?.parse().ok()?;
    let patch_str = parts.next().unwrap_or("0");
    let patch: u64 = patch_str
        .split(|c: char| !c.is_ascii_digit())
        .next()
        .unwrap_or("0")
        .parse()
        .unwrap_or(0);
    Some((major, minor, patch))
}

/// Check if a version is at least (min_major, min_minor).
pub fn version_at_least(version: &str, min_major: u64, min_minor: u64) -> bool {
    match parse_semver(version) {
        Some((major, minor, _)) => major > min_major || (major == min_major && minor >= min_minor),
        None => false,
    }
}

/// Extract the leading semantic-version substring from a tool's `--version` output.
///
/// Many tools prefix or wrap their version (e.g. `"pip 26.1 from ..."` or
/// `"Poetry (version 2.4.0)"`); this returns just the `MAJOR.MINOR.PATCH` portion
/// (`"26.1"`, `"2.4.0"`). If no digits are present, the trimmed input is returned.
pub fn extract_version_str(version: &str) -> &str {
    let s = version.trim();
    let Some(numeric_start) = s.find(|c: char| c.is_ascii_digit()) else {
        return s;
    };
    let rest = &s[numeric_start..];
    rest.split(|c: char| !c.is_ascii_digit() && c != '.')
        .next()
        .unwrap_or(rest)
}
