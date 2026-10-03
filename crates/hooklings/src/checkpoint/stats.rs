//! Line-delta accounting for staged files, used to build checkpoint commit bodies.

/// How many bytes are sampled when deciding whether a blob is binary.
const BINARY_SNIFF_LEN: usize = 8000;

/// Inserted and removed line counts between two blob revisions.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct LineDelta {
    pub added: u64,
    pub removed: u64,
}

/// Whether a blob should be counted by line or reported as binary.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Delta {
    Lines(LineDelta),
    Binary,
}

/// Returns `true` when a NUL byte appears in git's binary sniff window.
pub fn looks_binary(bytes: &[u8]) -> bool {
    bytes.iter().take(BINARY_SNIFF_LEN).any(|byte| *byte == 0)
}

/// Counts lines in a blob, treating a trailing newline as a terminator rather than a
/// line of its own.
fn count_lines(bytes: &[u8]) -> u64 {
    if bytes.is_empty() {
        return 0;
    }
    let newlines = bytes.iter().filter(|byte| **byte == b'\n').count() as u64;
    match bytes.last() {
        Some(b'\n') => newlines,
        _ => newlines + 1,
    }
}

/// Computes the line delta between two revisions of a file.
///
/// `before` is `None` for a newly added file and `after` is `None` for a deletion.
/// Binary content reports [`Delta::Binary`] instead of a misleading line count.
pub fn delta(before: Option<&[u8]>, after: Option<&[u8]>) -> Delta {
    if before.is_some_and(looks_binary) || after.is_some_and(looks_binary) {
        return Delta::Binary;
    }

    match (before, after) {
        (None, None) => Delta::Lines(LineDelta::default()),
        (None, Some(after)) => Delta::Lines(LineDelta {
            added: count_lines(after),
            removed: 0,
        }),
        (Some(before), None) => Delta::Lines(LineDelta {
            added: 0,
            removed: count_lines(before),
        }),
        (Some(before), Some(after)) => {
            if before == after {
                return Delta::Lines(LineDelta::default());
            }
            let input = gix::diff::blob::InternedInput::new(
                gix::diff::blob::sources::byte_lines(before),
                gix::diff::blob::sources::byte_lines(after),
            );
            let diff =
                gix::diff::blob::Diff::compute(gix::diff::blob::Algorithm::Histogram, &input);
            Delta::Lines(LineDelta {
                added: u64::from(diff.count_additions()),
                removed: u64::from(diff.count_removals()),
            })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_before_and_after_is_zero() {
        assert_eq!(delta(None, None), Delta::Lines(LineDelta::default()));
    }

    #[test]
    fn added_file_counts_every_line() {
        let delta = delta(None, Some(b"one\ntwo\nthree\n"));
        assert_eq!(
            delta,
            Delta::Lines(LineDelta {
                added: 3,
                removed: 0
            })
        );
    }

    #[test]
    fn trailing_line_without_newline_still_counts() {
        let delta = delta(None, Some(b"one\ntwo"));
        assert_eq!(
            delta,
            Delta::Lines(LineDelta {
                added: 2,
                removed: 0
            })
        );
    }

    #[test]
    fn deleted_file_counts_every_line() {
        let delta = delta(Some(b"one\ntwo\n"), None);
        assert_eq!(
            delta,
            Delta::Lines(LineDelta {
                added: 0,
                removed: 2
            })
        );
    }

    #[test]
    fn modified_file_counts_insertions_and_removals() {
        let before = b"a\nb\nc\n";
        let after = b"a\nB\nc\nd\n";
        let delta = delta(Some(before), Some(after));
        assert_eq!(
            delta,
            Delta::Lines(LineDelta {
                added: 2,
                removed: 1
            })
        );
    }

    #[test]
    fn identical_content_is_zero_delta() {
        let bytes = b"same\ncontent\n";
        assert_eq!(
            delta(Some(bytes), Some(bytes)),
            Delta::Lines(LineDelta::default())
        );
    }

    #[test]
    fn nul_byte_marks_content_binary() {
        assert!(looks_binary(&[0x00]));
        assert!(!looks_binary(b"plain text"));
    }

    #[test]
    fn nul_beyond_sniff_window_does_not_mark_binary() {
        let mut bytes = vec![b'a'; BINARY_SNIFF_LEN + 10];
        bytes[BINARY_SNIFF_LEN + 5] = 0;
        assert!(!looks_binary(&bytes));
    }

    #[test]
    fn binary_content_reports_binary_not_line_counts() {
        assert_eq!(delta(None, Some(&[0x00, 0x01])), Delta::Binary);
        assert_eq!(delta(Some(&[0x00]), None), Delta::Binary);
    }
}
