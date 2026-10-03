# Improvements

Catalogue of code-quality improvements, refactors, and architectural
changes proposed during development. Per the project workflow,
improvements are **logged here when noticed, not silently applied**
(see Maintenance Rule 8). The author decides whether to apply, defer,
or decline.

This is the dual of BUGS.md: bugs are things that are broken,
improvements are things that work but could be better.

Status vocabulary: suggested | applied | declined | deferred.
Effort vocabulary: trivial | small | medium | large.

## Suggested

### IMP-001: Rewrite the reconnaissance probe on top of `iris-proto`
**Status:** suggested
**Found:** 2026-09-20 (documentation session)
**Location:** [tools/phantom_probe.py](tools/phantom_probe.py)
**Effort:** small
**Description.** The probe builds packets by hand in Python, outside the allow-list that D-005 puts in the Rust protocol crate. BUG-001 came from exactly this kind of unguarded script.
**Proposal.** Once `iris-proto` exists, replace the script with an `irisctl probe` subcommand (or a small Rust example binary) that can only construct allow-listed packets. Keep the Python file only as a historical artefact, or delete it.
**Trade-offs.** Loses the zero-dependency, run-anywhere property that made the Python probe useful on day one; anyone wanting to probe a different board would need the Rust toolchain. Also couples a diagnostic tool to the code it may be needed to diagnose.
**Notes.** Related: BUG-001, AV-001, D-005.

### IMP-002: Automate `cargo-sources.json` regeneration
**Status:** suggested
**Found:** 2026-09-20 (documentation session)
**Location:** flatpak/ (not yet created)
**Effort:** trivial
**Description.** The Flatpak builds offline from a generated sources file that silently goes stale whenever `Cargo.lock` changes, producing a confusing build failure.
**Proposal.** A `tools/update-flatpak-sources.sh` wrapper plus a CI or pre-commit check that fails when `Cargo.lock` is newer than `flatpak/cargo-sources.json`.
**Trade-offs.** Vendors a copy of, or a network fetch of, `flatpak-cargo-generator.py`; adds a Python dependency to the commit path; a timestamp check gives false positives after a fresh clone, so it would need to compare content hashes instead.
**Notes.** Related: F-024, BUILD.md §Dependencies.

### IMP-003: Cross-reference checker
**Status:** suggested
**Found:** 2026-09-20 (documentation session)
**Location:** cross-cutting (documentation)
**Effort:** small
**Description.** The documentation standard's Maintenance Rule 6 asks for bidirectional F-/D-/AV-/BUG-/IMP- references. They were aligned by hand when these documents were written and will drift.
**Proposal.** A script that walks the markdown files, builds the ID graph and reports dangling or one-directional links; run it before commits.
**Trade-offs.** Another tool to maintain; strict bidirectionality creates noise for weak relationships, tempting people to omit useful one-way pointers. Could instead be shared across all of the author's repositories rather than living here.
**Notes.** Until it exists, CLAUDE.md states that cross-references may be stale.

## Applied

None.

## Declined

None.

## Deferred

None.
