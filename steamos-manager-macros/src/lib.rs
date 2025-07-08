/*
 * Copyright © 2023 Collabora Ltd.
 * Copyright © 2024 Valve Software
 * Copyright © 2024 Igalia S.L.
 *
 * SPDX-License-Identifier: MIT
 */

use proc_macro::TokenStream;
use proc_macro2::{Group, Literal, TokenStream as TokenStream2, TokenTree};
use quote::{format_ident, quote, ToTokens};
use std::collections::HashMap;
use syn::parse::{self, Parse, ParseStream};
use syn::spanned::Spanned;
use syn::{
    self, parse_macro_input, Attribute, FnArg, GenericArgument, Ident, ImplItem, ItemImpl, Meta,
    PathArguments, ReturnType, Type,
};

#[derive(Debug)]
struct Interface {
    name: String,
    properties: Vec<Property>,
    methods: Vec<Method>,
}

#[derive(Debug)]
struct Method {
    name: Ident,
    args: Vec<Type>,
    ret: Option<Type>,
}

#[derive(Debug)]
struct Property {
    name: Ident,
    attr: Attribute,
    emits_changed: bool,
    ty: Type,
    setter: bool,
}

fn clean_return_type(ty: Type) -> Type {
    match ty {
        Type::Path(ref path) => {
            if let Some(tail) = path.path.segments.last() {
                if tail.ident == "Result" {
                    match &tail.arguments {
                        PathArguments::None => ty,
                        PathArguments::AngleBracketed(args) => match args.args.first() {
                            Some(GenericArgument::Type(ty)) => ty.clone(),
                            _ => todo!(),
                        },
                        PathArguments::Parenthesized(_) => todo!("parenthesized return type"),
                    }
                } else {
                    ty
                }
            } else {
                todo!("no tail");
            }
        }
        other => todo!("unimplemented return type {other:?}"),
    }
}

fn parse_kv_pairs(group: Group) -> parse::Result<HashMap<String, Literal>> {
    let mut tokens = group.stream().into_iter();
    let mut kv = HashMap::new();
    loop {
        let prop = match tokens.next() {
            Some(TokenTree::Ident(prop)) => prop,
            Some(TokenTree::Punct(punct)) if punct.as_char() == ',' => continue,
            Some(token) => {
                return Err(syn::Error::new(token.span(), "expected `,` or identifier"));
            }
            None => break,
        };
        let value = {
            match tokens.next() {
                Some(TokenTree::Punct(punct)) if punct.as_char() == '=' => (),
                Some(token) => {
                    return Err(syn::Error::new(token.span(), "expected `=`"));
                }
                None => {
                    return Err(syn::Error::new(group.span_close(), "expected `=`"));
                }
            }
            match tokens.next() {
                Some(TokenTree::Literal(lit)) => lit,
                Some(token) => {
                    return Err(syn::Error::new(token.span(), "expected string"));
                }
                None => {
                    return Err(syn::Error::new(group.span_close(), "expected string"));
                }
            }
        };
        let prop_str = prop.to_string();
        if kv.insert(prop_str, value).is_some() {
            return Err(syn::Error::new(
                prop.span(),
                format!("duplicate key \"{prop}\""),
            ));
        }
    }
    Ok(kv)
}

impl Parse for Interface {
    fn parse(input: ParseStream<'_>) -> parse::Result<Interface> {
        let iface_impl: ItemImpl = input.parse()?;
        let Type::Path(path) = *iface_impl.self_ty else {
            return Err(syn::Error::new(input.span(), "Invalid name identifier"));
        };
        let name = path.path.require_ident()?;
        let mut properties = Vec::new();
        let mut methods = Vec::new();
        for item in iface_impl.items {
            let ImplItem::Fn(fn_item) = item else {
                continue;
            };
            let mut prop_attr = None;
            let mut emits_changed = true;
            for attr in fn_item.attrs {
                let Meta::List(ref list) = attr.meta else {
                    continue;
                };
                if list.path.require_ident()? != "zbus" {
                    continue;
                }
                let mut tokens = list.tokens.clone().into_iter();
                let first = tokens.next();
                match first {
                    Some(TokenTree::Ident(ident)) if ident == "property" => {
                        prop_attr = Some(attr);
                        if let Some(TokenTree::Group(group)) = tokens.next() {
                            let kv = parse_kv_pairs(group)?;
                            match kv.get("emits_changed_signal") {
                                None => emits_changed = true,
                                Some(val) if val.to_string() == "true" => emits_changed = true,
                                _ => emits_changed = false,
                            }
                        }
                    }
                    Some(TokenTree::Ident(ident)) if ident == "signal" => {
                        todo!("signals not implemented")
                    }
                    other => todo!("unknown attribute {other:?}"),
                }
            }
            let sig = fn_item.sig;
            let name = sig.ident;
            let inputs = sig.inputs;
            if !matches!(inputs.first(), Some(FnArg::Receiver(_))) {
                return Err(syn::Error::new(
                    sig.paren_token.span.open(),
                    "expected `self`",
                ));
            }

            if let Some(attr) = prop_attr {
                let setter = name.to_string().starts_with("set_");
                let ty = if setter {
                    let mut ty = None;
                    emits_changed = false;
                    'input: for input in inputs.into_iter().skip(1) {
                        let span = input.span();
                        let FnArg::Typed(fty) = input else {
                            continue;
                        };
                        for attr in &fty.attrs {
                            let Meta::List(ref list) = attr.meta else {
                                continue;
                            };
                            let Some(ident) = list.path.get_ident() else {
                                continue;
                            };
                            if ident == "zbus" {
                                continue 'input;
                            }
                        }
                        if ty.is_some() {
                            return Err(syn::Error::new(span, "unexpected argument type"));
                        }
                        ty = Some(*fty.ty);
                    }
                    ty.unwrap()
                } else {
                    if inputs.len() != 1 {
                        return Err(syn::Error::new(
                            sig.paren_token.span.join(),
                            "expected 1 argument",
                        ));
                    }
                    let ReturnType::Type(_, ret) = sig.output else {
                        return Err(syn::Error::new(sig.fn_token.span, "expected return value"));
                    };
                    clean_return_type(*ret)
                };
                properties.push(Property {
                    name,
                    attr,
                    setter,
                    ty,
                    emits_changed,
                });
            } else {
                let ret = match sig.output {
                    ReturnType::Type(_, ret) => Some(clean_return_type(*ret)),
                    ReturnType::Default => None,
                };
                let args = inputs
                    .into_iter()
                    .skip(1)
                    .map(|arg| {
                        let FnArg::Typed(ty) = arg else {
                            panic!();
                        };
                        *ty.ty
                    })
                    .collect();
                methods.push(Method { name, args, ret });
            }
        }

        Ok(Interface {
            name: name.to_string(),
            methods,
            properties,
        })
    }
}

impl ToTokens for Interface {
    fn to_tokens(&self, stream: &mut TokenStream2) {
        let mut substream = TokenStream2::new();
        let mut signals = Vec::new();
        for prop in self.properties.iter() {
            prop.to_tokens(&mut substream);
            if prop.emits_changed {
                signals.push(format_ident!("{}_changed", prop.name.clone()));
            }
        }
        for method in self.methods.iter() {
            method.to_tokens(&mut substream);
        }

        let name = format_ident!("{}", self.name);
        let struct_name: Ident = format_ident!("{}Remote", self.name);
        let proxy_name: Ident = format_ident!("{}Proxy", self.name);

        let receivers: Vec<Ident> = signals
            .iter()
            .map(|name| format_ident!("receive_{name}"))
            .collect();

        stream.extend(quote! {
            impl #struct_name {
                #substream
            }

            struct #struct_name {
                proxy: #proxy_name<'static>,
                signal_task: JoinHandle<Result<()>>,
                interlock: Option<oneshot::Sender<()>>,
            }

            impl #struct_name {
                pub async fn new(
                    destination: &BusName<'static>,
                    path: ObjectPath<'static>,
                    connection: &Connection
                )
                -> fdo::Result<#struct_name> {
                    let proxy = #proxy_name::builder(connection)
                        .path(path)?
                        .destination(destination)?
                        .build()
                        .await?;
                    let (signal_task, interlock) = #struct_name::signal_task(proxy.clone(), connection.clone())
                        .await
                        .map_err(to_zbus_fdo_error)?;
                    Ok(#struct_name {
                        proxy,
                        signal_task,
                        interlock: Some(interlock),
                    })
                }

                fn remote(&self) -> &BusName<'_> {
                    self.proxy.inner().destination()
                }

                async fn signal_task(
                    proxy: #proxy_name<'static>,
                    connection: Connection
                ) -> Result<(JoinHandle<Result<()>>, oneshot::Sender<()>)> {
                    let (tx1, rx1) = oneshot::channel();
                    let (tx2, rx2) = oneshot::channel();
                    let handle = spawn(async move {
                        let object_server = connection.object_server();
                        let dbus_proxy = DBusProxy::new(&connection).await?;
                        let mut name_changed_receiver = dbus_proxy.receive_name_owner_changed().await?;
                        #(let mut #receivers = proxy.#receivers().await;)*
                        // This should never fail. If it does, something has gone very wrong.
                        tx1.send(()).unwrap();
                        rx2.await?;
                        let mut interface = object_server
                            .interface::<_, #struct_name>(MANAGER_PATH)
                            .await?;
                        let emitter = interface.signal_emitter();
                        loop {
                            tokio::select! {
                                Some(changed) = name_changed_receiver.next() => {
                                    match changed.args() {
                                        Ok(args) => {
                                            if args.name() != proxy.inner().destination() {
                                                continue;
                                            }
                                            if args.new_owner().is_none() {
                                                let manager = object_server
                                                    .interface::<_, RemoteInterface1>(MANAGER_PATH)
                                                    .await?;
                                                let emitter = manager.signal_emitter();
                                                manager
                                                    .get_mut()
                                                    .await
                                                    .unregister_interface_impl(
                                                        Self::name().as_str(),
                                                        None,
                                                        &connection,
                                                        emitter
                                                    )
                                                    .await?;
                                            }
                                        },
                                        Err(e) => error!("Error receiving signal: {e}"),
                                    }
                                },
                                #(Some(val) = #receivers.next() => {
                                    if let Err(e) = interface.get().await.#signals(&emitter).await {
                                        error!("Error receiving signal: {e}");
                                    };
                                },)*
                            }
                        }
                    });
                    rx1.await?;
                    Ok((handle, tx2))
                }
            }

            impl Drop for #struct_name {
                fn drop(&mut self) {
                    self.signal_task.abort();
                }
            }

            impl RemoteInterface for #name {
                type Remote = #struct_name;
            }
        });
    }
}

impl ToTokens for Method {
    fn to_tokens(&self, stream: &mut TokenStream2) {
        let name = &self.name;
        let args = &self.args;
        let ret = &self.ret;
        let arg_names: Vec<Ident> = (0..args.len()).map(|i| format_ident!("arg{i}")).collect();
        stream.extend(quote! {
            async fn #name(&self #(, #arg_names: #args)*) -> fdo::Result<#ret> {
                self.proxy.#name(#(#arg_names),*).await.map_err(zbus_to_zbus_fdo)
            }
        });
    }
}

impl ToTokens for Property {
    fn to_tokens(&self, stream: &mut TokenStream2) {
        let attr = &self.attr;
        let ty = &self.ty;
        let name = &self.name;
        if self.setter {
            stream.extend(quote! {
                #attr
                async fn #name(&self, arg: #ty) -> zbus::Result<()> {
                    self.proxy.#name(arg).await
                }
            });
        } else {
            stream.extend(quote! {
                #attr
                async fn #name(&self) -> fdo::Result<#ty> {
                    Ok(self.proxy.#name().await?)
                }
            });
        }
    }
}

#[proc_macro_attribute]
pub fn remote(attr: TokenStream, input: TokenStream) -> TokenStream {
    let attr: TokenStream2 = attr.into();
    let imp: TokenStream2 = input.clone().into();
    let iface = parse_macro_input!(input as Interface);

    let out = quote! {
        #[interface(#attr)]
        #iface

        #[interface(#attr)]
        #imp
    };
    out.into()
}
