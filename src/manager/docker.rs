// Docker scanners: Docker Compose service images and Dockerfile FROM images.

use std::collections::HashSet;
use std::fs;
use std::path::Path;

use super::types::{CheckStatus, Recommendation};

/// Scan Docker Compose-style YAML for `image:` references.
pub fn scan_compose(path: &Path) -> Vec<Recommendation> {
    let Ok(content) = fs::read_to_string(path) else {
        return Vec::new();
    };

    content
        .lines()
        .enumerate()
        .filter_map(|(idx, line)| compose_image_value(line).map(|image| (idx + 1, image)))
        .filter_map(|(line_no, image)| image_recommendation("compose image", line_no, &image))
        .collect()
}

/// Scan Dockerfile `FROM` instructions.
pub fn scan_dockerfile(path: &Path) -> Vec<Recommendation> {
    let Ok(content) = fs::read_to_string(path) else {
        return Vec::new();
    };

    let mut stages = HashSet::new();
    let mut recs = Vec::new();
    for (idx, line) in content.lines().enumerate() {
        let Some(from) = dockerfile_from(line) else {
            continue;
        };
        let image = from.image;
        let is_stage_ref = stages.contains(image.as_str());
        if let Some(alias) = from.alias {
            stages.insert(alias);
        }
        if image == "scratch" || is_stage_ref {
            continue;
        }
        if let Some(rec) = image_recommendation("base image", idx + 1, &image) {
            recs.push(rec);
        }
    }
    recs
}

fn compose_image_value(line: &str) -> Option<String> {
    let stripped = strip_inline_comment(line).trim().to_string();
    let trimmed = stripped.trim_start();
    let rest = trimmed.strip_prefix("image")?.trim_start();
    let rest = rest.strip_prefix(':')?.trim();
    if rest.is_empty() {
        return None;
    }
    Some(unquote(rest).to_string())
}

struct FromInstruction {
    image: String,
    alias: Option<String>,
}

fn dockerfile_from(line: &str) -> Option<FromInstruction> {
    let line = strip_inline_comment(line);
    let mut parts = line.split_whitespace();
    let instr = parts.next()?;
    if !instr.eq_ignore_ascii_case("FROM") {
        return None;
    }

    let mut image = parts.next()?;
    while image.starts_with("--") {
        image = parts.next()?;
    }

    let mut alias = None;
    while let Some(part) = parts.next() {
        if part.eq_ignore_ascii_case("AS") {
            alias = parts.next().map(ToOwned::to_owned);
            break;
        }
    }

    Some(FromInstruction {
        image: image.to_string(),
        alias,
    })
}

fn image_recommendation(kind: &str, line_no: usize, image: &str) -> Option<Recommendation> {
    let reference = ImageReference::parse(image);
    let (description, expected, status) = if reference.has_latest_tag() {
        (
            "Avoid mutable Docker image tags",
            "version tag or digest-pinned image".to_string(),
            CheckStatus::WrongValue(image.to_string()),
        )
    } else if !reference.has_tag() && !reference.has_digest {
        (
            "Avoid Docker images without explicit tags",
            "version tag or digest-pinned image".to_string(),
            CheckStatus::WrongValue(image.to_string()),
        )
    } else if !reference.has_digest {
        (
            "Prefer digest-pinned Docker images",
            format!("{image}@sha256:<digest>"),
            CheckStatus::Unsupported(format!("not digest-pinned: {image}")),
        )
    } else {
        return None;
    };

    Some(Recommendation {
        key: format!("{kind} line {line_no}"),
        description: description.into(),
        expected,
        status,
    })
}

struct ImageReference<'a> {
    tag: Option<&'a str>,
    has_digest: bool,
}

impl<'a> ImageReference<'a> {
    fn parse(image: &'a str) -> Self {
        let (without_digest, has_digest) = match image.split_once('@') {
            Some((name, _)) => (name, true),
            None => (image, false),
        };
        let slash = without_digest.rfind('/');
        let colon = without_digest.rfind(':');
        let tag = match (slash, colon) {
            (_, Some(c)) if slash.map_or(true, |s| c > s) => Some(&without_digest[c + 1..]),
            _ => None,
        };
        Self { tag, has_digest }
    }

    fn has_tag(&self) -> bool {
        self.tag.is_some()
    }

    fn has_latest_tag(&self) -> bool {
        self.tag == Some("latest")
    }
}

fn strip_inline_comment(line: &str) -> &str {
    let mut in_single = false;
    let mut in_double = false;
    let mut prev_was_space = true;
    for (idx, ch) in line.char_indices() {
        match ch {
            '\'' if !in_double => in_single = !in_single,
            '"' if !in_single => in_double = !in_double,
            '#' if !in_single && !in_double && prev_was_space => return &line[..idx],
            _ => {}
        }
        prev_was_space = ch.is_whitespace();
    }
    line
}

fn unquote(value: &str) -> &str {
    value
        .strip_prefix('"')
        .and_then(|v| v.strip_suffix('"'))
        .or_else(|| value.strip_prefix('\'').and_then(|v| v.strip_suffix('\'')))
        .unwrap_or(value)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn tmp_file(content: &str) -> TmpFile {
        use std::sync::atomic::{AtomicU64, Ordering};
        static COUNTER: AtomicU64 = AtomicU64::new(0);
        let n = COUNTER.fetch_add(1, Ordering::Relaxed);
        let id = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path = std::env::temp_dir().join(format!(
            "depsguard_docker_test_{id}_{}_{n}",
            std::process::id()
        ));
        let mut f = fs::File::create(&path).unwrap();
        f.write_all(content.as_bytes()).unwrap();
        TmpFile(path)
    }

    struct TmpFile(std::path::PathBuf);
    impl TmpFile {
        fn path(&self) -> &Path {
            &self.0
        }
    }
    impl Drop for TmpFile {
        fn drop(&mut self) {
            let _ = fs::remove_file(&self.0);
        }
    }

    #[test]
    fn compose_flags_latest_tag() {
        let f = tmp_file("services:\n  app:\n    image: ghcr.io/example/app:latest\n");
        let recs = scan_compose(f.path());
        assert_eq!(recs.len(), 1);
        assert!(matches!(recs[0].status, CheckStatus::WrongValue(_)));
        assert_eq!(recs[0].key, "compose image line 3");
    }

    #[test]
    fn compose_flags_missing_tag() {
        let f = tmp_file("services:\n  db:\n    image: postgres\n");
        let recs = scan_compose(f.path());
        assert_eq!(recs.len(), 1);
        assert!(matches!(recs[0].status, CheckStatus::WrongValue(_)));
    }

    #[test]
    fn compose_warns_without_digest() {
        let f = tmp_file("services:\n  db:\n    image: \"postgres:16\"\n");
        let recs = scan_compose(f.path());
        assert_eq!(recs.len(), 1);
        assert!(recs[0].status.is_unsupported());
    }

    #[test]
    fn compose_accepts_digest_pin() {
        let f = tmp_file("services:\n  db:\n    image: postgres:16@sha256:abc\n");
        assert!(scan_compose(f.path()).is_empty());
    }

    #[test]
    fn dockerfile_flags_latest_from() {
        let f = tmp_file("FROM node:latest\n");
        let recs = scan_dockerfile(f.path());
        assert_eq!(recs.len(), 1);
        assert!(matches!(recs[0].status, CheckStatus::WrongValue(_)));
        assert_eq!(recs[0].key, "base image line 1");
    }

    #[test]
    fn dockerfile_flags_missing_tag() {
        let f = tmp_file("FROM ubuntu\n");
        let recs = scan_dockerfile(f.path());
        assert_eq!(recs.len(), 1);
        assert!(matches!(recs[0].status, CheckStatus::WrongValue(_)));
    }

    #[test]
    fn dockerfile_ignores_scratch_and_stage_refs() {
        let f = tmp_file("FROM rust:1 AS builder\nFROM scratch\nFROM builder AS final\n");
        let recs = scan_dockerfile(f.path());
        assert_eq!(recs.len(), 1, "only rust:1 should warn for missing digest");
        assert_eq!(recs[0].key, "base image line 1");
    }

    #[test]
    fn dockerfile_handles_platform_flag() {
        let f = tmp_file("FROM --platform=$BUILDPLATFORM node:22\n");
        let recs = scan_dockerfile(f.path());
        assert_eq!(recs.len(), 1);
        assert!(recs[0].status.is_unsupported());
    }

    #[test]
    fn registry_port_is_not_tag() {
        let parsed = ImageReference::parse("localhost:5000/example/app");
        assert!(!parsed.has_tag());
        let parsed = ImageReference::parse("localhost:5000/example/app:1.0");
        assert_eq!(parsed.tag, Some("1.0"));
    }
}
