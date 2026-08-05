//! Builders, so describing a page reads like describing a page.
//!
//! The types in [`crate::section`] are the wire format and can be constructed directly, but
//! doing so means naming every optional field. These exist so the common case is short.

use crate::field::{Field, FieldKind, SelectOption};
use crate::section::{Section, StatCard};

/// Builds a [`Section::Form`].
///
/// ```
/// use btcpay_ui::Form;
///
/// let form = Form::new("settings")
///     .text("api_key", "API key")
///     .required()
///     .help("From your exchange account")
///     .number("poll_secs", "Poll interval (seconds)")
///     .toggle("enabled", "Enabled");
/// ```
///
/// Modifiers such as [`Form::required`] apply to the field added most recently, so they read
/// in the order they are written.
#[derive(Debug, Clone)]
pub struct Form {
    id: String,
    title: Option<String>,
    fields: Vec<Field>,
    submit_label: String,
}

impl Form {
    /// A form identified by `id`, which appears in the submission.
    pub fn new(id: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            title: None,
            fields: Vec::new(),
            submit_label: "Save".to_string(),
        }
    }

    /// Adds a heading above the form.
    pub fn title(mut self, title: impl Into<String>) -> Self {
        self.title = Some(title.into());
        self
    }

    /// Changes the submit button's text.
    pub fn submit_label(mut self, label: impl Into<String>) -> Self {
        self.submit_label = label.into();
        self
    }

    /// Adds a single-line text input.
    pub fn text(self, id: impl Into<String>, label: impl Into<String>) -> Self {
        self.field(Field::new(
            id,
            label,
            FieldKind::Text {
                placeholder: None,
                max_length: None,
            },
        ))
    }

    /// Adds a secret input. Its value is never sent to the browser, and submitting it empty
    /// keeps whatever is stored.
    pub fn password(self, id: impl Into<String>, label: impl Into<String>) -> Self {
        self.field(Field::new(id, label, FieldKind::Password))
    }

    /// Adds a whole-number input.
    pub fn number(self, id: impl Into<String>, label: impl Into<String>) -> Self {
        self.field(Field::new(
            id,
            label,
            FieldKind::Number {
                min: None,
                max: None,
            },
        ))
    }

    /// Adds a checkbox.
    pub fn toggle(self, id: impl Into<String>, label: impl Into<String>) -> Self {
        self.field(Field::new(id, label, FieldKind::Toggle))
    }

    /// Adds a dropdown.
    pub fn select<V, L>(
        self,
        id: impl Into<String>,
        label: impl Into<String>,
        options: impl IntoIterator<Item = (V, L)>,
    ) -> Self
    where
        V: Into<String>,
        L: Into<String>,
    {
        let options = options
            .into_iter()
            .map(|(value, label)| SelectOption::new(value, label))
            .collect();
        self.field(Field::new(id, label, FieldKind::Select { options }))
    }

    /// Adds an already-built field.
    pub fn field(mut self, field: Field) -> Self {
        self.fields.push(field);
        self
    }

    /// Marks the most recently added field as required.
    pub fn required(mut self) -> Self {
        if let Some(field) = self.fields.last_mut() {
            field.required = true;
        }
        self
    }

    /// Adds help text to the most recently added field.
    pub fn help(mut self, help: impl Into<String>) -> Self {
        if let Some(field) = self.fields.last_mut() {
            field.help = Some(help.into());
        }
        self
    }

    /// Sets the current value of the most recently added field.
    pub fn value(mut self, value: impl Into<String>) -> Self {
        if let Some(field) = self.fields.last_mut() {
            let updated = std::mem::replace(field, placeholder_field()).value(value);
            *field = updated;
        }
        self
    }

    /// Bounds the most recently added field, if it is a number.
    pub fn range(mut self, min: i64, max: i64) -> Self {
        if let Some(field) = self.fields.last_mut() {
            if matches!(field.kind, FieldKind::Number { .. }) {
                field.kind = FieldKind::Number {
                    min: Some(min),
                    max: Some(max),
                };
            }
        }
        self
    }

    /// Sets the placeholder of the most recently added field, if it is text.
    pub fn placeholder(mut self, placeholder: impl Into<String>) -> Self {
        if let Some(field) = self.fields.last_mut() {
            if let FieldKind::Text { max_length, .. } = &field.kind {
                field.kind = FieldKind::Text {
                    placeholder: Some(placeholder.into()),
                    max_length: *max_length,
                };
            }
        }
        self
    }
}

/// Stands in while a field is rebuilt by value. Never observable.
fn placeholder_field() -> Field {
    Field::new("", "", FieldKind::Password)
}

impl From<Form> for Section {
    fn from(form: Form) -> Self {
        Section::Form {
            id: form.id,
            title: form.title,
            fields: form.fields,
            submit_label: form.submit_label,
        }
    }
}

/// Builds a [`Section::Table`].
#[derive(Debug, Clone)]
pub struct Table {
    title: Option<String>,
    columns: Vec<String>,
    rows: Vec<Vec<String>>,
    empty_message: Option<String>,
}

impl Table {
    /// A table with the given column headings.
    pub fn new<C: Into<String>>(columns: impl IntoIterator<Item = C>) -> Self {
        Self {
            title: None,
            columns: columns.into_iter().map(Into::into).collect(),
            rows: Vec::new(),
            empty_message: None,
        }
    }

    /// Adds a heading above the table.
    pub fn title(mut self, title: impl Into<String>) -> Self {
        self.title = Some(title.into());
        self
    }

    /// Appends a row.
    pub fn row<V: Into<String>>(mut self, cells: impl IntoIterator<Item = V>) -> Self {
        self.rows.push(cells.into_iter().map(Into::into).collect());
        self
    }

    /// Sets what to show when there are no rows.
    pub fn empty_message(mut self, message: impl Into<String>) -> Self {
        self.empty_message = Some(message.into());
        self
    }
}

impl From<Table> for Section {
    fn from(table: Table) -> Self {
        Section::Table {
            title: table.title,
            columns: table.columns,
            rows: table.rows,
            empty_message: table.empty_message,
        }
    }
}

/// Builds a [`Section::Stats`].
#[derive(Debug, Clone, Default)]
pub struct Stats {
    cards: Vec<StatCard>,
}

impl Stats {
    /// An empty row of cards.
    pub fn new() -> Self {
        Self::default()
    }

    /// Adds a card.
    pub fn card(mut self, label: impl Into<String>, value: impl Into<String>) -> Self {
        self.cards.push(StatCard::new(label, value));
        self
    }

    /// Adds detail to the most recently added card.
    pub fn detail(mut self, detail: impl Into<String>) -> Self {
        if let Some(card) = self.cards.pop() {
            self.cards.push(card.detail(detail));
        }
        self
    }
}

impl From<Stats> for Section {
    fn from(stats: Stats) -> Self {
        Section::Stats { cards: stats.cards }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fields_of(section: Section) -> Vec<Field> {
        match section {
            Section::Form { fields, .. } => fields,
            other => panic!("expected a form, got {other:?}"),
        }
    }

    #[test]
    fn modifiers_apply_to_the_field_just_added() {
        // Reading order is the whole point: `.text(..).required()` should mark that text
        // field, not some other one.
        let fields = fields_of(
            Form::new("f")
                .text("a", "A")
                .required()
                .text("b", "B")
                .help("about b")
                .into(),
        );

        assert!(fields[0].required, "the first field was the one marked");
        assert!(!fields[1].required);
        assert_eq!(fields[1].help.as_deref(), Some("about b"));
        assert_eq!(fields[0].help, None);
    }

    #[test]
    fn a_modifier_with_no_field_to_apply_to_is_ignored() {
        // Rather than panicking on a mistake that is obvious in the rendered page.
        let fields = fields_of(Form::new("f").required().help("nothing").into());

        assert!(fields.is_empty());
    }

    #[test]
    fn range_only_applies_to_numbers() {
        let fields = fields_of(Form::new("f").text("t", "T").range(1, 10).into());

        assert!(
            matches!(fields[0].kind, FieldKind::Text { .. }),
            "a text field should be left alone"
        );
    }

    #[test]
    fn setting_a_value_on_a_secret_is_refused_by_the_field_itself() {
        let fields = fields_of(Form::new("f").password("k", "Key").value("secret").into());

        assert_eq!(fields[0].value, None);
    }

    #[test]
    fn a_form_defaults_to_a_sensible_submit_label() {
        match Section::from(Form::new("f")) {
            Section::Form { submit_label, .. } => assert_eq!(submit_label, "Save"),
            other => panic!("expected a form, got {other:?}"),
        }
    }

    #[test]
    fn select_options_keep_their_order_and_labels() {
        let fields = fields_of(
            Form::new("f")
                .select("n", "Network", [("main", "Mainnet"), ("test", "Testnet")])
                .into(),
        );

        match &fields[0].kind {
            FieldKind::Select { options } => {
                assert_eq!(options.len(), 2);
                assert_eq!(options[0].value, "main");
                assert_eq!(options[0].label, "Mainnet");
            }
            other => panic!("expected a select, got {other:?}"),
        }
    }

    #[test]
    fn a_table_can_describe_its_own_emptiness() {
        match Section::from(Table::new(["A"]).empty_message("Nothing yet")) {
            Section::Table {
                rows,
                empty_message,
                ..
            } => {
                assert!(rows.is_empty());
                assert_eq!(empty_message.as_deref(), Some("Nothing yet"));
            }
            other => panic!("expected a table, got {other:?}"),
        }
    }
}
