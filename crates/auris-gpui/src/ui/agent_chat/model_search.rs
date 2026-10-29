//! Ranked model-name completion, shared by keyboard candidates and command submission.

use super::ModelOption;

/// Qualify duplicate names so completing a row preserves its provider route.
pub(crate) fn command_name(index: usize, models: &[ModelOption]) -> String {
    let model = &models[index];
    if models
        .iter()
        .filter(|other| other.name == model.name)
        .count()
        > 1
        && let Some(connection) = &model.connection
    {
        format!("{}::{}", connection.label(), model.name)
    } else {
        model.name.clone()
    }
}

fn normalized(value: &str) -> String {
    value
        .chars()
        .filter(|c| c.is_alphanumeric())
        .flat_map(char::to_lowercase)
        .collect()
}

/// Small rolling edit-distance table; adjacent transpositions count as one typo.
fn distance(left: &str, right: &str) -> usize {
    let left: Vec<_> = left.chars().collect();
    let right: Vec<_> = right.chars().collect();
    let mut previous: Vec<_> = (0..=right.len()).collect();
    let mut older = previous.clone();
    for (i, a) in left.iter().enumerate() {
        let mut row = vec![i + 1; right.len() + 1];
        for (j, b) in right.iter().enumerate() {
            row[j + 1] = (row[j] + 1)
                .min(previous[j + 1] + 1)
                .min(previous[j] + usize::from(a != b));
            if i > 0 && j > 0 && *a == right[j - 1] && left[i - 1] == *b {
                row[j + 1] = row[j + 1].min(older[j - 1] + 1);
            }
        }
        older = previous;
        previous = row;
    }
    previous[right.len()]
}

fn score(query: &str, name: &str) -> Option<(usize, usize)> {
    let name = normalized(name);
    if name == query {
        return Some((1, 0));
    }
    if name.starts_with(query) {
        return Some((2, name.len()));
    }
    if name.contains(query) {
        return Some((3, name.len()));
    }
    // Bound both work and tolerance: unrelated text must never select an arbitrary model.
    if query.chars().count() >= 3 && query.len() <= 256 && name.len() <= 512 {
        let edits = distance(query, &name);
        if edits <= (query.chars().count() / 4).clamp(1, 3) {
            return Some((4, edits));
        }
    }
    let mut characters = name.chars();
    if query.chars().count() >= 3 && query.chars().all(|c| characters.by_ref().any(|n| n == c)) {
        return Some((5, name.len()));
    }
    None
}

pub(crate) fn matches(query: &str, models: &[ModelOption]) -> Vec<usize> {
    let query = query.trim();
    if query.is_empty() {
        return (0..models.len()).collect();
    }
    let normalized_query = normalized(query);
    if normalized_query.is_empty() {
        return Vec::new();
    }
    let mut matches: Vec<_> = models
        .iter()
        .enumerate()
        .filter_map(|(index, model)| {
            let command = command_name(index, models);
            let rank =
                if command.eq_ignore_ascii_case(query) || model.name.eq_ignore_ascii_case(query) {
                    Some((0, 0))
                } else {
                    let short = model.name.rsplit('/').next().unwrap_or(&model.name);
                    [
                        Some(command.as_str()),
                        Some(model.name.as_str()),
                        Some(short),
                        Some(short.split(':').next().unwrap_or(short)),
                        model.display_name.as_deref(),
                    ]
                    .into_iter()
                    .flatten()
                    .filter_map(|name| score(&normalized_query, name))
                    .min()
                }?;
            Some((rank, command, index))
        })
        .collect();
    matches.sort();
    matches.into_iter().map(|(_, _, index)| index).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn models() -> Vec<ModelOption> {
        [
            "lfm3:latest",
            "lfm3-audio:latest",
            "gemma4:26b",
            "org/lfm3-omni:latest",
        ]
        .into_iter()
        .map(|name| ModelOption {
            name: name.into(),
            ..Default::default()
        })
        .collect()
    }

    #[test]
    fn ranks_exact_partial_normalized_and_mistyped_names() {
        for query in [
            "lfm3-audio:latest",
            "LFM 3 Audio",
            "lfm3-auido",
            "lfm3-audi",
            "lfm3-aduiо",
        ] {
            assert_eq!(matches(query, &models()).first(), Some(&1), "{query}");
        }
        assert_eq!(matches("lfm3", &models()).first(), Some(&0));
        assert_eq!(matches("lfm3 omni", &models()).first(), Some(&3));
    }

    #[test]
    fn unrelated_and_punctuation_only_queries_have_no_match() {
        assert!(matches("nonexistent-model", &models()).is_empty());
        assert!(matches("???", &models()).is_empty());
        assert!(matches("モデル不存在", &models()).is_empty());
        assert_eq!(matches("", &models()), vec![0, 1, 2, 3]);
    }

    #[test]
    fn qualified_completion_keeps_duplicate_names_on_their_provider() {
        let models: Vec<_> = ["codex", "rig"]
            .into_iter()
            .map(|backend| ModelOption {
                name: "same-model".into(),
                connection: Some(auris_agent::ModelConnection {
                    backend: backend.into(),
                    provider: "ollama".into(),
                    ..Default::default()
                }),
                ..Default::default()
            })
            .collect();
        assert_eq!(
            matches(&command_name(1, &models), &models).first(),
            Some(&1)
        );
    }
}
