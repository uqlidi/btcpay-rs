//! Errors crossing the FFI boundary.

/// Returned by fallible plugin operations; surfaces in C# as a typed exception
/// (`PluginException.InvalidInput`, `PluginException.Internal`, …).
#[derive(Debug, thiserror::Error, uniffi::Error)]
pub enum PluginError {
    /// The operator supplied something invalid — surface it on the settings form.
    #[error("invalid input: {message}")]
    InvalidInput {
        /// Operator-facing explanation of what was wrong.
        message: String,
    },

    /// Configuration is missing or incomplete; the plugin cannot run yet.
    #[error("not configured: {message}")]
    NotConfigured {
        /// What is missing, and ideally where to set it.
        message: String,
    },

    /// An external system (exchange, node, third-party API) failed.
    #[error("external service error: {message}")]
    External {
        /// What failed, including the remote error where available.
        message: String,
    },

    /// Anything else, including a panic caught at the FFI boundary.
    #[error("internal error: {message}")]
    Internal {
        /// Diagnostic detail; for caught panics this names the method that panicked.
        message: String,
    },
}

impl PluginError {
    /// Builds an [`PluginError::Internal`].
    pub fn internal(message: impl Into<String>) -> Self {
        Self::Internal {
            message: message.into(),
        }
    }

    /// Builds an [`PluginError::InvalidInput`].
    pub fn invalid_input(message: impl Into<String>) -> Self {
        Self::InvalidInput {
            message: message.into(),
        }
    }

    /// Builds a [`PluginError::NotConfigured`].
    pub fn not_configured(message: impl Into<String>) -> Self {
        Self::NotConfigured {
            message: message.into(),
        }
    }

    /// Builds an [`PluginError::External`].
    pub fn external(message: impl Into<String>) -> Self {
        Self::External {
            message: message.into(),
        }
    }
}

/// Errors the *host* reports back to the plugin, e.g. a failed settings write.
#[derive(Debug, thiserror::Error, uniffi::Error)]
pub enum HostError {
    /// The host could not complete the requested operation.
    #[error("host operation failed: {message}")]
    Failed {
        /// What the host was unable to do.
        message: String,
    },
}
