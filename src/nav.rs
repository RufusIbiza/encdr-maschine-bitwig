//! Track-hierarchy navigation for the 4D encoder.
//!
//! Bitwig's main track bank is a flat, depth-first list of tracks where each
//! track carries its nesting `depth` (0 = top level). Everything here is
//! derived from that list alone, so it works for any nesting depth and never
//! relies on track names (which need not be unique).
//!
//! Directions:
//!   left / right  previous / next visible track
//!   up            the parent group (which then gets collapsed); on an open
//!                 top-level group it just collapses the group
//!   down          on a group: expand it and enter its first child

use crate::logic::TrackData;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct NavTargets {
    pub left: Option<usize>,
    pub right: Option<usize>,
    /// Track whose colour the up LED shows (the parent, or the group itself when it will just collapse).
    pub up: Option<usize>,
    /// Track whose colour the down LED shows (the first child, or the group itself while its children are not loaded).
    pub down: Option<usize>,
}

/// Visibility of every bank slot: a track is hidden when any ancestor group is collapsed.
pub fn visible(tracks: &[TrackData]) -> Vec<bool> {
    // Depth of the outermost collapsed group currently hiding tracks.
    let mut hide_below: Option<u32> = None;
    tracks
        .iter()
        .map(|t| {
            if !t.exists {
                return false;
            }
            if !t.activated {
                // Hidden itself, but a deactivated group still closes over its children.
                hide_below = Some(t.depth);
                return false;
            }
            if let Some(d) = hide_below {
                if t.depth > d {
                    return false;
                }
            }
            hide_below = if t.is_group && !t.is_group_expanded { Some(t.depth) } else { None };
            true
        })
        .collect()
}

/// Bank index of the parent group of `idx`, if it is inside the bank window.
pub fn parent_of(tracks: &[TrackData], idx: usize) -> Option<usize> {
    let depth = tracks.get(idx)?.depth;
    if depth == 0 {
        return None;
    }
    (0..idx).rev().find(|&j| tracks[j].exists && tracks[j].depth < depth).filter(|&j| tracks[j].depth == depth - 1)
}

/// Bank index of the first child of group `idx`, if it is loaded in the bank.
pub fn first_child(tracks: &[TrackData], idx: usize) -> Option<usize> {
    let t = tracks.get(idx)?;
    let c = tracks.get(idx + 1)?;
    (t.is_group && c.exists && c.depth == t.depth + 1).then_some(idx + 1)
}

/// The nearest visible track before (`dir < 0`) or after (`dir > 0`) `idx`.
pub fn step(tracks: &[TrackData], idx: usize, dir: i32) -> Option<usize> {
    let vis = visible(tracks);
    if dir < 0 {
        (0..idx).rev().find(|&j| vis[j])
    } else {
        (idx + 1..tracks.len()).find(|&j| vis[j])
    }
}

pub fn targets(tracks: &[TrackData], sel: usize) -> NavTargets {
    let Some(t) = tracks.get(sel).filter(|t| t.exists) else {
        return NavTargets::default();
    };
    let up = parent_of(tracks, sel).or((t.is_group && t.is_group_expanded).then_some(sel));
    let down = if t.is_group { first_child(tracks, sel).or(Some(sel)) } else { None };
    NavTargets { left: step(tracks, sel, -1), right: step(tracks, sel, 1), up, down }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// (depth, is_group, expanded)
    fn bank(spec: &[(u32, bool, bool)]) -> Vec<TrackData> {
        spec.iter()
            .enumerate()
            .map(|(i, &(depth, is_group, is_group_expanded))| TrackData {
                index: i,
                name: format!("T{i}"),
                exists: true,
                depth,
                is_group,
                is_group_expanded,
                ..Default::default()
            })
            .collect()
    }

    // 0 A
    // 1 Perc (open)
    // 2   Kick
    // 3   G2 (open)
    // 4     Hat
    // 5     G3 (closed)
    // 6       Ride
    // 7   Clap
    // 8 B
    fn sample() -> Vec<TrackData> {
        bank(&[
            (0, false, false),
            (0, true, true),
            (1, false, false),
            (1, true, true),
            (2, false, false),
            (2, true, false),
            (3, false, false),
            (1, false, false),
            (0, false, false),
        ])
    }

    #[test]
    fn collapsed_group_hides_descendants_only() {
        let v = visible(&sample());
        assert_eq!(v, vec![true, true, true, true, true, true, false, true, true]);
    }

    #[test]
    fn left_right_skip_hidden_tracks() {
        let t = sample();
        assert_eq!(step(&t, 5, 1), Some(7));
        assert_eq!(step(&t, 7, -1), Some(5));
        assert_eq!(step(&t, 0, -1), None);
        assert_eq!(step(&t, 8, 1), None);
    }

    #[test]
    fn parent_works_at_every_depth() {
        let t = sample();
        assert_eq!(parent_of(&t, 0), None);
        assert_eq!(parent_of(&t, 2), Some(1));
        assert_eq!(parent_of(&t, 4), Some(3));
        assert_eq!(parent_of(&t, 6), Some(5));
        // Clap comes after a deeper subtree but still belongs to Perc.
        assert_eq!(parent_of(&t, 7), Some(1));
    }

    #[test]
    fn parent_outside_bank_window_is_none() {
        // Bank scrolled into the middle of a group: first track is already depth 1.
        let t = bank(&[(1, false, false), (1, false, false), (0, false, false)]);
        assert_eq!(parent_of(&t, 1), None);
    }

    #[test]
    fn targets_for_group_and_leaf() {
        let t = sample();
        // Open nested group: up = parent Perc, down = its first child Hat.
        assert_eq!(targets(&t, 3), NavTargets { left: Some(2), right: Some(4), up: Some(1), down: Some(4) });
        // Collapsed group whose child is loaded: down enters it.
        assert_eq!(targets(&t, 5).down, Some(6));
        // Open top-level group: up collapses itself.
        assert_eq!(targets(&t, 1).up, Some(1));
        // Leaf at top level: no up/down.
        assert_eq!(targets(&t, 0), NavTargets { left: None, right: Some(1), up: None, down: None });
    }

    #[test]
    fn deactivated_tracks_are_skipped() {
        let mut t = sample();
        t[7].activated = false; // Clap
        assert_eq!(step(&t, 5, 1), Some(8));
        t[1].activated = false; // Perc: its whole subtree is skipped
        assert_eq!(step(&t, 0, 1), Some(8));
        assert_eq!(step(&t, 8, -1), Some(0));
    }

    #[test]
    fn collapsed_group_without_loaded_children_targets_itself() {
        let t = bank(&[(0, true, false), (0, false, false)]);
        assert_eq!(targets(&t, 0).down, Some(0));
    }
}
