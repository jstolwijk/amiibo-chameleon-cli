use unicode_normalization::UnicodeNormalization;

pub fn normalize(input: &str) -> String {
    let mut normalized = String::new();
    let mut separator_pending = false;

    for character in input.nfc().flat_map(char::to_lowercase) {
        if character.is_alphanumeric() {
            if separator_pending && !normalized.is_empty() {
                normalized.push(' ');
            }
            normalized.push(character);
            separator_pending = false;
        } else {
            separator_pending = true;
        }
    }

    normalized
}

#[cfg(test)]
mod tests {
    use super::normalize;

    #[test]
    fn normalizes_case_whitespace_and_separators() {
        assert_eq!(normalize("  Super_Smash--Mario  "), "super smash mario");
    }

    #[test]
    fn retains_unicode_letters() {
        assert_eq!(normalize("Pokémon"), "pokémon");
    }

    #[test]
    fn composes_equivalent_unicode_names() {
        assert_eq!(normalize("Poke\u{301}mon"), normalize("Pokémon"));
    }
}
