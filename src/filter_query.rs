pub struct ParsedQuery {
    pub include_hidden: bool,
    pub text: String,
}

pub fn parse_filter_query(raw: &str) -> ParsedQuery {
    let mut include_hidden = false;
    let mut text_tokens: Vec<&str> = Vec::new();

    for token in raw.split_whitespace() {
        match token.to_lowercase().as_str() {
            "is:h" | "is:hidden" => include_hidden = true,
            _ => text_tokens.push(token),
        }
    }

    ParsedQuery {
        include_hidden,
        text: text_tokens.join(" "),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plain_text() {
        let q = parse_filter_query("foo");
        assert!(!q.include_hidden);
        assert_eq!(q.text, "foo");
    }

    #[test]
    fn is_h_prefix() {
        let q = parse_filter_query("is:h foo");
        assert!(q.include_hidden);
        assert_eq!(q.text, "foo");
    }

    #[test]
    fn is_hidden_long_form() {
        let q = parse_filter_query("foo is:hidden bar");
        assert!(q.include_hidden);
        assert_eq!(q.text, "foo bar");
    }

    #[test]
    fn is_hidden_uppercase() {
        let q = parse_filter_query("is:HIDDEN");
        assert!(q.include_hidden);
        assert_eq!(q.text, "");
    }

    #[test]
    fn unknown_is_token_passes_through() {
        let q = parse_filter_query("is:other foo");
        assert!(!q.include_hidden);
        assert_eq!(q.text, "is:other foo");
    }

    #[test]
    fn empty_query() {
        let q = parse_filter_query("");
        assert!(!q.include_hidden);
        assert_eq!(q.text, "");
    }
}
