//! `#[derive(BtcpaySettings)]`: one struct, and the form, storage and parsing follow from it.

use proc_macro2::TokenStream;
use quote::quote;
use syn::{Data, DeriveInput, Error, Fields, LitStr, Type};

/// Generates the settings form, typed loading and saving, and validation.
pub fn derive(input: DeriveInput) -> Result<TokenStream, Error> {
    let name = &input.ident;

    let Data::Struct(data) = &input.data else {
        return Err(Error::new_spanned(
            &input.ident,
            "BtcpaySettings works on a struct with named fields",
        ));
    };
    let Fields::Named(named) = &data.fields else {
        return Err(Error::new_spanned(
            &data.fields,
            "BtcpaySettings needs named fields: each one becomes a form field",
        ));
    };

    let fields: Vec<SettingField> = named
        .named
        .iter()
        .map(SettingField::parse)
        .collect::<Result<_, _>>()?;

    if fields.is_empty() {
        return Err(Error::new_spanned(
            &input.ident,
            "BtcpaySettings needs at least one field",
        ));
    }

    let form_fields = fields.iter().map(SettingField::to_form_call);
    let loads = fields.iter().map(SettingField::to_load);
    // Collected rather than lazy: the same parsing is emitted twice, once in `update` and once
    // in `from_values`, so that the two cannot drift.
    let parses = fields
        .iter()
        .map(SettingField::to_parse)
        .collect::<Vec<_>>();
    let stores = fields.iter().map(SettingField::to_store);

    Ok(quote! {
        impl #name {
            /// The form describing these settings, with the current values filled in.
            ///
            /// Secret fields carry no value: the contract refuses to send a stored secret to
            /// the browser.
            pub fn form(&self) -> ::btcpay_plugin::ui::Form {
                ::btcpay_plugin::ui::Form::new("settings")
                    #(#form_fields)*
            }

            /// Reads these settings from the host, falling back to [`Default`] per field.
            ///
            /// A value that fails to parse falls back rather than failing the load: settings
            /// come from storage that an operator or an older version may have written, and a
            /// plugin that refuses to start over one bad value is worse than one that uses a
            /// default and says so.
            pub fn load(host: &dyn ::btcpay_plugin::HostServices) -> Self
            where
                Self: ::core::default::Default,
            {
                let mut settings = <Self as ::core::default::Default>::default();
                #(#loads)*
                settings
            }

            /// Applies a submission on top of the values already held, leaving a field the
            /// submission does not mention alone.
            ///
            /// **Use this rather than [`Self::from_values`] whenever any field is `secret`.**
            /// The host omits an untouched secret from the submission, so that a stored password
            /// survives a save in which the operator did not retype it. That only works if the
            /// omission is applied to the current settings; parsing onto [`Default`] instead
            /// turns "leave it alone" into "reset it", quietly wiping the stored value.
            ///
            /// Applies the same rules the form declares, and rejects the whole submission if any
            /// of them fail. `self` is then left partly updated, so parse into a clone if a
            /// rejection must leave the live settings untouched.
            pub fn update(
                &mut self,
                values: &::std::collections::HashMap<::std::string::String, ::std::string::String>,
            ) -> ::core::result::Result<(), ::btcpay_plugin::PluginError> {
                // Named so the generated per-field parsing reads the same in both methods.
                let settings = self;
                #(#parses)*
                ::core::result::Result::Ok(())
            }

            /// Parses a submission into fresh settings, applying the same rules the form
            /// declares.
            ///
            /// Use this in `SettingsUpdated`: the values are the submission, and reading them
            /// from storage there would see the previous ones.
            ///
            /// Fields the submission does not mention take their [`Default`], which is wrong for
            /// a `secret` field the operator did not retype. Use [`Self::update`] when the
            /// struct has one.
            pub fn from_values(
                values: &::std::collections::HashMap<::std::string::String, ::std::string::String>,
            ) -> ::core::result::Result<Self, ::btcpay_plugin::PluginError>
            where
                Self: ::core::default::Default,
            {
                let mut settings = <Self as ::core::default::Default>::default();
                settings.update(values)?;
                ::core::result::Result::Ok(settings)
            }

            /// These settings as values to persist, for `PluginAction::SaveSettings`.
            pub fn to_values(
                &self,
            ) -> ::std::collections::HashMap<::std::string::String, ::std::string::String> {
                let mut values = ::std::collections::HashMap::new();
                #(#stores)*
                values
            }
        }
    })
}

/// What kind of input a field's Rust type calls for.
#[derive(Clone, PartialEq, Debug)]
enum Kind {
    Text,
    Secret,
    Number,
    Toggle,
    /// Anything else: rendered as a dropdown via the `Choice` trait.
    ///
    /// A derive cannot inspect a field's type, so rather than requiring a marker attribute,
    /// an unrecognised type emits trait calls. If it does not implement `Choice`, the compiler
    /// says so, and the trait's `on_unimplemented` note explains the options.
    Choice(Box<Type>),
}

struct SettingField {
    ident: syn::Ident,
    key: String,
    label: String,
    help: Option<String>,
    required: bool,
    kind: Kind,
    min: Option<i64>,
    max: Option<i64>,
}

impl SettingField {
    fn parse(field: &syn::Field) -> Result<Self, Error> {
        let ident = field
            .ident
            .clone()
            .ok_or_else(|| Error::new_spanned(field, "a settings field must be named"))?;

        let mut parsed = Self {
            key: ident.to_string(),
            label: humanise(&ident.to_string()),
            help: None,
            required: false,
            kind: kind_of(&field.ty)?,
            min: None,
            max: None,
            ident,
        };

        for attr in &field.attrs {
            if !attr.path().is_ident("setting") {
                continue;
            }

            attr.parse_nested_meta(|meta| {
                let name = meta
                    .path
                    .get_ident()
                    .map(|i| i.to_string())
                    .unwrap_or_default();

                match name.as_str() {
                    "label" => parsed.label = meta.value()?.parse::<LitStr>()?.value(),
                    "help" => parsed.help = Some(meta.value()?.parse::<LitStr>()?.value()),
                    "key" => parsed.key = meta.value()?.parse::<LitStr>()?.value(),
                    "required" => parsed.required = true,
                    "secret" => {
                        if parsed.kind != Kind::Text {
                            return Err(meta.error("only a String field can be a secret"));
                        }
                        parsed.kind = Kind::Secret;
                    }
                    "min" => {
                        parsed.min = Some(meta.value()?.parse::<syn::LitInt>()?.base10_parse()?)
                    }
                    "max" => {
                        parsed.max = Some(meta.value()?.parse::<syn::LitInt>()?.base10_parse()?)
                    }
                    other => {
                        return Err(meta.error(format!(
                            "unknown setting `{other}`; expected label, help, key, required, \
                             secret, min or max"
                        )))
                    }
                }
                Ok(())
            })?;
        }

        if (parsed.min.is_some() || parsed.max.is_some()) && parsed.kind != Kind::Number {
            return Err(Error::new_spanned(
                &parsed.ident,
                "min and max only apply to a numeric field",
            ));
        }

        Ok(parsed)
    }

    fn to_form_call(&self) -> TokenStream {
        let key = &self.key;
        let label = &self.label;
        let ident = &self.ident;

        let add = match &self.kind {
            Kind::Text => quote! { .text(#key, #label) },
            Kind::Secret => quote! { .password(#key, #label) },
            Kind::Number => quote! { .number(#key, #label) },
            Kind::Toggle => quote! { .toggle(#key, #label) },
            // Options come from the type, so they cannot disagree with what it can hold.
            Kind::Choice(ty) => quote! {
                .select(
                    #key,
                    #label,
                    <#ty as ::btcpay_plugin::Choice>::choices(),
                )
            },
        };

        let range = match (self.min, self.max) {
            (Some(min), Some(max)) => quote! { .range(#min, #max) },
            // The builder takes both together; an open bound uses the type's limit so the
            // form still communicates the one bound that was asked for.
            (Some(min), None) => quote! { .range(#min, i64::MAX) },
            (None, Some(max)) => quote! { .range(i64::MIN, #max) },
            (None, None) => quote! {},
        };

        let required = if self.required {
            quote! { .required() }
        } else {
            quote! {}
        };

        let help = match &self.help {
            Some(help) => quote! { .help(#help) },
            None => quote! {},
        };

        // Secret values are dropped by the field itself, so this is uniform.
        let value = match &self.kind {
            Kind::Choice(ty) => quote! {
                .value(<#ty as ::btcpay_plugin::Choice>::choice_value(&self.#ident))
            },
            _ => quote! { .value(::std::string::ToString::to_string(&self.#ident)) },
        };

        quote! { #add #range #required #help #value }
    }

    fn to_load(&self) -> TokenStream {
        let key = &self.key;
        let ident = &self.ident;

        match &self.kind {
            Kind::Text | Kind::Secret => quote! {
                if let ::core::option::Option::Some(value) =
                    host.get_setting(::std::string::String::from(#key))
                {
                    settings.#ident = value;
                }
            },
            Kind::Choice(ty) => quote! {
                if let ::core::option::Option::Some(value) =
                    host.get_setting(::std::string::String::from(#key))
                {
                    // A stored value that is no longer an option falls back, rather than
                    // failing the load. An option removed in a newer version would otherwise
                    // stop the plugin starting.
                    if let ::core::option::Option::Some(parsed) =
                        <#ty as ::btcpay_plugin::Choice>::from_choice_value(&value)
                    {
                        settings.#ident = parsed;
                    }
                }
            },
            _ => quote! {
                if let ::core::option::Option::Some(value) =
                    host.get_setting(::std::string::String::from(#key))
                {
                    if let ::core::result::Result::Ok(parsed) = value.trim().parse() {
                        settings.#ident = parsed;
                    }
                }
            },
        }
    }

    fn to_parse(&self) -> TokenStream {
        let key = &self.key;
        let label = &self.label;
        let ident = &self.ident;

        let required_check = if self.required && self.kind != Kind::Secret {
            quote! {
                if value.trim().is_empty() {
                    return ::core::result::Result::Err(
                        ::btcpay_plugin::PluginError::invalid_input(
                            ::std::format!("{} is required", #label),
                        ),
                    );
                }
            }
        } else {
            quote! {}
        };

        let bounds = {
            let min = match self.min {
                Some(min) => quote! {
                    if parsed < #min {
                        return ::core::result::Result::Err(
                            ::btcpay_plugin::PluginError::invalid_input(
                                ::std::format!("{} must be at least {}", #label, #min),
                            ),
                        );
                    }
                },
                None => quote! {},
            };
            let max = match self.max {
                Some(max) => quote! {
                    if parsed > #max {
                        return ::core::result::Result::Err(
                            ::btcpay_plugin::PluginError::invalid_input(
                                ::std::format!("{} must be at most {}", #label, #max),
                            ),
                        );
                    }
                },
                None => quote! {},
            };
            quote! { #min #max }
        };

        match &self.kind {
            Kind::Choice(ty) => {
                // Unlike loading, a submission with an unknown value is refused: the form
                // offered a fixed set, so anything else is a tampered post.
                quote! {
                    if let ::core::option::Option::Some(value) = values.get(#key) {
                        settings.#ident =
                            <#ty as ::btcpay_plugin::Choice>::from_choice_value(value)
                                .ok_or_else(|| {
                                    ::btcpay_plugin::PluginError::invalid_input(
                                        ::std::format!(
                                            "{} is not one of the available options", #label,
                                        ),
                                    )
                                })?;
                    }
                }
            }
            Kind::Text | Kind::Secret => {
                // An absent secret means "keep what is stored", which is why the host omits
                // it from the submission rather than sending an empty string.
                quote! {
                    if let ::core::option::Option::Some(value) = values.get(#key) {
                        #required_check
                        settings.#ident = ::std::clone::Clone::clone(value);
                    }
                }
            }
            Kind::Toggle => quote! {
                if let ::core::option::Option::Some(value) = values.get(#key) {
                    settings.#ident = value.trim().eq_ignore_ascii_case("true");
                }
            },
            Kind::Number => quote! {
                if let ::core::option::Option::Some(value) = values.get(#key) {
                    #required_check
                    if !value.trim().is_empty() {
                        // Checked as i64 so a bound reads the same here as on the form, then
                        // converted; a value outside the field's own type is reported rather
                        // than wrapping.
                        let parsed: i64 = value.trim().parse().map_err(|_| {
                            ::btcpay_plugin::PluginError::invalid_input(
                                ::std::format!("{} must be a whole number", #label),
                            )
                        })?;
                        #bounds
                        settings.#ident = ::core::convert::TryFrom::try_from(parsed).map_err(|_| {
                            ::btcpay_plugin::PluginError::invalid_input(
                                ::std::format!("{} is out of range", #label),
                            )
                        })?;
                    }
                }
            },
        }
    }

    fn to_store(&self) -> TokenStream {
        let key = &self.key;
        let ident = &self.ident;
        let value = match &self.kind {
            Kind::Choice(ty) => quote! {
                <#ty as ::btcpay_plugin::Choice>::choice_value(&self.#ident)
            },
            _ => quote! { ::std::string::ToString::to_string(&self.#ident) },
        };
        quote! {
            values.insert(::std::string::String::from(#key), #value);
        }
    }
}

/// Maps a Rust type onto the input it calls for.
fn kind_of(ty: &Type) -> Result<Kind, Error> {
    let Type::Path(path) = ty else {
        return Err(Error::new_spanned(
            ty,
            "BtcpaySettings supports String, bool and integer fields; use the builder API for \
             anything else",
        ));
    };

    let last = path
        .path
        .segments
        .last()
        .ok_or_else(|| Error::new_spanned(ty, "expected a type name"))?;

    match last.ident.to_string().as_str() {
        "String" => Ok(Kind::Text),
        "bool" => Ok(Kind::Toggle),
        "i8" | "i16" | "i32" | "i64" | "isize" | "u8" | "u16" | "u32" | "u64" | "usize" => {
            Ok(Kind::Number)
        }
        // Assumed to be a dropdown. The trait bound reports it if not, with a better message
        // than this macro could produce.
        _ => Ok(Kind::Choice(Box::new(ty.clone()))),
    }
}

/// Turns a field name into a readable label: `api_key` becomes `Api key`.
///
/// Only a default; anything user-facing should set `label` explicitly.
fn humanise(field: &str) -> String {
    let spaced = field.replace('_', " ");
    let mut chars = spaced.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
        None => spaced,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_field_name_becomes_a_readable_label() {
        assert_eq!(humanise("api_key"), "Api key");
        assert_eq!(humanise("greeting"), "Greeting");
        assert_eq!(humanise(""), "");
    }

    #[test]
    fn supported_types_map_to_inputs() {
        let text: Type = syn::parse_quote!(String);
        let toggle: Type = syn::parse_quote!(bool);
        let number: Type = syn::parse_quote!(u32);

        assert!(matches!(kind_of(&text), Ok(Kind::Text)));
        assert!(matches!(kind_of(&toggle), Ok(Kind::Toggle)));
        assert!(matches!(kind_of(&number), Ok(Kind::Number)));
    }

    #[test]
    fn an_unrecognised_type_is_treated_as_a_dropdown() {
        // Not an error here: the generated code calls the Choice trait, and the trait's
        // on_unimplemented note produces a better message than this macro could, including
        // for a type that was never meant to be a settings field.
        let ty: Type = syn::parse_quote!(HashMap<String, String>);

        assert!(matches!(kind_of(&ty), Ok(Kind::Choice(_))));
    }
}
