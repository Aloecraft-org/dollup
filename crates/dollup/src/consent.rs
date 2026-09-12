//! `dollup consent`: review and accept a root's declared ceiling, without
//! starting anything.
//!
//! The same `consent::check` drt runs at start, with the same answers: a
//! first acceptance takes an interactive yes or `-y`; a ceiling that
//! narrowed since it was accepted is updated silently; a ceiling that
//! widened prints what changed and takes an interactive yes or
//! `--accept-changes` — and `-y` does not satisfy that, because otherwise
//! every systemd unit and CI job would carry permanent pre-consent to all
//! future widening. `--all` writes the blanket operator entry, which is the
//! explicit opt-out from all of it.
//!
//! No TTY and no applicable flag is a named failure: never a hang, never an
//! assumed yes. The ssh host-key analogy holds with its real lesson — people
//! type yes — so what is printed before the question is the whole ceiling,
//! or the whole delta, and never a summary.

use std::io::{BufRead, IsTerminal, Write};
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::{bail, Context, Result};
use drt_config::consent::{self, Accepted, ConsentCheck, ConsentJson};
use drt_config::project::{self, ProjectJson};
use drt_config::realm::Realm;
use drt_config::time::Timestamp;

use crate::deployment::write_json;
use crate::root;

/// What the command line allows without asking.
#[derive(Debug, Clone, Copy, Default)]
pub struct Flags {
    /// `-y`: accept a **first** acceptance. Nothing else.
    pub yes: bool,
    /// `--accept-changes`: accept a ceiling that widened since it was
    /// accepted.
    pub accept_changes: bool,
    /// `--all`: the blanket operator entry.
    pub all: bool,
}

/// Review, and accept where the flags or the terminal say so. Returns the
/// lines to print; a refusal is an error naming what was needed.
pub fn consent(dir: &Path, flags: Flags) -> Result<Vec<String>> {
    if !root::exists(dir) {
        bail!(
            "no root here — {} does not exist (discovery does not walk up; --root names one)",
            root::project_path(dir).display()
        );
    }
    let project_path = root::project_path(dir);
    let project: ProjectJson = serde_json::from_slice(&std::fs::read(&project_path)?)
        .with_context(|| format!("{} does not parse", project_path.display()))?;
    let consent_path = root::consent_path(dir);
    let mut consent_json = if consent_path.is_file() {
        serde_json::from_slice::<ConsentJson>(&std::fs::read(&consent_path)?)
            .with_context(|| format!("{} does not parse", consent_path.display()))?
    } else {
        ConsentJson::new(project.root_id)
    };
    let now =
        Timestamp::from_unix_secs(SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs() as i64);
    let mut lines = vec![];

    if flags.all {
        // The opt-out. It bypasses the delta, --accept-changes and the
        // widening protection for good; said before it is written.
        let ceiling_hash = project::ceiling_hash(&project)?;
        lines.push(
            "blanket operator consent: everything under `operator`, forever; nothing \
             about this root's ceiling will prompt again. Edit consent.json to return to \
             listed consent."
                .into(),
        );
        set_root_entry(
            &mut consent_json,
            Accepted::All {
                realm: Realm::root(),
                accepted_against: ceiling_hash,
                accepted_at: now,
            },
        );
        write_json(&consent_path, &consent_json)?;
        lines.push(format!("wrote {}", consent_path.display()));
        return Ok(lines);
    }

    // The same check start runs; a failure here is the failure start would name.
    let check = consent::check(&project, Some(&consent_json))?;
    let accepted = match check {
        ConsentCheck::Unchanged => {
            lines.push("consent: listed, matches the ceiling — nothing to accept".into());
            return Ok(lines);
        }
        ConsentCheck::Blanket {
            accepted_at,
            ceiling_changed,
        } => {
            lines.push(format!(
                "blanket operator consent is in force (accepted {accepted_at}){}; nothing \
                 prompts. Edit consent.json to return to listed consent.",
                if ceiling_changed {
                    ", against a ceiling that has since changed"
                } else {
                    ""
                }
            ));
            return Ok(lines);
        }
        ConsentCheck::First { .. } => {
            lines.push("the ceiling this root declares, not yet accepted:".into());
            lines.extend(project.caps.iter().map(|g| format!("  {}", describe(g))));
            if flags.yes {
                lines.push("accepted (-y)".into());
                true
            } else {
                ask(&mut lines, "Accept this ceiling?", "-y")?
            }
        }
        ConsentCheck::Narrowed { change, .. } => {
            // Removals only: what start does silently, done here and said.
            lines.push(
                "the ceiling narrowed since it was accepted (removals only); the entry \
                 is updated, as start would do silently:"
                    .into(),
            );
            lines.extend(change.lines().into_iter().map(|l| format!("  {l}")));
            true
        }
        ConsentCheck::Widened {
            objection, change, ..
        } => {
            lines.push(format!(
                "the ceiling WIDENED since it was accepted: {objection}"
            ));
            lines.extend(change.lines().into_iter().map(|l| format!("  {l}")));
            lines.push("the ceiling as it stands now:".into());
            lines.extend(project.caps.iter().map(|g| format!("  {}", describe(g))));
            if flags.accept_changes {
                lines.push("accepted (--accept-changes)".into());
                true
            } else if flags.yes {
                bail!(
                    "{}\n-y accepts a first acceptance only; this ceiling widened since it \
                     was accepted, and --accept-changes is what accepts that",
                    lines.join("\n")
                );
            } else {
                ask(
                    &mut lines,
                    "Accept the widened ceiling?",
                    "--accept-changes",
                )?
            }
        }
    };
    if !accepted {
        bail!(
            "{}\nnot accepted; consent.json is unchanged",
            lines.join("\n")
        );
    }
    set_root_entry(
        &mut consent_json,
        Accepted::Listed {
            realm: Realm::root(),
            ceiling_hash: project::ceiling_hash(&project)?,
            ceiling: project.caps.clone(),
            accepted_at: now,
        },
    );
    write_json(&consent_path, &consent_json)?;
    lines.push(format!("wrote {}", consent_path.display()));
    Ok(lines)
}

/// Ask on the terminal, or refuse by name when there is none. Everything
/// printed so far goes out first, so the question is never asked blind.
fn ask(lines: &mut Vec<String>, question: &str, flag: &str) -> Result<bool> {
    let stdin = std::io::stdin();
    if !stdin.is_terminal() {
        bail!(
            "{}\nno terminal to ask on, and no {flag}: not accepting. Run this on a \
             terminal, or pass {flag} to accept without one",
            lines.join("\n")
        );
    }
    for line in lines.drain(..) {
        println!("{line}");
    }
    print!("{question} [y/N] ");
    std::io::stdout().flush()?;
    let mut answer = String::new();
    stdin.lock().read_line(&mut answer)?;
    let answer = answer.trim();
    Ok(answer.eq_ignore_ascii_case("y") || answer.eq_ignore_ascii_case("yes"))
}

/// The entry at the root realm is the one that governs; replace it and
/// leave narrower entries and the signers as they are.
fn set_root_entry(consent_json: &mut ConsentJson, entry: Accepted) {
    consent_json
        .accepted
        .retain(|e| e.realm() != &Realm::root());
    consent_json.accepted.insert(0, entry);
}

/// `grant host:fs/*`, `deny host:fs/remove`, with the scope when one is
/// set — through the grant's own JSON, so what is shown is what is hashed.
fn describe<T: serde::Serialize>(grant: &T) -> String {
    let value = serde_json::to_value(grant).unwrap_or_default();
    let effect = value["effect"].as_str().unwrap_or("grant");
    let capability = value["capability"].as_str().unwrap_or("?");
    match value.get("scope") {
        Some(scope) if !scope.is_null() => format!("{effect} {capability} {scope}"),
        _ => format!("{effect} {capability}"),
    }
}
