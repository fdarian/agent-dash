use std::cmp::Reverse;

/// Items whose text matches `query`, best match first; equal scores keep their input order.
pub fn fuzzy_rank<T>(items: impl IntoIterator<Item = (T, String)>, query: &str) -> Vec<T> {
    use nucleo_matcher::pattern::{CaseMatching, Normalization, Pattern};
    use nucleo_matcher::{Matcher, Utf32Str};

    let mut matcher = Matcher::new(nucleo_matcher::Config::DEFAULT);
    let pattern = Pattern::parse(query, CaseMatching::Smart, Normalization::Smart);
    let mut scored: Vec<(u32, T)> = items
        .into_iter()
        .filter_map(|(item, haystack)| {
            let mut buf = Vec::new();
            let score = pattern.score(Utf32Str::new(&haystack, &mut buf), &mut matcher)?;
            Some((score, item))
        })
        .collect();
    scored.sort_by_key(|(score, _)| Reverse(*score));
    scored.into_iter().map(|(_, item)| item).collect()
}
