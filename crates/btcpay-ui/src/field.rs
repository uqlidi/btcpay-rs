//! Form fields.

use serde::{Deserialize, Serialize};

/// One input on a form.
///
/// Common attributes live here rather than being repeated per kind, so adding an attribute
/// (a placeholder, a help note) applies everywhere at once.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Field {
    /// Identifies the field in submitted values and in storage. Must be unique in its form.
    pub id: String,
    /// Shown next to the input.
    pub label: String,
    /// Optional explanation shown under the input.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub help: Option<String>,
    /// Whether the operator must supply a value.
    pub required: bool,
    /// The current value, shown when the form is rendered.
    ///
    /// Never populated for [`FieldKind::Password`]: see [`Field::is_secret`].
    #[serde(skip_serializing_if = "Option::is_none")]
    pub value: Option<String>,
    /// What kind of input this is.
    ///
    /// Flattened, so a field reads `"kind": "number", "min": 1` rather than nesting a second
    /// object that repeats the tag.
    #[serde(flatten)]
    pub kind: FieldKind,
}

impl Field {
    /// A required-by-default text field. Prefer the [`crate::Form`] builder.
    pub fn new(id: impl Into<String>, label: impl Into<String>, kind: FieldKind) -> Self {
        Self {
            id: id.into(),
            label: label.into(),
            help: None,
            required: false,
            value: None,
            kind,
        }
    }

    /// Adds explanatory text under the input.
    pub fn help(mut self, help: impl Into<String>) -> Self {
        self.help = Some(help.into());
        self
    }

    /// Marks the field as required.
    pub fn required(mut self) -> Self {
        self.required = true;
        self
    }

    /// Sets the value shown when the form is rendered.
    ///
    /// Ignored for secret fields, which are never echoed back to the browser.
    pub fn value(mut self, value: impl Into<String>) -> Self {
        if !self.is_secret() {
            self.value = Some(value.into());
        }
        self
    }

    /// Whether this field holds something that must not be sent back to the browser.
    ///
    /// A stored API key is readable by anyone who can view the page source, or anyone looking
    /// over the operator's shoulder, so secret values are never rendered. The host shows a
    /// placeholder instead and keeps the stored value when the field is submitted empty.
    pub fn is_secret(&self) -> bool {
        matches!(self.kind, FieldKind::Password)
    }

    /// Checks a submitted value against this field's constraints.
    ///
    /// Applied by the host before the plugin sees the submission, and available to plugins
    /// that want to check the same rules themselves.
    pub fn validate(&self, submitted: &str) -> Result<(), String> {
        let trimmed = submitted.trim();

        if trimmed.is_empty() {
            // An empty secret means "keep what is stored", not "clear it": the browser was
            // never sent the current value, so it cannot send it back.
            if self.required && !self.is_secret() {
                return Err(format!("{} is required", self.label));
            }
            return Ok(());
        }

        match &self.kind {
            FieldKind::Number { min, max } => {
                let parsed: i64 = trimmed
                    .parse()
                    .map_err(|_| format!("{} must be a whole number", self.label))?;
                if let Some(min) = min {
                    if parsed < *min {
                        return Err(format!("{} must be at least {min}", self.label));
                    }
                }
                if let Some(max) = max {
                    if parsed > *max {
                        return Err(format!("{} must be at most {max}", self.label));
                    }
                }
            }
            FieldKind::Select { options } => {
                if !options.iter().any(|o| o.value == trimmed) {
                    return Err(format!(
                        "{} is not one of the available options",
                        self.label
                    ));
                }
            }
            FieldKind::Text { max_length, .. } => {
                if let Some(max) = max_length {
                    if trimmed.chars().count() > *max as usize {
                        return Err(format!("{} must be {max} characters or fewer", self.label));
                    }
                }
            }
            FieldKind::Toggle | FieldKind::Password => {}
        }
        Ok(())
    }
}

/// The kinds of input a form can contain.
///
/// Tagged by `kind` in JSON so a host that meets an unknown kind can skip it rather than fail
/// to parse the document.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum FieldKind {
    /// Single-line text.
    #[serde(rename_all = "camelCase")]
    Text {
        /// Greyed-out example shown in the empty input.
        #[serde(skip_serializing_if = "Option::is_none")]
        placeholder: Option<String>,
        /// Longest accepted value, in characters.
        #[serde(skip_serializing_if = "Option::is_none")]
        max_length: Option<u32>,
    },
    /// A secret. Never rendered with its value; submitting it empty keeps what is stored.
    Password,
    /// A whole number, optionally bounded.
    #[serde(rename_all = "camelCase")]
    Number {
        /// Smallest accepted value.
        #[serde(skip_serializing_if = "Option::is_none")]
        min: Option<i64>,
        /// Largest accepted value.
        #[serde(skip_serializing_if = "Option::is_none")]
        max: Option<i64>,
    },
    /// A checkbox. Submitted as `"true"` or `"false"`.
    Toggle,
    /// A dropdown.
    #[serde(rename_all = "camelCase")]
    Select {
        /// What the operator can choose from.
        options: Vec<SelectOption>,
    },
}

/// One choice in a [`FieldKind::Select`].
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SelectOption {
    /// Stored and submitted.
    pub value: String,
    /// Shown to the operator.
    pub label: String,
}

impl SelectOption {
    /// A choice whose stored value and label differ.
    pub fn new(value: impl Into<String>, label: impl Into<String>) -> Self {
        Self {
            value: value.into(),
            label: label.into(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn number(min: Option<i64>, max: Option<i64>) -> Field {
        Field::new("n", "Count", FieldKind::Number { min, max })
    }

    #[test]
    fn a_required_field_rejects_nothing_but_accepts_a_value() {
        let field = Field::new("name", "Name", text_kind()).required();

        assert!(field.validate("").is_err());
        assert!(field.validate("   ").is_err(), "whitespace is not a value");
        assert!(field.validate("something").is_ok());
    }

    #[test]
    fn numbers_are_checked_against_their_bounds() {
        let field = number(Some(5), Some(10));

        assert!(field.validate("7").is_ok());
        assert!(field.validate("4").is_err());
        assert!(field.validate("11").is_err());
        assert!(field.validate("seven").is_err());
    }

    #[test]
    fn the_error_names_the_field_so_an_operator_can_act_on_it() {
        let err = number(Some(5), None).validate("1").unwrap_err();

        assert!(err.contains("Count"), "should name the field: {err}");
        assert!(err.contains('5'), "should state the bound: {err}");
    }

    #[test]
    fn a_select_rejects_a_value_that_is_not_offered() {
        let field = Field::new(
            "network",
            "Network",
            FieldKind::Select {
                options: vec![SelectOption::new("main", "Mainnet")],
            },
        );

        assert!(field.validate("main").is_ok());
        // Guards against a tampered form post, which the browser would never produce.
        assert!(field.validate("regtest").is_err());
    }

    #[test]
    fn a_secret_never_carries_its_value_into_the_page() {
        let field = Field::new("api_key", "API key", FieldKind::Password).value("s3cret");

        assert!(field.is_secret());
        assert_eq!(
            field.value, None,
            "a stored secret must not reach the browser"
        );
    }

    #[test]
    fn an_empty_secret_means_keep_the_stored_one() {
        // The browser was never given the current value, so it cannot send it back. Treating
        // empty as "clear it" would wipe the operator's key every time they saved the form.
        let field = Field::new("api_key", "API key", FieldKind::Password).required();

        assert!(field.validate("").is_ok());
    }

    #[test]
    fn text_length_is_enforced() {
        let field = Field::new(
            "note",
            "Note",
            FieldKind::Text {
                placeholder: None,
                max_length: Some(3),
            },
        );

        assert!(field.validate("abc").is_ok());
        assert!(field.validate("abcd").is_err());
    }

    fn text_kind() -> FieldKind {
        FieldKind::Text {
            placeholder: None,
            max_length: None,
        }
    }
}
