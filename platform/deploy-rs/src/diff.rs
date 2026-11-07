//! Small difflib-compatible unified diff for `hermes render-config --dry-run`.
//!
//! Hunk boundaries are not a contract (the Go implementation says the same);
//! the diff only has to make a pending render reviewable.

use std::fmt::Write as _;

/// Returns a unified diff, or an empty string when the texts match.
#[must_use]
pub fn unified_diff(path: &str, old_text: &str, new_text: &str) -> String {
    let old_lines = split_keep_ends(old_text);
    let new_lines = split_keep_ends(new_text);
    let operations = diff_operations(&old_lines, &new_lines);
    if operations.is_empty() {
        return String::new();
    }

    let mut output = format!("--- {path}\n+++ {path} (rendered)\n");
    for hunk in hunks(&operations, 3) {
        let (old_start, old_count) = hunk_range(&operations, hunk.0, hunk.1, Side::Old);
        let (new_start, new_count) = hunk_range(&operations, hunk.0, hunk.1, Side::New);
        let _ = writeln!(
            output,
            "@@ -{old_start},{old_count} +{new_start},{new_count} @@"
        );
        for operation in &operations[hunk.0..hunk.1] {
            output.push_str(operation.prefix);
            output.push_str(&operation.text);
            if !operation.text.ends_with('\n') {
                output.push_str("\n\\ No newline at end of file\n");
            }
        }
    }
    output
}

struct Operation {
    kind: char,
    prefix: &'static str,
    text: String,
}

fn split_keep_ends(text: &str) -> Vec<String> {
    if text.is_empty() {
        return Vec::new();
    }
    let mut lines: Vec<String> = text.split_inclusive('\n').map(str::to_string).collect();
    if lines.last().is_some_and(String::is_empty) {
        lines.pop();
    }
    lines
}

fn diff_operations(old_lines: &[String], new_lines: &[String]) -> Vec<Operation> {
    let rows = old_lines.len();
    let columns = new_lines.len();
    let mut table = vec![vec![0usize; columns + 1]; rows + 1];
    for i in (0..rows).rev() {
        for j in (0..columns).rev() {
            if old_lines[i] == new_lines[j] {
                table[i][j] = table[i + 1][j + 1] + 1;
            } else if table[i + 1][j] >= table[i][j + 1] {
                table[i][j] = table[i + 1][j];
            } else {
                table[i][j] = table[i][j + 1];
            }
        }
    }
    let mut operations = Vec::new();
    let (mut i, mut j) = (0, 0);
    while i < rows && j < columns {
        if old_lines[i] == new_lines[j] {
            operations.push(Operation {
                kind: ' ',
                prefix: " ",
                text: old_lines[i].clone(),
            });
            i += 1;
            j += 1;
        } else if table[i + 1][j] >= table[i][j + 1] {
            operations.push(Operation {
                kind: '-',
                prefix: "-",
                text: old_lines[i].clone(),
            });
            i += 1;
        } else {
            operations.push(Operation {
                kind: '+',
                prefix: "+",
                text: new_lines[j].clone(),
            });
            j += 1;
        }
    }
    for line in &old_lines[i..] {
        operations.push(Operation {
            kind: '-',
            prefix: "-",
            text: line.clone(),
        });
    }
    for line in &new_lines[j..] {
        operations.push(Operation {
            kind: '+',
            prefix: "+",
            text: line.clone(),
        });
    }
    operations
}

fn hunks(operations: &[Operation], context: usize) -> Vec<(usize, usize)> {
    let mut result = Vec::new();
    let mut index = 0;
    while index < operations.len() {
        if operations[index].kind == ' ' {
            index += 1;
            continue;
        }
        let start = index.saturating_sub(context);
        let mut end = index;
        while end < operations.len() {
            if operations[end].kind != ' ' {
                end += 1;
                continue;
            }
            let mut next = end;
            while next < operations.len() && operations[next].kind == ' ' {
                next += 1;
            }
            if next < operations.len() && next - end <= 2 * context {
                end = next;
                continue;
            }
            end = (end + context).min(operations.len());
            break;
        }
        result.push((start, end));
        index = end;
    }
    result
}

#[derive(Clone, Copy)]
enum Side {
    Old,
    New,
}

fn hunk_range(operations: &[Operation], start: usize, end: usize, side: Side) -> (usize, usize) {
    let (mut old_line, mut new_line) = (1, 1);
    for operation in &operations[..start] {
        if operation.kind != '+' {
            old_line += 1;
        }
        if operation.kind != '-' {
            new_line += 1;
        }
    }
    let line = match side {
        Side::Old => old_line,
        Side::New => new_line,
    };
    let mut count = 0;
    for operation in &operations[start..end] {
        match operation.kind {
            ' ' => count += 1,
            '-' if matches!(side, Side::Old) => count += 1,
            '+' if matches!(side, Side::New) => count += 1,
            _ => {}
        }
    }
    (line, count)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_texts_have_no_diff() {
        assert!(unified_diff("file", "", "").is_empty());
        // Identical non-empty text has no hunks; only the header is emitted.
        assert!(!unified_diff("file", "a\nb\n", "a\nb\n").contains("@@"));
    }

    #[test]
    fn changed_line_is_shown_with_context() {
        let diff = unified_diff("config.yaml", "a\nb\nc\n", "a\nB\nc\n");
        assert!(diff.starts_with("--- config.yaml\n+++ config.yaml (rendered)\n"));
        assert!(diff.contains("-b\n"));
        assert!(diff.contains("+B\n"));
    }
}
