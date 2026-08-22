//! Describe a BTCPay plugin's UI as data, and let the host render it.
//!
//! A plugin returns a [`Document`]; generic Razor views in `BtcpayRs.Host` turn it into a
//! page using BTCPay's own styles. No Razor, no C#, no HTML.
//!
//! ```
//! use btcpay_ui::{Document, Form};
//!
//! let doc = Document::new("Swap settings").form(
//!     Form::new("settings")
//!         .text("api_key", "API key")
//!         .number("poll_secs", "Poll interval (seconds)")
//! );
//! ```
//!
//! # Why the tree travels as JSON
//!
//! Only [`Document`] crosses the FFI, as a single JSON string. Modelling each node as a
//! generated type would make every new node kind an ABI break, forcing every plugin to be
//! rebuilt. As JSON, a host that meets a node it does not know can render a placeholder and
//! carry on, so a plugin built against a newer btcpay-rs degrades instead of failing.
//!
//! [`WIRE_VERSION`] exists so the host can tell the difference between "a node I do not know"
//! and "a document shaped differently from what I expect".
//!
//! # Escaping
//!
//! Nothing here carries HTML. Every string is data, and the host HTML-encodes it when
//! rendering. A plugin cannot inject markup into BTCPay's admin pages through this type,
//! by construction.

#![deny(missing_docs)]
#![warn(clippy::all)]

mod builder;
mod document;
mod field;
mod section;

pub use builder::{Actions, Form, Stats, Table};
pub use document::{Document, WIRE_VERSION};
pub use field::{Field, FieldKind, SelectOption};
pub use section::{AlertLevel, Button, ButtonStyle, Section, StatCard};

/// Everything needed to build a page, in one import.
pub mod prelude {
    pub use crate::{Actions, AlertLevel, Button, Document, Field, Form, Section, Stats, Table};
}
