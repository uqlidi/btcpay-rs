//! Types that render as a dropdown.

/// A fixed set of options, rendered as a dropdown on a settings form.
///
/// Implement it with `#[derive(BtcpayChoice)]` on a unit-only enum:
///
/// ```ignore
/// #[derive(Default, Clone, Copy, PartialEq, BtcpayChoice)]
/// enum Network {
///     #[choice(label = "Mainnet")]
///     #[default]
///     Main,
///     #[choice(label = "Testnet")]
///     Test,
/// }
/// ```
///
/// A field of that type in a `#[derive(BtcpaySettings)]` struct then becomes a dropdown whose
/// options come from the type itself, so the form and the values the field can hold cannot
/// disagree. That is the point: a `String` holding `"main"` or `"test"` can also hold
/// anything else, and every match on it needs an arm for a case that should be impossible.
#[diagnostic::on_unimplemented(
    message = "`{Self}` cannot be used as a settings field",
    label = "this type",
    note = "settings fields may be String, bool, an integer, or a type with #[derive(BtcpayChoice)]",
    note = "for a form whose shape depends on runtime state, describe it with the builder API instead"
)]
pub trait Choice: Sized {
    /// Every option, as `(stored value, label shown to the operator)`, in display order.
    fn choices() -> Vec<(String, String)>;

    /// The stored value for this instance.
    fn choice_value(&self) -> String;

    /// Parses a stored value, returning `None` when it is not one of the options.
    ///
    /// Returning `None` rather than a default lets the caller decide: loading falls back,
    /// while a submission is rejected.
    fn from_choice_value(value: &str) -> Option<Self>;
}
