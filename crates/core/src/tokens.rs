/// Rough token count for the context meter before a provider reports usage.
pub fn estimate_tokens(text: &str) -> u32 {
    let chars = text.chars().count() as u32;
    if chars == 0 {
        0
    } else {
        chars.div_ceil(4)
    }
}

pub fn format_context(used: u32, limit: u32) -> String {
    format!("{}/{}", format_tokens(used), format_tokens(limit))
}

pub fn format_tokens(n: u32) -> String {
    if n < 1_000 {
        return n.to_string();
    }
    let tenths = ((n as f64) / 100.0).round() as u32;
    let whole = tenths / 10;
    let frac = tenths % 10;
    if frac == 0 {
        format!("{whole}K")
    } else {
        format!("{whole}.{frac}K")
    }
}

pub fn estimate_thread_tokens(parts: &[String]) -> u32 {
    parts.iter().map(|part| estimate_tokens(part)).sum()
}

pub fn title_from_message(content: &str) -> String {
    let one_line = content.split_whitespace().collect::<Vec<_>>().join(" ");
    let mut chars = one_line.chars();
    let clipped: String = chars.by_ref().take(48).collect();
    if chars.next().is_some() {
        format!("{clipped}…")
    } else if clipped.is_empty() {
        "New chat".into()
    } else {
        clipped
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn context_meter_matches_the_spec_example() {
        assert_eq!(format_context(118_800, 256_000), "118.8K/256K");
        assert_eq!(format_tokens(900), "900");
        assert_eq!(format_tokens(8_192), "8.2K");
    }

    #[test]
    fn titles_stay_short() {
        assert_eq!(title_from_message("  hello   world "), "hello world");
        let long = "a".repeat(60);
        assert!(title_from_message(&long).ends_with('…'));
        assert_eq!(title_from_message("   "), "New chat");
    }
}
