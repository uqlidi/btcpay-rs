//! `#[derive(BtcpayChoice)]`: an enum becomes a dropdown.

use proc_macro2::TokenStream;
use quote::quote;
use syn::{Data, DeriveInput, Error, Fields, LitStr};

/// Generates a [`Choice`](btcpay_plugin::Choice) implementation for a unit-only enum.
pub fn derive(input: DeriveInput) -> Result<TokenStream, Error> {
    let name = &input.ident;

    let Data::Enum(data) = &input.data else {
        return Err(Error::new_spanned(
            &input.ident,
            "BtcpayChoice works on an enum: a dropdown is a fixed set of options",
        ));
    };

    if data.variants.is_empty() {
        return Err(Error::new_spanned(
            &input.ident,
            "BtcpayChoice needs at least one variant, or the dropdown would be empty",
        ));
    }

    let variants: Vec<Variant> = data
        .variants
        .iter()
        .map(Variant::parse)
        .collect::<Result<_, _>>()?;

    // Two variants sharing a stored value would make one of them unreachable, and which one
    // would depend on declaration order.
    for (index, variant) in variants.iter().enumerate() {
        if let Some(clash) = variants[..index].iter().find(|v| v.value == variant.value) {
            return Err(Error::new_spanned(
                &variant.ident,
                format!(
                    "value `{}` is already used by `{}`; give one of them an explicit \
                     #[choice(value = \"...\")]",
                    variant.value, clash.ident
                ),
            ));
        }
    }

    let choices = variants.iter().map(|v| {
        let value = &v.value;
        let label = &v.label;
        quote! {
            (
                ::std::string::String::from(#value),
                ::std::string::String::from(#label),
            )
        }
    });

    let to_value = variants.iter().map(|v| {
        let ident = &v.ident;
        let value = &v.value;
        quote! { Self::#ident => ::std::string::String::from(#value) }
    });

    let from_value = variants.iter().map(|v| {
        let ident = &v.ident;
        let value = &v.value;
        quote! { #value => ::core::option::Option::Some(Self::#ident) }
    });

    Ok(quote! {
        impl ::btcpay_plugin::Choice for #name {
            fn choices() -> ::std::vec::Vec<(::std::string::String, ::std::string::String)> {
                ::std::vec![#(#choices),*]
            }

            fn choice_value(&self) -> ::std::string::String {
                match self {
                    #(#to_value),*
                }
            }

            fn from_choice_value(value: &str) -> ::core::option::Option<Self> {
                match value.trim() {
                    #(#from_value),*,
                    _ => ::core::option::Option::None,
                }
            }
        }
    })
}

struct Variant {
    ident: syn::Ident,
    value: String,
    label: String,
}

impl Variant {
    fn parse(variant: &syn::Variant) -> Result<Self, Error> {
        if !matches!(variant.fields, Fields::Unit) {
            return Err(Error::new_spanned(
                &variant.fields,
                "BtcpayChoice variants cannot carry data: a dropdown option is just a value",
            ));
        }

        let mut parsed = Self {
            value: snake_case(&variant.ident.to_string()),
            label: humanise(&variant.ident.to_string()),
            ident: variant.ident.clone(),
        };

        for attr in &variant.attrs {
            if !attr.path().is_ident("choice") {
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
                    "value" => parsed.value = meta.value()?.parse::<LitStr>()?.value(),
                    other => {
                        return Err(meta.error(format!(
                            "unknown choice option `{other}`; expected label or value"
                        )))
                    }
                }
                Ok(())
            })?;
        }

        if parsed.value.trim().is_empty() {
            return Err(Error::new_spanned(
                &parsed.ident,
                "a choice value cannot be empty: it is what gets stored",
            ));
        }

        Ok(parsed)
    }
}

/// `CoreRpc` becomes `core_rpc`, which is what gets stored.
fn snake_case(variant: &str) -> String {
    let mut out = String::with_capacity(variant.len() + 4);
    for (index, ch) in variant.char_indices() {
        if ch.is_uppercase() {
            if index > 0 {
                out.push('_');
            }
            out.extend(ch.to_lowercase());
        } else {
            out.push(ch);
        }
    }
    out
}

/// `CoreRpc` becomes `Core rpc`. Only a default; anything user-facing should set a label.
fn humanise(variant: &str) -> String {
    let snake = snake_case(variant).replace('_', " ");
    let mut chars = snake.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
        None => snake,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_variant_name_becomes_a_stored_value() {
        assert_eq!(snake_case("Main"), "main");
        assert_eq!(snake_case("CoreRpc"), "core_rpc");
        assert_eq!(snake_case("ExpiredPaidPartial"), "expired_paid_partial");
    }

    #[test]
    fn a_variant_name_becomes_a_readable_label() {
        assert_eq!(humanise("Main"), "Main");
        assert_eq!(humanise("CoreRpc"), "Core rpc");
    }
}
