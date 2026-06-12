use std::collections::HashSet;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::fingerprint::Fingerprint;
use crate::grouper::DuplicateGroup;

const IGNORE_FILE_NAME: &str = ".dupes-ignore.toml";

/// An entry in the ignore file.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct IgnoreEntry {
    /// The fingerprint of the duplicated code.
    pub fingerprint: String,
    /// Optional reason for ignoring.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
    /// Names of the code units in the group (for documentation).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub members: Vec<String>,
    /// Content fingerprints of the group's member units when recorded.
    ///
    /// Group fingerprints change when a group's membership drifts (a new
    /// near member joins) even though the registered duplicate relationship
    /// persists. An entry whose recorded member fingerprints all still
    /// appear together in one group keeps matching that group, so it
    /// survives membership drift; it stops matching only when the recorded
    /// content itself changes.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub member_fingerprints: Vec<String>,
}

impl IgnoreEntry {
    /// Parse the entry's recorded member fingerprints, skipping invalid hex.
    fn recorded_member_fingerprints(&self) -> Vec<Fingerprint> {
        self.member_fingerprints
            .iter()
            .filter_map(|hex| Fingerprint::from_hex(hex))
            .collect()
    }

    /// True when every recorded member fingerprint appears in `member_set`.
    fn members_all_in(&self, member_set: &HashSet<Fingerprint>) -> bool {
        let recorded = self.recorded_member_fingerprints();
        !recorded.is_empty() && recorded.iter().all(|fp| member_set.contains(fp))
    }
}

/// The ignore file structure.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct IgnoreFile {
    #[serde(default)]
    pub ignore: Vec<IgnoreEntry>,
}

/// Get the path to the ignore file for a project root.
#[must_use]
pub fn ignore_file_path(root: &Path) -> PathBuf {
    root.join(IGNORE_FILE_NAME)
}

/// Load the ignore file from disk.
#[must_use]
pub fn load_ignore_file(root: &Path) -> IgnoreFile {
    let path = ignore_file_path(root);
    if !path.exists() {
        return IgnoreFile::default();
    }
    std::fs::read_to_string(&path).map_or_else(
        |_| IgnoreFile::default(),
        |content| toml::from_str(&content).unwrap_or_default(),
    )
}

/// Save the ignore file to disk.
pub fn save_ignore_file(root: &Path, ignore_file: &IgnoreFile) -> std::io::Result<()> {
    let path = ignore_file_path(root);
    let content = toml::to_string_pretty(ignore_file)
        .map_err(|e| std::io::Error::other(format!("Failed to serialize ignore file: {e}")))?;
    std::fs::write(path, content)
}

/// Add an ignore entry for a fingerprint.
pub fn add_ignore(
    ignore_file: &mut IgnoreFile,
    fingerprint: &Fingerprint,
    reason: Option<String>,
    members: Vec<String>,
) {
    add_ignore_with_member_fingerprints(ignore_file, fingerprint, reason, members, Vec::new());
}

/// Add an ignore entry recording the group's member content fingerprints.
pub fn add_ignore_with_member_fingerprints(
    ignore_file: &mut IgnoreFile,
    fingerprint: &Fingerprint,
    reason: Option<String>,
    members: Vec<String>,
    member_fingerprints: Vec<String>,
) {
    let fp_hex = fingerprint.to_hex();
    // Don't add duplicates
    if ignore_file.ignore.iter().any(|e| e.fingerprint == fp_hex) {
        return;
    }
    ignore_file.ignore.push(IgnoreEntry {
        fingerprint: fp_hex,
        reason,
        members,
        member_fingerprints,
    });
}

/// Remove an ignore entry by fingerprint.
pub fn remove_ignore(ignore_file: &mut IgnoreFile, fingerprint: &str) -> bool {
    let initial_len = ignore_file.ignore.len();
    ignore_file.ignore.retain(|e| e.fingerprint != fingerprint);
    ignore_file.ignore.len() < initial_len
}

/// Check if a fingerprint is ignored.
#[must_use]
pub fn is_ignored(ignore_file: &IgnoreFile, fingerprint: &Fingerprint) -> bool {
    let fp_hex = fingerprint.to_hex();
    ignore_file.ignore.iter().any(|e| e.fingerprint == fp_hex)
}

/// Filter out ignored groups from a list of duplicate groups.
///
/// A group is ignored when its group fingerprint matches an entry, or when
/// an entry's recorded member fingerprints all still appear in the group
/// (the registered relationship persists despite membership drift).
#[must_use]
pub fn filter_ignored(
    groups: Vec<DuplicateGroup>,
    ignore_file: &IgnoreFile,
) -> Vec<DuplicateGroup> {
    groups
        .into_iter()
        .filter(|group| !group_is_ignored(ignore_file, group))
        .collect()
}

fn group_is_ignored(ignore_file: &IgnoreFile, group: &DuplicateGroup) -> bool {
    if is_ignored(ignore_file, &group.fingerprint) {
        return true;
    }
    let member_set: HashSet<Fingerprint> = group
        .members
        .iter()
        .map(|member| member.fingerprint)
        .collect();
    ignore_file
        .ignore
        .iter()
        .any(|entry| entry.members_all_in(&member_set))
}

/// Return whether an entry still matches the current analysis.
///
/// Live means the entry's group fingerprint is among the live group
/// fingerprints, or its recorded member fingerprints all appear together in
/// one live group's member set.
fn entry_is_live(
    entry: &IgnoreEntry,
    live_fingerprints: &HashSet<Fingerprint>,
    live_member_sets: &[HashSet<Fingerprint>],
) -> bool {
    // Invalid hex never matches: such entries are always stale.
    if Fingerprint::from_hex(&entry.fingerprint).is_some_and(|fp| live_fingerprints.contains(&fp)) {
        return true;
    }
    live_member_sets.iter().any(|set| entry.members_all_in(set))
}

/// Find ignore entries that no longer match any live group.
#[must_use]
pub fn find_stale_entries<'a>(
    ignore_file: &'a IgnoreFile,
    live_fingerprints: &HashSet<Fingerprint>,
    live_member_sets: &[HashSet<Fingerprint>],
) -> Vec<&'a IgnoreEntry> {
    ignore_file
        .ignore
        .iter()
        .filter(|entry| !entry_is_live(entry, live_fingerprints, live_member_sets))
        .collect()
}

/// Remove and return stale ignore entries.
pub fn remove_stale_entries(
    ignore_file: &mut IgnoreFile,
    live_fingerprints: &HashSet<Fingerprint>,
    live_member_sets: &[HashSet<Fingerprint>],
) -> Vec<IgnoreEntry> {
    let mut stale = Vec::new();
    let mut live = Vec::new();
    for entry in ignore_file.ignore.drain(..) {
        if entry_is_live(&entry, live_fingerprints, live_member_sets) {
            live.push(entry);
        } else {
            stale.push(entry);
        }
    }
    ignore_file.ignore = live;
    stale
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::code_unit::DetectionDimension;
    use crate::grouper::MatchKind;
    use crate::node::{LiteralKind, NodeKind, NormalizedNode};
    use tempfile::TempDir;

    fn test_fingerprint() -> Fingerprint {
        Fingerprint::from_node(&NormalizedNode::leaf(NodeKind::Literal(LiteralKind::Int)))
    }

    #[test]
    fn load_nonexistent_returns_default() {
        let tmp = TempDir::new().unwrap();
        let ignore = load_ignore_file(tmp.path());
        assert!(ignore.ignore.is_empty());
    }

    #[test]
    fn roundtrip_save_and_load() {
        let tmp = TempDir::new().unwrap();
        let fp = test_fingerprint();
        let mut ignore = IgnoreFile::default();
        add_ignore(
            &mut ignore,
            &fp,
            Some("test reason".to_string()),
            vec!["foo".to_string(), "bar".to_string()],
        );
        save_ignore_file(tmp.path(), &ignore).unwrap();
        let loaded = load_ignore_file(tmp.path());
        assert_eq!(loaded.ignore.len(), 1);
        assert_eq!(loaded.ignore[0].fingerprint, fp.to_hex());
        assert_eq!(loaded.ignore[0].reason, Some("test reason".to_string()));
        assert_eq!(loaded.ignore[0].members, vec!["foo", "bar"]);
    }

    #[test]
    fn add_ignore_deduplicates() {
        let fp = test_fingerprint();
        let mut ignore = IgnoreFile::default();
        add_ignore(&mut ignore, &fp, None, vec![]);
        add_ignore(&mut ignore, &fp, None, vec![]);
        assert_eq!(ignore.ignore.len(), 1);
    }

    #[test]
    fn remove_ignore_works() {
        let fp = test_fingerprint();
        let mut ignore = IgnoreFile::default();
        add_ignore(&mut ignore, &fp, None, vec![]);
        assert!(remove_ignore(&mut ignore, &fp.to_hex()));
        assert!(ignore.ignore.is_empty());
    }

    #[test]
    fn remove_nonexistent_returns_false() {
        let mut ignore = IgnoreFile::default();
        assert!(!remove_ignore(&mut ignore, "nonexistent"));
    }

    #[test]
    fn is_ignored_works() {
        let fp = test_fingerprint();
        let mut ignore = IgnoreFile::default();
        assert!(!is_ignored(&ignore, &fp));
        add_ignore(&mut ignore, &fp, None, vec![]);
        assert!(is_ignored(&ignore, &fp));
    }

    #[test]
    fn filter_ignored_removes_matching_groups() {
        let fp = test_fingerprint();
        let mut ignore = IgnoreFile::default();
        add_ignore(&mut ignore, &fp, None, vec![]);

        let matching = DuplicateGroup {
            suppressed: None,
            also_seen: Vec::new(),
            dimension: DetectionDimension::Ast,
            match_kind: MatchKind::Exact,
            fingerprint: fp,
            members: vec![],
            similarity: 1.0,
        };
        let mut unmatched = matching.clone();
        unmatched.fingerprint = Fingerprint::from_node(&NormalizedNode::leaf(NodeKind::Opaque));

        let filtered = filter_ignored(vec![matching, unmatched], &ignore);
        assert_eq!(filtered.len(), 1);
    }

    // jscpd:ignore-start

    #[test]
    fn filter_ignored_removes_near_duplicates_with_matching_fingerprint() {
        let fp = test_fingerprint();
        let mut ignore = IgnoreFile::default();
        add_ignore(&mut ignore, &fp, None, vec![]);

        let groups = vec![DuplicateGroup {
            suppressed: None,
            also_seen: Vec::new(),
            dimension: DetectionDimension::Ast,
            match_kind: MatchKind::Near,
            fingerprint: fp,
            members: vec![],
            similarity: 0.85,
        }];

        let filtered = filter_ignored(groups, &ignore);
        assert!(filtered.is_empty());
    }

    #[test]
    fn filter_ignored_keeps_near_duplicates_without_matching_entry() {
        let fp = test_fingerprint();
        let other_fp =
            Fingerprint::from_node(&NormalizedNode::with_children(NodeKind::Block, vec![]));
        let mut ignore = IgnoreFile::default();
        add_ignore(&mut ignore, &other_fp, None, vec![]);

        let groups = vec![DuplicateGroup {
            suppressed: None,
            also_seen: Vec::new(),
            dimension: DetectionDimension::Ast,
            match_kind: MatchKind::Near,
            fingerprint: fp,
            members: vec![],
            similarity: 0.85,
        }];

        let filtered = filter_ignored(groups, &ignore);
        assert_eq!(filtered.len(), 1);
    }

    #[test]
    fn find_stale_entries_identifies_stale_vs_live() {
        let live_fp = test_fingerprint();
        let stale_fp =
            Fingerprint::from_node(&NormalizedNode::with_children(NodeKind::Block, vec![]));

        let mut ignore = IgnoreFile::default();
        add_ignore(&mut ignore, &live_fp, Some("live".to_string()), vec![]);
        add_ignore(&mut ignore, &stale_fp, Some("stale".to_string()), vec![]);

        let mut live_set = std::collections::HashSet::new();
        live_set.insert(live_fp);

        let stale = find_stale_entries(&ignore, &live_set, &[]);
        assert_eq!(stale.len(), 1);
        assert_eq!(stale[0].reason, Some("stale".to_string()));
    }

    #[test]
    fn remove_stale_entries_removes_only_stale() {
        let live_fp = test_fingerprint();
        let stale_fp =
            Fingerprint::from_node(&NormalizedNode::with_children(NodeKind::Block, vec![]));

        let mut ignore = IgnoreFile::default();
        add_ignore(&mut ignore, &live_fp, Some("live".to_string()), vec![]);
        add_ignore(&mut ignore, &stale_fp, Some("stale".to_string()), vec![]);

        let mut live_set = std::collections::HashSet::new();
        live_set.insert(live_fp);

        let removed = remove_stale_entries(&mut ignore, &live_set, &[]);
        assert_eq!(removed.len(), 1);
        assert_eq!(removed[0].reason, Some("stale".to_string()));
        assert_eq!(ignore.ignore.len(), 1);
        assert_eq!(ignore.ignore[0].reason, Some("live".to_string()));
    }

    // jscpd:ignore-end

    #[test]
    fn ignore_file_path_is_correct() {
        let path = ignore_file_path(Path::new("/project"));
        assert_eq!(path, PathBuf::from("/project/.dupes-ignore.toml"));
    }

    fn member_unit(seed: &str) -> crate::code_unit::CodeUnit {
        crate::text_units::window_unit(
            Path::new("member.rs"),
            "member",
            crate::code_unit::CodeUnitKind::LineWindow,
            1,
            2,
            &[seed.to_string()],
        )
    }

    fn entry_with_member_fingerprints(seeds: &[&str]) -> IgnoreEntry {
        IgnoreEntry {
            fingerprint: test_fingerprint().to_hex(),
            reason: None,
            members: Vec::new(),
            member_fingerprints: seeds
                .iter()
                .map(|seed| member_unit(seed).fingerprint.to_hex())
                .collect(),
        }
    }

    #[test]
    fn member_subset_matching_survives_membership_drift() {
        // The group gained a member, so its composite fingerprint no longer
        // matches the entry; the recorded members still appear together, so
        // the entry keeps suppressing the group and stays live.
        let entry = entry_with_member_fingerprints(&["alpha", "bravo"]);
        let mut ignore = IgnoreFile::default();
        ignore.ignore.push(entry);

        let drifted_group = DuplicateGroup {
            suppressed: None,
            also_seen: Vec::new(),
            dimension: DetectionDimension::Ast,
            match_kind: MatchKind::Near,
            fingerprint: Fingerprint::from_bytes(b"drifted composite"),
            members: vec![
                member_unit("alpha"),
                member_unit("bravo"),
                member_unit("charlie"),
            ],
            similarity: 0.92,
        };
        let member_set: HashSet<Fingerprint> = drifted_group
            .members
            .iter()
            .map(|member| member.fingerprint)
            .collect();

        assert!(filter_ignored(vec![drifted_group], &ignore).is_empty());
        assert!(find_stale_entries(&ignore, &HashSet::new(), &[member_set]).is_empty());
    }

    #[test]
    fn member_subset_matching_resurfaces_edited_members() {
        // One recorded member's content changed, so the registered
        // relationship must come back for review: the group is reported and
        // the entry is stale.
        let entry = entry_with_member_fingerprints(&["alpha", "bravo"]);
        let mut ignore = IgnoreFile::default();
        ignore.ignore.push(entry);

        let edited_group = DuplicateGroup {
            suppressed: None,
            also_seen: Vec::new(),
            dimension: DetectionDimension::Ast,
            match_kind: MatchKind::Near,
            fingerprint: Fingerprint::from_bytes(b"edited composite"),
            members: vec![member_unit("alpha"), member_unit("bravo edited")],
            similarity: 0.91,
        };
        let member_set: HashSet<Fingerprint> = edited_group
            .members
            .iter()
            .map(|member| member.fingerprint)
            .collect();

        assert_eq!(filter_ignored(vec![edited_group], &ignore).len(), 1);
        assert_eq!(
            find_stale_entries(&ignore, &HashSet::new(), &[member_set]).len(),
            1
        );
    }

    #[test]
    fn member_fingerprints_roundtrip_through_the_ignore_file() {
        let tmp = TempDir::new().unwrap();
        let fp = test_fingerprint();
        let mut ignore = IgnoreFile::default();
        add_ignore_with_member_fingerprints(
            &mut ignore,
            &fp,
            Some("near family".to_string()),
            vec!["first".to_string(), "second".to_string()],
            vec!["aaaa".to_string(), "bbbb".to_string()],
        );
        save_ignore_file(tmp.path(), &ignore).unwrap();
        let loaded = load_ignore_file(tmp.path());
        assert_eq!(loaded.ignore[0].member_fingerprints, vec!["aaaa", "bbbb"]);
    }
}
