//! The page a plugin hands to the host.

use serde::{Deserialize, Serialize};

use crate::section::Section;

/// Version of the JSON shape below.
///
/// Bumped only when the *document* changes shape, not when a node kind is added: new node
/// kinds are handled by the host skipping what it does not recognise. A host seeing a version
/// it does not know can say so plainly rather than mis-rendering.
pub const WIRE_VERSION: u32 = 1;

/// A page: a title and the blocks that make it up.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Document {
    /// Always [`WIRE_VERSION`] for documents built here.
    pub wire_version: u32,
    /// Heading for the page.
    pub title: String,
    /// Blocks, rendered in order.
    pub sections: Vec<Section>,
}

impl Document {
    /// An empty page with the given title.
    pub fn new(title: impl Into<String>) -> Self {
        Self {
            wire_version: WIRE_VERSION,
            title: title.into(),
            sections: Vec::new(),
        }
    }

    /// A page with nothing on it, for plugins that expose no settings.
    pub fn empty() -> Self {
        Self::new("")
    }

    /// Appends a section.
    pub fn section(mut self, section: impl Into<Section>) -> Self {
        self.sections.push(section.into());
        self
    }

    /// Appends a form. Shorthand for [`Document::section`].
    pub fn form(self, form: crate::Form) -> Self {
        self.section(form)
    }

    /// Appends a table.
    pub fn table(self, table: crate::Table) -> Self {
        self.section(table)
    }

    /// Appends a row of headline numbers.
    pub fn stats(self, stats: crate::Stats) -> Self {
        self.section(stats)
    }

    /// Appends a notice.
    pub fn alert(self, level: crate::AlertLevel, text: impl Into<String>) -> Self {
        self.section(Section::Alert {
            level,
            text: text.into(),
        })
    }

    /// Appends a paragraph.
    pub fn text(self, text: impl Into<String>) -> Self {
        self.section(Section::Text { text: text.into() })
    }

    /// Whether the page has nothing to show.
    pub fn is_empty(&self) -> bool {
        self.sections.is_empty()
    }

    /// Serialises to the JSON the host reads.
    ///
    /// Infallible in practice: every type here is plain data with no map keys that could
    /// collide and no values serde can refuse. A failure would mean a bug in this crate, so
    /// it produces an empty document rather than making every caller handle an error that
    /// cannot happen.
    pub fn to_json(&self) -> String {
        serde_json::to_string(self)
            .unwrap_or_else(|_| String::from(r#"{"wireVersion":1,"title":"","sections":[]}"#))
    }

    /// Parses a document produced by [`Document::to_json`].
    pub fn from_json(json: &str) -> Result<Self, String> {
        serde_json::from_str(json).map_err(|e| format!("not a valid UI document: {e}"))
    }

    /// Finds a form by id, for validating a submission against it.
    pub fn form_by_id(&self, form_id: &str) -> Option<&Section> {
        self.sections
            .iter()
            .find(|section| matches!(section, Section::Form { id, .. } if id == form_id))
    }
}

impl Default for Document {
    fn default() -> Self {
        Self::empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{AlertLevel, Form, Stats, Table};

    #[test]
    fn a_document_round_trips_through_json() {
        let original = Document::new("Settings")
            .form(Form::new("main").text("api_key", "API key").required())
            .table(Table::new(["When", "What"]).row(["now", "a thing"]))
            .stats(Stats::new().card("Swaps", "12"))
            .alert(AlertLevel::Warning, "Careful")
            .text("Some explanation.");

        let parsed = Document::from_json(&original.to_json()).expect("should parse");

        assert_eq!(parsed, original);
        assert_eq!(parsed.sections.len(), 5);
    }

    #[test]
    fn the_wire_format_uses_the_casing_the_host_expects() {
        // The host reads this with System.Text.Json, which is configured for camelCase.
        let json = Document::new("t").to_json();

        assert!(json.contains(r#""wireVersion":1"#), "got: {json}");
        assert!(!json.contains("wire_version"));
    }

    #[test]
    fn a_form_can_be_found_again_to_validate_a_submission() {
        // The host re-asks the plugin for the document on submit, then checks the values
        // against it, so a tampered post cannot bypass the plugin's own constraints.
        let doc = Document::new("Settings").form(Form::new("main").text("key", "Key"));

        assert!(doc.form_by_id("main").is_some());
        assert!(doc.form_by_id("nonexistent").is_none());
    }

    #[test]
    fn an_empty_document_is_still_valid_json() {
        let parsed = Document::from_json(&Document::empty().to_json()).unwrap();

        assert!(parsed.is_empty());
    }

    #[test]
    fn a_malformed_document_is_reported_rather_than_panicking() {
        let err = Document::from_json("{not json").unwrap_err();

        assert!(err.contains("not a valid UI document"), "got: {err}");
    }
}
