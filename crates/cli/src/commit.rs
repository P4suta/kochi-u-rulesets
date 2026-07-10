//! `commit` subcommand — stage the changes under one or more paths and land them
//! on the current branch as a signed, Verified commit via GitHub's
//! `createCommitOnBranch` GraphQL mutation (commits made with `GITHUB_TOKEN` are
//! auto-signed, satisfying a require-signed-commits ruleset that a plain `git
//! push` from Actions cannot).
//!
//! Building the file-change payload in Rust is type-safe, O(n), and
//! unit-testable — no jq-in-a-loop or quoting foot-guns. Runs only in GitHub
//! Actions (needs `GITHUB_TOKEN` / `GITHUB_REPOSITORY` / `GITHUB_REF_NAME`).

use std::path::PathBuf;
use std::process::Command;

use anyhow::{Context, Result, bail};
use base64::Engine;
use serde_json::{Value, json};

const API: &str = "https://api.github.com";
const USER_AGENT: &str = "kochi-u-rulesets commit (+https://github.com/P4suta/kochi-u-rulesets)";

#[derive(clap::Args)]
pub struct CommitArgs {
    /// Directory or file whose changes to commit. Repeatable (e.g. `--path
    /// sources --path out --path manifest.json`).
    #[arg(long, required = true)]
    path: Vec<String>,
    /// Commit message headline.
    #[arg(long)]
    message: String,
}

/// Additions (with contents) and deletions to send to `createCommitOnBranch`.
#[derive(Debug, Default, PartialEq)]
struct Changes {
    additions: Vec<String>,
    deletions: Vec<String>,
}

impl Changes {
    fn is_empty(&self) -> bool {
        self.additions.is_empty() && self.deletions.is_empty()
    }
    fn len(&self) -> usize {
        self.additions.len() + self.deletions.len()
    }
}

pub fn run(args: CommitArgs) -> Result<()> {
    let repo = env("GITHUB_REPOSITORY")?;
    let branch = env("GITHUB_REF_NAME")?;
    let token = env("GITHUB_TOKEN")?;

    // Stage and diff every requested path in one shot.
    let mut add = vec!["-c", "core.quotepath=false", "add", "-A", "--"];
    add.extend(args.path.iter().map(String::as_str));
    git(&add)?;

    let mut diff_cmd = vec![
        "-c",
        "core.quotepath=false",
        "diff",
        "--cached",
        "--name-status",
        "--",
    ];
    diff_cmd.extend(args.path.iter().map(String::as_str));
    let diff = git_stdout(&diff_cmd)?;
    let changes = parse_diff(&diff);

    if changes.is_empty() {
        println!("変更なし: {}", args.path.join(", "));
        set_output("changed", "false")?;
        return Ok(());
    }

    let http = reqwest::blocking::Client::builder()
        .user_agent(USER_AGENT)
        .build()
        .context("failed to build HTTP client")?;

    let head = branch_head_oid(&http, &repo, &branch, &token)?;
    let file_changes = build_file_changes(&changes)?;
    let url = create_commit(
        &http,
        &repo,
        &branch,
        &head,
        &args.message,
        file_changes,
        &token,
    )?;

    println!(
        "{} 件の変更を {branch} にコミットしました: {url}",
        changes.len()
    );
    set_output("changed", "true")?;
    Ok(())
}

/// Parse `git diff --cached --name-status` output into additions / deletions.
/// `A`/`M`/`T` → add the path; `D` → delete; `R…`/`C…` → delete old, add new.
fn parse_diff(diff: &str) -> Changes {
    let mut changes = Changes::default();
    for line in diff.lines() {
        let mut cols = line.split('\t');
        let Some(status) = cols.next() else { continue };
        let tag = status.chars().next().unwrap_or(' ');
        match tag {
            'D' => {
                if let Some(p) = cols.next() {
                    changes.deletions.push(p.to_owned());
                }
            }
            'R' | 'C' => {
                let old = cols.next();
                let new = cols.next();
                if let (Some(old), Some(new)) = (old, new) {
                    changes.deletions.push(old.to_owned());
                    changes.additions.push(new.to_owned());
                }
            }
            _ => {
                if let Some(p) = cols.next() {
                    changes.additions.push(p.to_owned());
                }
            }
        }
    }
    changes
}

/// Read + base64-encode each addition; assemble the `fileChanges` GraphQL input.
fn build_file_changes(changes: &Changes) -> Result<Value> {
    let mut additions = Vec::with_capacity(changes.additions.len());
    for path in &changes.additions {
        let bytes = std::fs::read(path).with_context(|| format!("failed to read {path}"))?;
        additions.push(json!({
            "path": path,
            "contents": base64::engine::general_purpose::STANDARD.encode(&bytes),
        }));
    }
    let deletions: Vec<Value> = changes
        .deletions
        .iter()
        .map(|p| json!({ "path": p }))
        .collect();
    Ok(json!({ "additions": additions, "deletions": deletions }))
}

/// The current tip OID of `branch` (the commit's parent).
fn branch_head_oid(
    http: &reqwest::blocking::Client,
    repo: &str,
    branch: &str,
    token: &str,
) -> Result<String> {
    let url = format!("{API}/repos/{repo}/git/ref/heads/{branch}");
    let resp = http
        .get(&url)
        .bearer_auth(token)
        .header("Accept", "application/vnd.github+json")
        .send()
        .context("failed to look up branch head")?;
    let status = resp.status();
    let body: Value = resp.json().context("branch head response was not JSON")?;
    if !status.is_success() {
        bail!("branch head lookup returned HTTP {status}: {body}");
    }
    body.get("object")
        .and_then(|o| o.get("sha"))
        .and_then(Value::as_str)
        .map(str::to_owned)
        .context("branch head response missing object.sha")
}

/// Run the `createCommitOnBranch` mutation; return the new commit's URL.
fn create_commit(
    http: &reqwest::blocking::Client,
    repo: &str,
    branch: &str,
    expected_head: &str,
    headline: &str,
    file_changes: Value,
    token: &str,
) -> Result<String> {
    let query = "mutation($i: CreateCommitOnBranchInput!) { \
                 createCommitOnBranch(input: $i) { commit { url } } }";
    let variables = json!({
        "i": {
            "branch": { "repositoryNameWithOwner": repo, "branchName": branch },
            "message": { "headline": headline },
            "expectedHeadOid": expected_head,
            "fileChanges": file_changes,
        }
    });
    let resp = http
        .post(format!("{API}/graphql"))
        .bearer_auth(token)
        .json(&json!({ "query": query, "variables": variables }))
        .send()
        .context("createCommitOnBranch request failed")?;
    let status = resp.status();
    let body: Value = resp
        .json()
        .context("createCommitOnBranch response not JSON")?;
    if !status.is_success() {
        bail!("createCommitOnBranch returned HTTP {status}: {body}");
    }
    if let Some(errors) = body.get("errors").filter(|e| !e.is_null()) {
        bail!("createCommitOnBranch failed: {errors}");
    }
    body.pointer("/data/createCommitOnBranch/commit/url")
        .and_then(Value::as_str)
        .map(str::to_owned)
        .with_context(|| format!("unexpected createCommitOnBranch response: {body}"))
}

/// Invoking `git` (a stable tool) is fine; the point of moving off the shell is to
/// keep the *logic* — payload assembly, the API calls — in typed, tested Rust.
fn git(args: &[&str]) -> Result<()> {
    let ok = Command::new("git")
        .args(args)
        .status()
        .context("failed to run git")?
        .success();
    if !ok {
        bail!("git {args:?} failed");
    }
    Ok(())
}

fn git_stdout(args: &[&str]) -> Result<String> {
    let out = Command::new("git")
        .args(args)
        .output()
        .context("failed to run git")?;
    if !out.status.success() {
        bail!(
            "git {args:?} failed: {}",
            String::from_utf8_lossy(&out.stderr)
        );
    }
    Ok(String::from_utf8_lossy(&out.stdout).into_owned())
}

fn env(key: &str) -> Result<String> {
    std::env::var(key).with_context(|| format!("{key} is not set (this runs only in CI)"))
}

/// Append `key=value` to `$GITHUB_OUTPUT` (a no-op off CI) for later steps.
fn set_output(key: &str, value: &str) -> Result<()> {
    use std::io::Write;
    let Some(path) = std::env::var_os("GITHUB_OUTPUT") else {
        return Ok(());
    };
    let mut f = std::fs::OpenOptions::new()
        .append(true)
        .create(true)
        .open(PathBuf::from(path))
        .context("failed to open GITHUB_OUTPUT")?;
    writeln!(f, "{key}={value}").context("failed to write GITHUB_OUTPUT")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_diff_classifies_statuses() {
        let diff =
            "A\tsources/a.pdf\nM\tout/b.md\nD\tout/old.md\nR100\tsources/x.pdf\tsources/y.pdf";
        let c = parse_diff(diff);
        assert_eq!(c.additions, ["sources/a.pdf", "out/b.md", "sources/y.pdf"]);
        assert_eq!(c.deletions, ["out/old.md", "sources/x.pdf"]);
        assert_eq!(c.len(), 5);
    }

    #[test]
    fn parse_diff_empty_is_empty() {
        assert!(parse_diff("").is_empty());
        assert!(parse_diff("\n").is_empty());
    }

    #[test]
    fn build_file_changes_shape_for_deletions_only() {
        let changes = Changes {
            additions: vec![],
            deletions: vec!["out/gone.md".into()],
        };
        let v = build_file_changes(&changes).unwrap();
        assert_eq!(v["additions"], json!([]));
        assert_eq!(v["deletions"], json!([{ "path": "out/gone.md" }]));
    }
}
