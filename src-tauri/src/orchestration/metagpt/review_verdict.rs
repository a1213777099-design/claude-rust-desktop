/// Structured review verdict returned by the Reviewer.
#[derive(Debug, Clone)]
pub struct ReviewVerdict {
    pub approved: bool,
    pub quality_score: u8,
    pub issues: Vec<String>,
    pub suggestions: Vec<String>,
    pub feedback: String,
}

impl ReviewVerdict {
    pub fn from_text(text: &str) -> Self {
        let lower = text.to_lowercase();
        let has_score = (lower.contains("quality_score") || lower.contains("quality score"))
            && extract_score(&lower).map_or(false, |s| s >= 7);
        let approved = lower.contains("approved: true") 
            || lower.contains("approval: true")
            || lower.contains("**approved**")
            || has_score;

        let quality_score = extract_score(&lower).unwrap_or(if approved { 8 } else { 5 });
        let issues = extract_list_items(text, &["issues", "problems", "bugs", "defects"]);
        let suggestions = extract_list_items(text, &["suggestions", "improvements", "recommendations"]);

        Self { approved, quality_score, issues, suggestions, feedback: text.to_string() }
    }
}

fn extract_score(text: &str) -> Option<u8> {
    for pattern in &["quality_score:", "quality score:", "score:", "rating:"] {
        if let Some(idx) = text.find(pattern) {
            let after = &text[idx + pattern.len()..];
            let num_str: String = after.chars().skip_while(|c| !c.is_ascii_digit()).take_while(|c| c.is_ascii_digit()).collect();
            if let Ok(n) = num_str.trim().parse::<u8>() {
                if n <= 10 { return Some(n); }
            }
        }
    }
    None
}

fn extract_list_items(text: &str, headers: &[&str]) -> Vec<String> {
    let lines: Vec<&str> = text.lines().collect();
    let mut items = Vec::new();
    let mut in_section = false;

    for line in &lines {
        let lower = line.to_lowercase().trim().to_string();
        // Detect section header: line contains a header keyword
        if !in_section && headers.iter().any(|h| lower.contains(h)) {
            in_section = true;
            continue;
        }
        if in_section {
            let trimmed = line.trim();
            // Exit section on new header that doesn't match
            if trimmed.starts_with('#') && !headers.iter().any(|h| trimmed.to_lowercase().contains(h)) {
                in_section = false;
                continue;
            }
            if trimmed.starts_with("- ") || trimmed.starts_with("* ") || trimmed.starts_with("1.") || trimmed.starts_with("2.") {
                let cleaned: String = trimmed.chars()
                    .skip_while(|c| *c == '-' || *c == '*' || c.is_ascii_digit() || *c == '.' || *c == ' ')
                    .collect();
                if !cleaned.is_empty() { items.push(cleaned); }
            } else if !trimmed.is_empty() && !trimmed.starts_with('-') && !trimmed.starts_with('*') {
                // Might be a continuation or plain text issue
                // Only stop if we hit another section-like line
            }
        }
    }
    items
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_approved() {
        let text = "quality_score: 8\napproved: true\nThe code looks good.";
        let verdict = ReviewVerdict::from_text(text);
        assert!(verdict.approved);
        assert_eq!(verdict.quality_score, 8);
    }

    #[test]
    fn test_parse_rejected() {
        let text = "quality_score: 4\napproved: false\nIssues found:\n- Missing error handling";
        let verdict = ReviewVerdict::from_text(text);
        assert!(!verdict.approved);
        assert_eq!(verdict.quality_score, 4);
    }

    #[test]
    fn test_auto_approve_high_score() {
        let text = "quality score: 9/10\nVery well written code.";
        let verdict = ReviewVerdict::from_text(text);
        assert!(verdict.approved);
        assert_eq!(verdict.quality_score, 9);
    }

    #[test]
    fn test_extract_issues() {
        let text = "Issues:\n- Null pointer risk\n- Memory leak\nSuggestions:\n- Add bounds check";
        let verdict = ReviewVerdict::from_text(text);
        assert!(verdict.issues.len() >= 2, "Expected >=2 issues, got {}", verdict.issues.len());
    }
}
