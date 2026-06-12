// Docker scanners: Docker Compose service images and Dockerfile FROM images.

use std::collections::HashSet;
use std::fs;
use std::path::Path;

use super::config::{read_flat_config, read_ini_value, read_toml_value, read_yaml_value};
use super::date::{parse_iso8601_days, parse_relative_days};
use super::detect::get_delay_days;
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

    let instructions = dockerfile_instructions(&content);
    let mut stages = HashSet::new();
    let mut recs = Vec::new();
    let mut hardening = HardeningState::default();
    let context = path.parent().unwrap_or_else(|| Path::new("."));

    for instruction in &instructions {
        let Some(from) = dockerfile_from(&instruction.text) else {
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
        if let Some(rec) = image_recommendation("base image", instruction.line_no, &image) {
            recs.push(rec);
        }
    }

    for instruction in &instructions {
        update_hardening_from_instruction(&mut hardening, context, &instruction.text);
        recs.extend(package_manager_recommendations(
            &hardening,
            instruction.line_no,
            &instruction.text,
        ));
    }
    recs
}

struct DockerInstruction {
    line_no: usize,
    text: String,
}

fn dockerfile_instructions(content: &str) -> Vec<DockerInstruction> {
    let mut instructions = Vec::new();
    let mut current = String::new();
    let mut start_line = 1;
    for (idx, raw) in content.lines().enumerate() {
        let line_no = idx + 1;
        let trimmed_end = raw.trim_end();
        if current.is_empty() {
            start_line = line_no;
        } else {
            current.push(' ');
        }

        if let Some(without_slash) = trimmed_end.strip_suffix('\\') {
            current.push_str(without_slash.trim_end());
        } else {
            current.push_str(trimmed_end);
            if !current.trim().is_empty() {
                instructions.push(DockerInstruction {
                    line_no: start_line,
                    text: current.trim().to_string(),
                });
            }
            current.clear();
        }
    }

    if !current.trim().is_empty() {
        instructions.push(DockerInstruction {
            line_no: start_line,
            text: current.trim().to_string(),
        });
    }

    instructions
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

#[derive(Default)]
struct HardeningState {
    npm_ignore_scripts: bool,
    npm_release_age: bool,
    pnpm_ignore_scripts: bool,
    pnpm_release_age: bool,
    yarn_release_age: bool,
    bun_release_age: bool,
    pip_release_age: bool,
    uv_release_age: bool,
    poetry_release_age: bool,
}

fn update_hardening_from_instruction(state: &mut HardeningState, context: &Path, text: &str) {
    let normalized = normalized_command(text);
    update_hardening_from_command(state, &normalized);

    if let Some(sources) = copy_sources(text) {
        for source in sources {
            update_hardening_from_copied_file(state, context, &source);
        }
    }
}

fn update_hardening_from_command(state: &mut HardeningState, normalized: &str) {
    let days = get_delay_days();
    let minutes = days.saturating_mul(24).saturating_mul(60);
    let seconds = days.saturating_mul(86400);

    if normalized.contains("npm config set ignore-scripts true")
        || normalized.contains("npm config set ignore-scripts=true")
    {
        state.npm_ignore_scripts = true;
    }
    if normalized.contains(&format!("npm config set min-release-age {days}"))
        || normalized.contains(&format!("npm config set min-release-age={days}"))
    {
        state.npm_release_age = true;
    }

    if normalized.contains("pnpm config set ignore-scripts true")
        || normalized.contains("pnpm config set ignore-scripts=true")
    {
        state.pnpm_ignore_scripts = true;
    }
    if normalized.contains(&format!("pnpm config set minimum-release-age {minutes}"))
        || normalized.contains(&format!("pnpm config set minimum-release-age={minutes}"))
        || normalized.contains(&format!("pnpm config set minimumreleaseage {minutes}"))
        || normalized.contains(&format!("pnpm config set minimumreleaseage={minutes}"))
    {
        state.pnpm_release_age = true;
    }

    if normalized.contains(&format!("yarn config set npmminimalagegate {days}d"))
        || normalized.contains(&format!("yarn config set npmminimalagegate={days}d"))
    {
        state.yarn_release_age = true;
    }

    if normalized.contains(&format!(
        "bun config set install.minimumreleaseage {seconds}"
    )) || normalized.contains(&format!(
        "bun config set install.minimumreleaseage={seconds}"
    )) {
        state.bun_release_age = true;
    }

    if normalized.contains(&format!(
        "pip config set install.uploaded-prior-to p{days}d"
    )) || normalized.contains(&format!(
        "pip3 config set install.uploaded-prior-to p{days}d"
    )) || normalized.contains(&format!(
        "pip config set install.uploaded-prior-to=p{days}d"
    )) || normalized.contains(&format!(
        "pip3 config set install.uploaded-prior-to=p{days}d"
    )) {
        state.pip_release_age = true;
    }

    if normalized.contains(&format!("uv config set exclude-newer {days} days"))
        || normalized.contains(&format!("uv config set exclude-newer={days} days"))
    {
        state.uv_release_age = true;
    }

    if normalized.contains(&format!("poetry config solver.min-release-age {days}"))
        || normalized.contains(&format!("poetry config solver.min-release-age={days}"))
    {
        state.poetry_release_age = true;
    }
}

fn update_hardening_from_copied_file(state: &mut HardeningState, context: &Path, source: &str) {
    let source = source.trim_matches('"').trim_matches('\'');
    if source.contains('*') || source.starts_with("http://") || source.starts_with("https://") {
        return;
    }
    let path = context.join(source);
    let name = match path.file_name().and_then(|n| n.to_str()) {
        Some(name) => name,
        None => return,
    };
    let days = get_delay_days();
    let minutes = days.saturating_mul(24).saturating_mul(60);
    let seconds = days.saturating_mul(86400);

    match name {
        ".npmrc" => {
            let cfg = read_flat_config(&path);
            if cfg.get("ignore-scripts").is_some_and(|v| v == "true") {
                state.npm_ignore_scripts = true;
                state.pnpm_ignore_scripts = true;
            }
            if cfg
                .get("min-release-age")
                .and_then(|v| v.parse::<u64>().ok())
                == Some(days)
            {
                state.npm_release_age = true;
            }
            if cfg
                .get("minimum-release-age")
                .and_then(|v| v.parse::<u64>().ok())
                == Some(minutes)
            {
                state.pnpm_release_age = true;
            }
        }
        "pnpm-workspace.yaml" => {
            if read_yaml_value(&path, "ignoreScripts").as_deref() == Some("true") {
                state.pnpm_ignore_scripts = true;
            }
            if read_yaml_value(&path, "minimumReleaseAge").and_then(|v| v.parse::<u64>().ok())
                == Some(minutes)
            {
                state.pnpm_release_age = true;
            }
        }
        ".yarnrc.yml" => {
            if read_yaml_value(&path, "npmMinimalAgeGate")
                .and_then(|v| super::date::parse_duration_minutes(&v))
                == Some(minutes)
            {
                state.yarn_release_age = true;
            }
        }
        ".bunfig.toml" | "bunfig.toml" => {
            if read_toml_value(&path, "install.minimumReleaseAge")
                .and_then(|v| v.parse::<u64>().ok())
                == Some(seconds)
            {
                state.bun_release_age = true;
            }
        }
        "pip.conf" | "pip.ini" => {
            if read_ini_value(&path, "install.uploaded-prior-to")
                .and_then(|v| parse_iso8601_days(&v))
                == Some(days)
            {
                state.pip_release_age = true;
            }
        }
        "uv.toml" => {
            if read_toml_value(&path, "exclude-newer").and_then(|v| parse_relative_days(&v))
                == Some(days)
            {
                state.uv_release_age = true;
            }
        }
        "poetry.toml" | "config.toml"
            if read_toml_value(&path, "solver.min-release-age")
                .and_then(|v| v.parse::<u64>().ok())
                == Some(days) =>
        {
            state.poetry_release_age = true;
        }
        _ => {}
    }
}

fn package_manager_recommendations(
    state: &HardeningState,
    line_no: usize,
    text: &str,
) -> Vec<Recommendation> {
    let Some(command) = run_command(text) else {
        return Vec::new();
    };
    let normalized = normalized_command(command);
    let mut recs = Vec::new();
    let days = get_delay_days();
    let minutes = days.saturating_mul(24).saturating_mul(60);
    let seconds = days.saturating_mul(86400);

    if npm_install(&normalized)
        && !(state.npm_ignore_scripts || command_has_flag(&normalized, "--ignore-scripts"))
    {
        recs.push(package_manager_rec(
            "npm ignore-scripts",
            line_no,
            command,
            "Configure npm install-script blocking before Docker build installs packages",
            "npm config set ignore-scripts true before npm install",
        ));
    }
    if npm_install(&normalized)
        && !(state.npm_release_age
            || command_has_value_flag(&normalized, "--min-release-age", &days.to_string()))
    {
        recs.push(package_manager_rec(
            "npm release age",
            line_no,
            command,
            "Configure npm release-age delay before Docker build installs packages",
            &format!("npm config set min-release-age {days} before npm install"),
        ));
    }

    if pnpm_install(&normalized)
        && !(state.pnpm_ignore_scripts || command_has_flag(&normalized, "--ignore-scripts"))
    {
        recs.push(package_manager_rec(
            "pnpm ignore-scripts",
            line_no,
            command,
            "Configure pnpm install-script blocking before Docker build installs packages",
            "pnpm config set ignore-scripts true before pnpm install",
        ));
    }
    if pnpm_install(&normalized)
        && !(state.pnpm_release_age
            || command_has_value_flag(&normalized, "--minimum-release-age", &minutes.to_string()))
    {
        recs.push(package_manager_rec(
            "pnpm release age",
            line_no,
            command,
            "Configure pnpm release-age delay before Docker build installs packages",
            &format!("pnpm config set minimum-release-age {minutes} before pnpm install"),
        ));
    }

    if yarn_install(&normalized) && !state.yarn_release_age {
        recs.push(package_manager_rec(
            "yarn release age",
            line_no,
            command,
            "Configure Yarn release-age delay before Docker build installs packages",
            &format!("yarn config set npmMinimalAgeGate {days}d before yarn install"),
        ));
    }

    if bun_install(&normalized)
        && !(state.bun_release_age
            || command_has_value_flag(&normalized, "--minimum-release-age", &seconds.to_string()))
    {
        recs.push(package_manager_rec(
            "bun release age",
            line_no,
            command,
            "Configure Bun release-age delay before Docker build installs packages",
            &format!("bunfig install.minimumReleaseAge = {seconds} before bun install"),
        ));
    }

    if pip_install(&normalized)
        && !(state.pip_release_age
            || command_has_value_flag(&normalized, "--uploaded-prior-to", &format!("p{days}d")))
    {
        recs.push(package_manager_rec(
            "pip release age",
            line_no,
            command,
            "Configure pip upload-age delay before Docker build installs packages",
            &format!("pip config set install.uploaded-prior-to P{days}D before pip install"),
        ));
    }

    if uv_install(&normalized)
        && !(state.uv_release_age
            || command_has_value_flag(&normalized, "--exclude-newer", &format!("{days} days")))
    {
        recs.push(package_manager_rec(
            "uv exclude-newer",
            line_no,
            command,
            "Configure uv release-age delay before Docker build installs packages",
            &format!("uv.toml exclude-newer = \"{days} days\" before uv install"),
        ));
    }

    if poetry_install(&normalized) && !state.poetry_release_age {
        recs.push(package_manager_rec(
            "poetry release age",
            line_no,
            command,
            "Configure Poetry release-age delay before Docker build installs packages",
            &format!("poetry config solver.min-release-age {days} before poetry install"),
        ));
    }

    recs
}

fn package_manager_rec(
    kind: &str,
    line_no: usize,
    command: &str,
    description: &str,
    expected: &str,
) -> Recommendation {
    Recommendation {
        key: format!("{kind} line {line_no}"),
        description: description.into(),
        expected: expected.into(),
        status: CheckStatus::WrongValue(compact(command)),
    }
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

fn run_command(text: &str) -> Option<&str> {
    let text = strip_inline_comment(text);
    let trimmed = text.trim_start();
    let rest = trimmed.get(3..)?;
    if trimmed[..3].eq_ignore_ascii_case("RUN")
        && trimmed.chars().nth(3).map_or(true, |ch| ch.is_whitespace())
    {
        Some(rest.trim())
    } else {
        None
    }
}

fn copy_sources(text: &str) -> Option<Vec<String>> {
    let text = strip_inline_comment(text);
    let mut parts = text.split_whitespace();
    let instr = parts.next()?;
    if !instr.eq_ignore_ascii_case("COPY") && !instr.eq_ignore_ascii_case("ADD") {
        return None;
    }

    let mut args: Vec<String> = parts
        .filter(|part| !part.starts_with("--"))
        .map(|part| part.trim_end_matches(',').to_string())
        .collect();
    if args.len() < 2 {
        let quoted = quoted_values(text);
        if quoted.len() >= 2 {
            args = quoted;
        }
    }
    if args.len() < 2 {
        return None;
    }
    args.pop();
    Some(args)
}

fn quoted_values(text: &str) -> Vec<String> {
    let mut values = Vec::new();
    let mut in_quote = false;
    let mut current = String::new();
    for ch in text.chars() {
        match ch {
            '"' if in_quote => {
                values.push(current.clone());
                current.clear();
                in_quote = false;
            }
            '"' => in_quote = true,
            _ if in_quote => current.push(ch),
            _ => {}
        }
    }
    values
}

fn normalized_command(command: &str) -> String {
    command
        .to_ascii_lowercase()
        .replace(['\\', '\t', '\n', '"', '\''], " ")
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

fn npm_install(command: &str) -> bool {
    has_token_sequence(command, &["npm", "install"])
        || has_token_sequence(command, &["npm", "ci"])
        || has_token_sequence(command, &["npm", "i"])
}

fn pnpm_install(command: &str) -> bool {
    has_token_sequence(command, &["pnpm", "install"]) || has_token_sequence(command, &["pnpm", "i"])
}

fn yarn_install(command: &str) -> bool {
    has_token_sequence(command, &["yarn", "install"])
}

fn bun_install(command: &str) -> bool {
    has_token_sequence(command, &["bun", "install"])
}

fn pip_install(command: &str) -> bool {
    !has_token_sequence(command, &["uv", "pip", "install"])
        && (has_token_sequence(command, &["pip", "install"])
            || has_token_sequence(command, &["pip3", "install"]))
}

fn uv_install(command: &str) -> bool {
    has_token_sequence(command, &["uv", "sync"])
        || has_token_sequence(command, &["uv", "pip", "install"])
}

fn poetry_install(command: &str) -> bool {
    has_token_sequence(command, &["poetry", "install"])
}

fn has_token_sequence(command: &str, seq: &[&str]) -> bool {
    command
        .split_whitespace()
        .collect::<Vec<_>>()
        .windows(seq.len())
        .any(|window| window == seq)
}

fn command_has_flag(command: &str, flag: &str) -> bool {
    command.split_whitespace().any(|token| token == flag)
}

fn command_has_value_flag(command: &str, flag: &str, expected: &str) -> bool {
    let expected = expected.to_ascii_lowercase();
    if command.contains(&format!("{flag} {expected}"))
        || command.contains(&format!("{flag}={expected}"))
    {
        return true;
    }
    let eq = format!("{flag}={expected}");
    let tokens: Vec<&str> = command.split_whitespace().collect();
    for (idx, token) in tokens.iter().enumerate() {
        if *token == eq {
            return true;
        }
        if *token == flag && tokens.get(idx + 1).copied() == Some(expected.as_str()) {
            return true;
        }
    }
    false
}

fn compact(command: &str) -> String {
    command.split_whitespace().collect::<Vec<_>>().join(" ")
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
    use crate::manager::detect::set_delay_days;
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

    fn tmp_dir(name: &str) -> TmpDir {
        let path = std::env::temp_dir().join(format!(
            "depsguard_docker_{name}_{}_{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir_all(&path).unwrap();
        TmpDir(path)
    }

    struct TmpDir(std::path::PathBuf);
    impl TmpDir {
        fn path(&self) -> &Path {
            &self.0
        }
    }
    impl Drop for TmpDir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
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

    #[test]
    fn dockerfile_flags_npm_install_without_prior_hardening() {
        set_delay_days(7);
        let f = tmp_file("FROM node:22@sha256:abc\nRUN npm ci\n");
        let recs = scan_dockerfile(f.path());
        assert_eq!(recs.len(), 2);
        assert_eq!(recs[0].key, "npm ignore-scripts line 2");
        assert_eq!(recs[1].key, "npm release age line 2");
    }

    #[test]
    fn dockerfile_accepts_npm_config_before_install() {
        set_delay_days(7);
        let f = tmp_file(
            "FROM node:22@sha256:abc\nRUN npm config set ignore-scripts true && npm config set min-release-age 7\nRUN npm ci\n",
        );
        assert!(scan_dockerfile(f.path()).is_empty());
    }

    #[test]
    fn dockerfile_reports_npm_config_after_install_as_too_late() {
        set_delay_days(7);
        let f = tmp_file(
            "FROM node:22@sha256:abc\nRUN npm ci\nRUN npm config set ignore-scripts true && npm config set min-release-age 7\n",
        );
        let recs = scan_dockerfile(f.path());
        assert_eq!(recs.len(), 2);
        assert!(recs.iter().all(|r| r.key.ends_with("line 2")));
    }

    #[test]
    fn dockerfile_accepts_copied_npmrc_before_install() {
        set_delay_days(7);
        let dir = tmp_dir("npmrc");
        fs::write(
            dir.path().join(".npmrc"),
            "ignore-scripts=true\nmin-release-age=7\n",
        )
        .unwrap();
        let dockerfile = dir.path().join("Dockerfile");
        fs::write(
            &dockerfile,
            "FROM node:22@sha256:abc\nCOPY .npmrc ./\nRUN npm ci\n",
        )
        .unwrap();

        assert!(scan_dockerfile(&dockerfile).is_empty());
    }

    #[test]
    fn dockerfile_flags_pnpm_install_without_prior_hardening() {
        set_delay_days(7);
        let f = tmp_file("FROM node:22@sha256:abc\nRUN pnpm install --frozen-lockfile\n");
        let recs = scan_dockerfile(f.path());
        assert_eq!(recs.len(), 2);
        assert_eq!(recs[0].key, "pnpm ignore-scripts line 2");
        assert_eq!(recs[1].key, "pnpm release age line 2");
    }

    #[test]
    fn dockerfile_accepts_pip_inline_uploaded_prior_to() {
        set_delay_days(7);
        let f = tmp_file(
            "FROM python:3.12@sha256:abc\nRUN pip install --uploaded-prior-to=P7D flask\n",
        );
        assert!(scan_dockerfile(f.path()).is_empty());
    }

    #[test]
    fn dockerfile_flags_pip_install_without_prior_hardening() {
        set_delay_days(7);
        let f = tmp_file("FROM python:3.12@sha256:abc\nRUN pip install flask\n");
        let recs = scan_dockerfile(f.path());
        assert_eq!(recs.len(), 1);
        assert_eq!(recs[0].key, "pip release age line 2");
    }

    #[test]
    fn dockerfile_accepts_uv_inline_exclude_newer() {
        set_delay_days(7);
        let f = tmp_file("FROM python:3.12@sha256:abc\nRUN uv sync --exclude-newer \"7 days\"\n");
        assert!(scan_dockerfile(f.path()).is_empty());
    }

    #[test]
    fn dockerfile_flags_poetry_install_without_prior_hardening() {
        set_delay_days(7);
        let f = tmp_file("FROM python:3.12@sha256:abc\nRUN poetry install\n");
        let recs = scan_dockerfile(f.path());
        assert_eq!(recs.len(), 1);
        assert_eq!(recs[0].key, "poetry release age line 2");
    }

    #[test]
    fn dockerfile_handles_continued_run_lines() {
        set_delay_days(7);
        let f = tmp_file(
            "FROM node:22@sha256:abc\nRUN npm config set ignore-scripts true \\\n    && npm config set min-release-age 7 \\\n    && npm ci\n",
        );
        assert!(scan_dockerfile(f.path()).is_empty());
    }
}
