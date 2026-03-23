use crate::types::ProtectedRegion;

/// Find all protected regions in the text that should not be compressed.
///
/// Protected regions include:
/// - Triple-backtick fenced code blocks (``` ... ```)
/// - Inline code spans (`...`)
/// - JSON-like blocks ({...} with proper nesting)
pub fn find_protected_regions(text: &str) -> Vec<ProtectedRegion> {
    let bytes = text.as_bytes();
    let len = bytes.len();
    let mut regions = Vec::new();
    let mut i = 0;

    while i < len {
        // --- Triple-backtick fenced code blocks ---
        if i + 2 < len && bytes[i] == b'`' && bytes[i + 1] == b'`' && bytes[i + 2] == b'`' {
            let start = i;
            // Skip the opening ``` and any info string until newline
            i += 3;
            while i < len && bytes[i] != b'\n' {
                i += 1;
            }
            // Now scan for closing ```
            let mut found_close = false;
            while i < len {
                if i + 2 < len && bytes[i] == b'`' && bytes[i + 1] == b'`' && bytes[i + 2] == b'`' {
                    i += 3;
                    found_close = true;
                    break;
                }
                i += 1;
            }
            if !found_close {
                // Unclosed block: protect to end of text
                i = len;
            }
            regions.push(ProtectedRegion { start, end: i });
            continue;
        }

        // --- Inline code spans ---
        if bytes[i] == b'`' {
            let start = i;
            i += 1;
            let mut found_close = false;
            while i < len {
                if bytes[i] == b'`' {
                    i += 1;
                    found_close = true;
                    break;
                }
                // Inline code does not span multiple lines in most Markdown
                // but we'll allow it for robustness; just stop at the closing tick.
                i += 1;
            }
            if !found_close {
                i = len;
            }
            regions.push(ProtectedRegion { start, end: i });
            continue;
        }

        // --- JSON-like blocks ---
        if bytes[i] == b'{' {
            let start = i;
            let mut depth: i32 = 0;
            let mut j = i;
            let mut found_close = false;
            while j < len {
                match bytes[j] {
                    b'{' => depth += 1,
                    b'}' => {
                        depth -= 1;
                        if depth == 0 {
                            j += 1;
                            found_close = true;
                            break;
                        }
                    }
                    b'"' => {
                        // Skip strings inside JSON to avoid counting braces in strings
                        j += 1;
                        while j < len {
                            if bytes[j] == b'\\' {
                                j += 2; // skip escaped char
                                continue;
                            }
                            if bytes[j] == b'"' {
                                break;
                            }
                            j += 1;
                        }
                    }
                    _ => {}
                }
                j += 1;
            }
            if found_close {
                regions.push(ProtectedRegion { start, end: j });
                i = j;
            } else {
                // Not a well-formed JSON block; skip the opening brace
                i += 1;
            }
            continue;
        }

        i += 1;
    }

    regions
}

/// Check if a given byte position falls within any protected region.
pub fn is_in_protected_region(pos: usize, regions: &[ProtectedRegion]) -> bool {
    regions.iter().any(|r| pos >= r.start && pos < r.end)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_fenced_code_block() {
        let text = "before\n```rust\nfn main() {}\n```\nafter";
        let regions = find_protected_regions(text);
        assert_eq!(regions.len(), 1);
        assert!(is_in_protected_region(10, &regions)); // inside code block
        assert!(!is_in_protected_region(0, &regions)); // "before"
    }

    #[test]
    fn test_inline_code() {
        let text = "use `cargo build` to compile";
        let regions = find_protected_regions(text);
        assert_eq!(regions.len(), 1);
        let r = &regions[0];
        assert_eq!(&text[r.start..r.end], "`cargo build`");
    }

    #[test]
    fn test_json_block() {
        let text = r#"config: {"key": "value", "nested": {"a": 1}} done"#;
        let regions = find_protected_regions(text);
        assert_eq!(regions.len(), 1);
        assert!(regions[0].start < regions[0].end);
    }

    #[test]
    fn test_no_protected_regions() {
        let text = "just plain text without any special blocks";
        let regions = find_protected_regions(text);
        assert!(regions.is_empty());
    }

    #[test]
    fn test_is_in_protected_region_boundary() {
        let regions = vec![ProtectedRegion { start: 5, end: 10 }];
        assert!(!is_in_protected_region(4, &regions));
        assert!(is_in_protected_region(5, &regions));
        assert!(is_in_protected_region(9, &regions));
        assert!(!is_in_protected_region(10, &regions));
    }
}
