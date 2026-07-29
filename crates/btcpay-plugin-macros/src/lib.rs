//! Procedural macros for `btcpay-plugin`. Not intended to be used directly; depend on
//! `btcpay-plugin` and use `#[btcpay_plugin::plugin]`.

use proc_macro::TokenStream;
use quote::quote;
use syn::punctuated::Punctuated;
use syn::{
    parse_macro_input, Error, Expr, ExprLit, ExprPath, ImplItem, ItemImpl, Lit, Meta, Token, Type,
};

/// Registers a [`Plugin`](trait@btcpay_plugin::Plugin) implementation as *the* plugin
/// exported by this library, and optionally writes its `metadata()` for you.
///
/// # Generating metadata
///
/// Pass `identifier` and the macro implements [`Plugin::metadata`], taking the rest from
/// `Cargo.toml` so version and description cannot drift from the package that produced them:
///
/// ```ignore
/// #[derive(Default)]
/// struct HelloPlugin;
///
/// #[btcpay_plugin::plugin(identifier = "BTCPayServer.Plugins.Hello")]
/// impl Plugin for HelloPlugin {}
/// ```
///
/// | argument      | default                                  |
/// |---------------|------------------------------------------|
/// | `identifier`  | *required to generate metadata*          |
/// | `name`        | `CARGO_PKG_NAME`                         |
/// | `version`     | `CARGO_PKG_VERSION`                      |
/// | `description` | `CARGO_PKG_DESCRIPTION`                  |
/// | `btcpay`      | `">=2.4.0"` (the minimum BTCPay version) |
/// | `factory`     | `Default::default()`                     |
///
/// Omit `identifier` to write `metadata()` by hand. That is necessary when a plugin depends on
/// *other* plugins, or computes its identity at runtime.
///
/// # Construction
///
/// The annotated type must implement [`Default`], unless you point at a constructor:
///
/// ```ignore
/// #[btcpay_plugin::plugin(identifier = "Acme.Swaps", factory = SwapPlugin::new)]
/// impl Plugin for SwapPlugin {}
/// ```
///
/// where `SwapPlugin::new` is any `fn() -> SwapPlugin`.
///
/// Exactly one type per cdylib may be annotated; a second registration panics at load with a
/// clear message rather than silently winning or losing.
#[proc_macro_attribute]
pub fn plugin(attr: TokenStream, item: TokenStream) -> TokenStream {
    let args = match Args::parse(attr) {
        Ok(args) => args,
        Err(e) => return e.to_compile_error().into(),
    };

    let input = parse_macro_input!(item as ItemImpl);

    // Reject `impl MyType`: registration would compile, but the type would not be usable as
    // a plugin and the error would surface at load time instead of here.
    if input.trait_.is_none() {
        return Error::new_spanned(
            &input.self_ty,
            "#[btcpay_plugin::plugin] must be applied to an `impl Plugin for T` block, \
             not an inherent impl",
        )
        .to_compile_error()
        .into();
    }

    let self_ty: &Type = &input.self_ty;

    let construct = match &args.factory {
        Some(path) => quote! { #path() },
        None => quote! { <#self_ty as ::core::default::Default>::default() },
    };

    // Build metadata() only when asked. Writing it by hand stays fully supported.
    let mut input = input;
    if let Some(identifier) = &args.identifier {
        let has_manual = input
            .items
            .iter()
            .any(|item| matches!(item, ImplItem::Fn(f) if f.sig.ident == "metadata"));
        if has_manual {
            return Error::new_spanned(
                identifier,
                "this impl already defines `metadata()`, so `identifier = ...` would generate \
                 a duplicate; remove one of them",
            )
            .to_compile_error()
            .into();
        }

        let name = match &args.name {
            Some(lit) => quote! { #lit },
            None => quote! { ::core::env!("CARGO_PKG_NAME") },
        };
        let version = match &args.version {
            Some(lit) => quote! { #lit },
            None => quote! { ::core::env!("CARGO_PKG_VERSION") },
        };
        let description = match &args.description {
            Some(lit) => quote! { #lit },
            None => quote! { ::core::env!("CARGO_PKG_DESCRIPTION") },
        };
        let btcpay = match &args.btcpay {
            Some(lit) => quote! { #lit },
            None => quote! { ">=2.4.0" },
        };

        input.items.push(syn::parse_quote! {
            fn metadata(&self) -> ::btcpay_plugin::PluginMetadata {
                ::btcpay_plugin::PluginMetadata {
                    identifier: ::std::string::ToString::to_string(#identifier),
                    name: ::std::string::ToString::to_string(#name),
                    version: ::std::string::ToString::to_string(#version),
                    description: ::std::string::ToString::to_string(#description),
                    dependencies: ::std::vec![
                        ::btcpay_plugin::PluginDependency::btcpay_server(#btcpay)
                    ],
                }
            }
        });
    }

    quote! {
        #input

        // Registered at library load, before the host calls any exported function.
        #[::btcpay_plugin::__private::ctor]
        fn __btcpay_rs_register_plugin() {
            ::btcpay_plugin::register_plugin(|| {
                ::std::sync::Arc::new(#construct) as ::std::sync::Arc<dyn ::btcpay_plugin::Plugin>
            });
        }
    }
    .into()
}

/// Parsed `#[plugin(...)]` arguments.
#[derive(Default)]
struct Args {
    identifier: Option<ExprLit>,
    name: Option<ExprLit>,
    version: Option<ExprLit>,
    description: Option<ExprLit>,
    btcpay: Option<ExprLit>,
    factory: Option<ExprPath>,
}

impl Args {
    fn parse(attr: TokenStream) -> syn::Result<Self> {
        let mut args = Args::default();
        if attr.is_empty() {
            return Ok(args);
        }

        let metas =
            syn::parse::Parser::parse(Punctuated::<Meta, Token![,]>::parse_terminated, attr)?;

        for meta in metas {
            let Meta::NameValue(nv) = meta else {
                return Err(Error::new_spanned(
                    &meta,
                    "expected `key = value`, e.g. `identifier = \"Acme.Plugins.Thing\"`",
                ));
            };
            let key = nv
                .path
                .get_ident()
                .ok_or_else(|| Error::new_spanned(&nv.path, "expected a bare argument name"))?
                .to_string();

            match key.as_str() {
                "factory" => {
                    match nv.value {
                        Expr::Path(path) => args.factory = Some(path),
                        other => return Err(Error::new_spanned(
                            other,
                            "`factory` takes a path to a function, e.g. `factory = MyPlugin::new`",
                        )),
                    }
                }
                "identifier" | "name" | "version" | "description" | "btcpay" => {
                    let lit = string_literal(nv.value, &key)?;
                    match key.as_str() {
                        "identifier" => args.identifier = Some(lit),
                        "name" => args.name = Some(lit),
                        "version" => args.version = Some(lit),
                        "description" => args.description = Some(lit),
                        "btcpay" => args.btcpay = Some(lit),
                        _ => unreachable!(),
                    }
                }
                other => {
                    return Err(Error::new_spanned(
                        &nv.path,
                        format!(
                            "unknown argument `{other}`; expected one of: identifier, name, \
                             version, description, btcpay, factory"
                        ),
                    ))
                }
            }
        }

        // `name`/`version`/... without `identifier` would be silently ignored, which reads as
        // "my metadata is being generated" when it is not. Catch it here.
        if args.identifier.is_none() {
            for (present, key) in [
                (args.name.is_some(), "name"),
                (args.version.is_some(), "version"),
                (args.description.is_some(), "description"),
                (args.btcpay.is_some(), "btcpay"),
            ] {
                if present {
                    return Err(Error::new(
                        proc_macro2::Span::call_site(),
                        format!(
                            "`{key}` only applies when metadata is generated; add \
                             `identifier = \"...\"`, or write `metadata()` by hand and drop `{key}`"
                        ),
                    ));
                }
            }
        }

        Ok(args)
    }
}

fn string_literal(value: Expr, key: &str) -> syn::Result<ExprLit> {
    match value {
        Expr::Lit(
            lit @ ExprLit {
                lit: Lit::Str(_), ..
            },
        ) => Ok(lit),
        other => Err(Error::new_spanned(
            other,
            format!("`{key}` takes a string literal"),
        )),
    }
}
