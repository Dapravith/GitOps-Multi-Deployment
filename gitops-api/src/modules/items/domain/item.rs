use std::fmt;

use uuid::Uuid;

use super::error::ValidationError;

pub const NAME_MAX_CHARS: usize = 100;
pub const DESCRIPTION_MAX_CHARS: usize = 1000;

/// Server-generated identifier (UUID v4). Clients can never choose one.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ItemId(Uuid);

impl ItemId {
    #[must_use]
    pub fn generate() -> Self {
        Self(Uuid::new_v4())
    }

    /// # Errors
    /// Returns [`ValidationError`] unless `raw` is a UUID.
    pub fn parse(raw: &str) -> Result<Self, ValidationError> {
        Uuid::try_parse(raw)
            .map(Self)
            .map_err(|_| ValidationError::new("id", "must be a UUID"))
    }
}

impl fmt::Display for ItemId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.hyphenated().fmt(f)
    }
}

/// Trimmed, 1..=100 characters, no control characters.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ItemName(String);

impl ItemName {
    /// # Errors
    /// Returns [`ValidationError`] if the trimmed name is empty, too long, or
    /// contains control characters.
    pub fn parse(raw: &str) -> Result<Self, ValidationError> {
        let name = raw.trim();
        if name.is_empty() {
            return Err(ValidationError::new("name", "must not be empty"));
        }
        if name.chars().count() > NAME_MAX_CHARS {
            return Err(ValidationError::new(
                "name",
                "must be at most 100 characters",
            ));
        }
        if name.chars().any(char::is_control) {
            return Err(ValidationError::new(
                "name",
                "must not contain control characters",
            ));
        }
        Ok(Self(name.to_owned()))
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Uniqueness key: names are unique case-insensitively.
    #[must_use]
    pub fn unique_key(&self) -> String {
        self.0.to_lowercase()
    }
}

/// 0..=1000 characters; newlines and tabs allowed, other control characters not.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Description(String);

impl Description {
    /// # Errors
    /// Returns [`ValidationError`] if too long or containing control
    /// characters other than `\n`, `\r`, `\t`.
    pub fn parse(raw: &str) -> Result<Self, ValidationError> {
        if raw.chars().count() > DESCRIPTION_MAX_CHARS {
            return Err(ValidationError::new(
                "description",
                "must be at most 1000 characters",
            ));
        }
        if raw
            .chars()
            .any(|c| c.is_control() && !matches!(c, '\n' | '\r' | '\t'))
        {
            return Err(ValidationError::new(
                "description",
                "must not contain control characters other than newline and tab",
            ));
        }
        Ok(Self(raw.to_owned()))
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Item {
    id: ItemId,
    name: ItemName,
    description: Description,
}

impl Item {
    /// A new item with a freshly generated id.
    #[must_use]
    pub fn new(name: ItemName, description: Description) -> Self {
        Self::with_id(ItemId::generate(), name, description)
    }

    #[must_use]
    pub const fn with_id(id: ItemId, name: ItemName, description: Description) -> Self {
        Self {
            id,
            name,
            description,
        }
    }

    #[must_use]
    pub const fn id(&self) -> ItemId {
        self.id
    }

    #[must_use]
    pub const fn name(&self) -> &ItemName {
        &self.name
    }

    #[must_use]
    pub const fn description(&self) -> &Description {
        &self.description
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn name_is_trimmed_and_bounded_in_characters_not_bytes() {
        assert_eq!(ItemName::parse("  widget ").unwrap().as_str(), "widget");
        assert!(ItemName::parse(&"é".repeat(NAME_MAX_CHARS)).is_ok());
        assert!(ItemName::parse(&"a".repeat(NAME_MAX_CHARS + 1)).is_err());
    }

    #[test]
    fn name_rejects_blank_and_control_characters() {
        for bad in ["", "   ", "a\u{0}b", "line\nbreak", "esc\u{1b}[31m"] {
            assert_eq!(ItemName::parse(bad).unwrap_err().field, "name", "{bad:?}");
        }
    }

    #[test]
    fn unique_key_is_case_insensitive() {
        assert_eq!(
            ItemName::parse("Widget").unwrap().unique_key(),
            ItemName::parse("wIDGET").unwrap().unique_key()
        );
    }

    #[test]
    fn description_allows_multiline_but_not_other_controls() {
        assert!(Description::parse("").is_ok());
        assert!(Description::parse("line 1\nline 2\r\n\tindented").is_ok());
        assert!(Description::parse(&"d".repeat(DESCRIPTION_MAX_CHARS)).is_ok());
        assert!(Description::parse(&"d".repeat(DESCRIPTION_MAX_CHARS + 1)).is_err());
        assert!(Description::parse("bell\u{7}").is_err());
    }

    #[test]
    fn id_round_trips_and_rejects_garbage() {
        let id = ItemId::generate();
        assert_eq!(ItemId::parse(&id.to_string()).unwrap(), id);
        for bad in ["", "123", "not-a-uuid", "../../etc/passwd"] {
            assert_eq!(ItemId::parse(bad).unwrap_err().field, "id");
        }
    }

    #[test]
    fn new_items_get_distinct_ids() {
        let name = ItemName::parse("a").unwrap();
        let a = Item::new(name.clone(), Description::default());
        let b = Item::new(name, Description::default());
        assert_ne!(a.id(), b.id());
    }
}
