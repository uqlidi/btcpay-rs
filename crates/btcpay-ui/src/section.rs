//! The blocks a page is made of.

use serde::{Deserialize, Serialize};

use crate::field::Field;

/// A block on a page.
///
/// Tagged by `type` in JSON. A host that meets an unknown type renders a placeholder telling
/// the operator to update, rather than failing to parse the page.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum Section {
    /// Inputs the operator can edit and submit.
    #[serde(rename_all = "camelCase")]
    Form {
        /// Identifies the form in a submission. Unique within a document.
        id: String,
        /// Optional heading above the form.
        #[serde(skip_serializing_if = "Option::is_none")]
        title: Option<String>,
        /// The inputs, in order.
        fields: Vec<Field>,
        /// Text on the submit button.
        submit_label: String,
    },

    /// Rows of read-only data.
    #[serde(rename_all = "camelCase")]
    Table {
        /// Optional heading above the table.
        #[serde(skip_serializing_if = "Option::is_none")]
        title: Option<String>,
        /// Column headings.
        columns: Vec<String>,
        /// Rows, each matching `columns` in length. Ragged rows are padded when rendered.
        rows: Vec<Vec<String>>,
        /// Shown instead of an empty table.
        #[serde(skip_serializing_if = "Option::is_none")]
        empty_message: Option<String>,
    },

    /// A row of headline numbers.
    #[serde(rename_all = "camelCase")]
    Stats {
        /// The cards, in order.
        cards: Vec<StatCard>,
    },

    /// A coloured notice.
    #[serde(rename_all = "camelCase")]
    Alert {
        /// How prominent the notice is.
        level: AlertLevel,
        /// The message. Plain text; it is HTML-encoded when rendered.
        text: String,
    },

    /// Buttons that ask the plugin to do something.
    ///
    /// Distinct from a form's submit button: a form saves values, a command asks for work.
    #[serde(rename_all = "camelCase")]
    Actions {
        /// Optional heading above the buttons.
        #[serde(skip_serializing_if = "Option::is_none")]
        title: Option<String>,
        /// The buttons, in order.
        buttons: Vec<Button>,
    },

    /// A paragraph of plain text.
    #[serde(rename_all = "camelCase")]
    Text {
        /// The text. Plain, not markup: it is HTML-encoded when rendered.
        text: String,
    },
}

/// One headline number.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StatCard {
    /// What the number means.
    pub label: String,
    /// The number, already formatted. Formatting stays with the plugin, which knows the
    /// currency, precision and units involved.
    pub value: String,
    /// Optional detail under the value.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
}

impl StatCard {
    /// A card showing `value` under `label`.
    pub fn new(label: impl Into<String>, value: impl Into<String>) -> Self {
        Self {
            label: label.into(),
            value: value.into(),
            detail: None,
        }
    }

    /// Adds a line of detail under the value.
    pub fn detail(mut self, detail: impl Into<String>) -> Self {
        self.detail = Some(detail.into());
        self
    }
}

/// A button that asks the plugin to do something.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Button {
    /// Identifies the command, and is what the plugin receives when it is pressed.
    pub command: String,
    /// Text on the button.
    pub label: String,
    /// How prominent, and how alarming, the button looks.
    pub style: ButtonStyle,
    /// When set, the operator is asked to confirm, and this is what they are asked.
    ///
    /// A command that moves funds or cannot be undone should always set this. The host
    /// enforces it; a plugin cannot be surprised by an unconfirmed press.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub confirm: Option<String>,
}

impl Button {
    /// An ordinary button.
    pub fn new(command: impl Into<String>, label: impl Into<String>) -> Self {
        Self {
            command: command.into(),
            label: label.into(),
            style: ButtonStyle::Secondary,
            confirm: None,
        }
    }

    /// The page's main action.
    pub fn primary(mut self) -> Self {
        self.style = ButtonStyle::Primary;
        self
    }

    /// Marks the button as destructive, and requires confirmation.
    ///
    /// The two go together deliberately: a destructive command without a confirmation is one
    /// stray click away from something irreversible.
    pub fn destructive(mut self, question: impl Into<String>) -> Self {
        self.style = ButtonStyle::Danger;
        self.confirm = Some(question.into());
        self
    }

    /// Asks the operator to confirm before the command runs.
    pub fn confirm(mut self, question: impl Into<String>) -> Self {
        self.confirm = Some(question.into());
        self
    }
}

/// How a [`Button`] looks.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ButtonStyle {
    /// The page's main action.
    Primary,
    /// An ordinary action.
    Secondary,
    /// Something destructive.
    Danger,
}

/// How prominent an [`Section::Alert`] is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum AlertLevel {
    /// Neutral information.
    Info,
    /// Something went well.
    Success,
    /// Something needs attention but still works.
    Warning,
    /// Something is broken.
    Danger,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sections_are_tagged_so_an_unknown_one_can_be_skipped() {
        // The host switches on this tag; without it an unrecognised section could not be
        // told apart from a malformed document.
        let json = serde_json::to_string(&Section::Text {
            text: "hello".into(),
        })
        .unwrap();

        assert!(json.contains(r#""type":"text""#), "got: {json}");
    }

    #[test]
    fn alert_levels_match_the_names_the_host_renders() {
        let json = serde_json::to_string(&AlertLevel::Warning).unwrap();

        assert_eq!(json, r#""warning""#);
    }

    #[test]
    fn optional_parts_are_omitted_rather_than_sent_as_null() {
        // Keeps the wire format small and lets the host treat absent and empty alike.
        let json = serde_json::to_string(&StatCard::new("Swaps", "12")).unwrap();

        assert!(!json.contains("detail"), "got: {json}");
        assert!(json.contains(r#""label":"Swaps""#));
    }
}
