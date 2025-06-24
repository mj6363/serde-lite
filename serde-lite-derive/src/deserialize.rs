use std::str::FromStr;

use proc_macro2::{Literal, Span, TokenStream};
use quote::quote;
use syn::{Attribute, Data, DataEnum, Fields, FieldsNamed, FieldsUnnamed, Ident, Variant};
use synstructure::AddBounds;

use crate::attributes;

/// Expand the derive Deserialize.
// TODO: use features of synstructure more extensively.
pub(crate) fn derive_deserialize(mut s: synstructure::Structure) -> TokenStream {
    let deserialize = if let Some(delegated_type) = attributes::get_from(&s.ast().attrs) {
        expand_delegation(&delegated_type)
    } else {
        match s.ast().data.clone() {
            Data::Struct(data) => expand_for_struct(data.fields),
            Data::Enum(data) => expand_for_enum(data, &s.ast().attrs),
            Data::Union(_) => panic!("derive Deserialize is not supported for union types"),
        }
    };

    // Generate StreamDeserialize implementation
    let stream_deserialize = if let Some(delegated_type) = attributes::get_from(&s.ast().attrs) {
        expand_stream_delegation(&delegated_type)
    } else {
        match s.ast().data.clone() {
            Data::Struct(data) => expand_stream_for_struct(data.fields),
            Data::Enum(data) => expand_stream_for_enum(data, &s.ast().attrs),
            Data::Union(_) => panic!("derive Deserialize is not supported for union types"),
        }
    };

    s.add_bounds(AddBounds::Generics);

    let deserialize_impl = s.bound_impl(
        quote!(serde_lite::Deserialize),
        quote! {
            #[allow(unused_variables)]
            fn deserialize(__val: &serde_lite::Intermediate) -> Result<Self, serde_lite::Error> {
                #deserialize
            }
        },
    );

    let stream_deserialize_impl = s.bound_impl(
        quote!(serde_lite::StreamDeserialize),
        quote! {
            #[allow(unused_variables)]
            fn deserialize_from_tokens<__T: serde_lite::Tokenizer>(__tokenizer: &mut __T) -> Result<Self, serde_lite::Error>
            where
                __T::Error: Into<serde_lite::Error>,
            {
                #stream_deserialize
            }
        },
    );

    quote! {
        #deserialize_impl
        #stream_deserialize_impl
    }
}

/// Expand Deserialize as delegation.
fn expand_delegation(delegated_type: &str) -> TokenStream {
    let ty: TokenStream = delegated_type.parse().unwrap();

    quote! {
        <#ty as serde_lite::Deserialize>::deserialize(__val).map(|v| Self::from(v))
    }
}

/// Expand Deserialize for a given struct.
fn expand_for_struct(fields: Fields) -> TokenStream {
    match fields {
        Fields::Named(fields) => expand_struct_named_fields(fields),
        Fields::Unnamed(fields) => expand_struct_unnamed_fields(fields),
        Fields::Unit => quote! {
            Ok(Self)
        },
    }
}

/// Expand Deserialize for a given enum.
fn expand_for_enum(data: DataEnum, attrs: &[Attribute]) -> TokenStream {
    if data.variants.is_empty() {
        panic!("enum with no variants cannot be deserialized")
    }

    if let Some(tag) = attributes::get_enum_tag(attrs) {
        let content = attributes::get_enum_content(attrs);

        expand_internally_tagged_enum(&tag, content.as_deref(), data)
    } else {
        expand_externally_tagged_enum(data)
    }
}

/// Expand Deserialize for named struct fields.
fn expand_struct_named_fields(fields: FieldsNamed) -> TokenStream {
    let (deserialize, constructor) = deserialize_named_fields(&fields);

    quote! {
        #deserialize

        Ok(Self {
            #constructor
        })
    }
}

/// Expand Deserialize for unnamed struct fields.
fn expand_struct_unnamed_fields(fields: FieldsUnnamed) -> TokenStream {
    let (deserialize, constructor) = deserialize_unnamed_fields(&fields);

    quote! {
        #deserialize

        Ok(Self(#constructor))
    }
}

/// Expand Deserialize for an internally tagged enum or an adjacently tagged
/// enum.
fn expand_internally_tagged_enum(
    tag_field: &str,
    content_field: Option<&str>,
    data: DataEnum,
) -> TokenStream {
    let mut deserialize = TokenStream::new();

    for variant in data.variants.into_iter() {
        let sname = attributes::get_variant_name(&variant);
        let lname = Literal::string(&sname);
        let constructor = if content_field.is_some() {
            // This is a bit counter-intuitive. It means that the enum content
            // is in a sub-field and we don't know yet if the field exists.
            // Therefore, we have to use the construct_enum_variant function
            // here which will check if the field exists.
            construct_enum_variant(&variant, content_field)
        } else {
            // Here the enum content is a part of the currently deserialized
            // object, so we don't need to check anything.
            construct_enum_variant_with_content(&variant)
        };

        deserialize.extend(quote! {
            #lname => { #constructor }
        });
    }

    let content = if let Some(content) = content_field {
        let lcontent = Literal::string(content);

        quote! {
            let __content = __obj.get(#lcontent);
        }
    } else {
        quote! {
            let __content = __val;
        }
    };

    let ltag = Literal::string(tag_field);

    quote! {
        let __obj = __val.as_map().ok_or_else(|| serde_lite::Error::invalid_value_static("object"))?;

        let __variant = __obj
            .get(#ltag)
            .map(|v| v.as_str())
            .ok_or_else(|| serde_lite::Error::MissingField)
            .and_then(|v| v.ok_or_else(|| serde_lite::Error::invalid_value_static("enum variant name")))
            .map_err(|err| serde_lite::Error::from(
                serde_lite::NamedFieldError::new_static(#ltag, err)
            ))?;

        #content

        match __variant {
            #deserialize
            _ => Err(serde_lite::Error::UnknownEnumVariant),
        }
    }
}

/// Expand Deserialize for an externally tagged enum.
fn expand_externally_tagged_enum(data: DataEnum) -> TokenStream {
    let mut plain = TokenStream::new();
    let mut with_content = TokenStream::new();

    for (index, variant) in data.variants.into_iter().enumerate() {
        let sname = attributes::get_variant_name(&variant);
        let lname = Literal::string(&sname);
        let constructor_with_content = construct_enum_variant_with_content(&variant);
        let constructor_without_content = construct_enum_variant_without_content(&variant, None);

        plain.extend(quote! {
            #lname => { #constructor_without_content }
        });

        if index == 0 {
            with_content.extend(quote! {
                if let Some(__content) = __obj.get(#lname) {
                    #constructor_with_content
                }
            })
        } else {
            with_content.extend(quote! {
                else if let Some(__content) = __obj.get(#lname) {
                    #constructor_with_content
                }
            })
        }
    }

    quote! {
        if let Some(__obj) = __val.as_map() {
            #with_content
            else {
                Err(serde_lite::Error::UnknownEnumVariant)
            }
        } else if let Some(__variant) = __val.as_str() {
            match __variant {
                #plain
                _ => Err(serde_lite::Error::UnknownEnumVariant),
            }
        } else {
            Err(serde_lite::Error::invalid_value_static("enum variant"))
        }
    }
}

/// Generate code for constructing a given enum variant.
fn construct_enum_variant(variant: &Variant, content_field: Option<&str>) -> TokenStream {
    let with_content = construct_enum_variant_with_content(variant);
    let without_content = construct_enum_variant_without_content(variant, content_field);

    quote! {
        if let Some(__content) = __content {
            #with_content
        } else {
            #without_content
        }
    }
}

/// Generate code for constructing a given enum variant and use the available
/// variant content.
fn construct_enum_variant_with_content(variant: &Variant) -> TokenStream {
    match &variant.fields {
        Fields::Named(fields) => construct_struct_enum_variant(variant, fields),
        Fields::Unnamed(fields) => construct_tuple_enum_variant(variant, fields),
        Fields::Unit => construct_unit_enum_variant(variant),
    }
}

/// Generate code for constructing a given enum variant without any content.
fn construct_enum_variant_without_content(
    variant: &Variant,
    content_field: Option<&str>,
) -> TokenStream {
    match &variant.fields {
        Fields::Named(fields) if fields.named.is_empty() => {
            return construct_struct_enum_variant(variant, fields);
        }
        Fields::Unnamed(fields) if fields.unnamed.is_empty() => {
            return construct_tuple_enum_variant(variant, fields);
        }
        Fields::Unit => return construct_unit_enum_variant(variant),
        _ => (),
    }

    if let Some(content) = content_field {
        let lcontent = Literal::string(content);

        quote! {
            Err(serde_lite::Error::from(
                serde_lite::NamedFieldError::new_static(#lcontent, serde_lite::Error::MissingField)
            ))
        }
    } else {
        quote! {
            Err(serde_lite::Error::MissingEnumVariantContent)
        }
    }
}

/// Generate code for constructing a given struct-like enum variant.
fn construct_struct_enum_variant(variant: &Variant, fields: &FieldsNamed) -> TokenStream {
    let mut init = TokenStream::new();

    if !fields.named.is_empty() {
        init.extend(quote! {
            let __val = __content;
        });
    }

    let (deserialize, constructor) = deserialize_named_fields(fields);

    let ident = &variant.ident;

    quote! {
        #init
        #deserialize

        Ok(Self::#ident {
            #constructor
        })
    }
}

/// Generate code for constructing a given tuple-like enum variant.
fn construct_tuple_enum_variant(variant: &Variant, fields: &FieldsUnnamed) -> TokenStream {
    let mut init = TokenStream::new();

    if !fields.unnamed.is_empty() {
        init.extend(quote! {
            let __val = __content;
        });
    }

    let (deserialize, constructor) = deserialize_unnamed_fields(fields);

    let ident = &variant.ident;

    quote! {
        #init
        #deserialize

        Ok(Self::#ident(#constructor))
    }
}

/// Generate code for constructing a given enum variant.
fn construct_unit_enum_variant(variant: &Variant) -> TokenStream {
    let ident = &variant.ident;

    quote! {
        Ok(Self::#ident)
    }
}

/// Generate code for deserializing given named fields.
fn deserialize_named_fields(fields: &FieldsNamed) -> (TokenStream, TokenStream) {
    let mut deserialize = TokenStream::new();
    let mut constructor = TokenStream::new();

    if !fields.named.is_empty() {
        deserialize.extend(quote! {
            let __obj = __val
                .as_map()
                .ok_or_else(|| serde_lite::Error::invalid_value_static("object"))?;

            let mut __field_errors = serde_lite::ErrorList::new();
        });
    }

    for field in &fields.named {
        let name = field.ident.as_ref().unwrap();
        let ty = &field.ty;
        let sname = attributes::get_field_name(field);
        let lname = Literal::string(&sname);
        let deserializer = if let Some(path) = attributes::get_field_deserializer(field) {
            TokenStream::from_str(&path)
                .expect("invalid path given for the deserialize_with attribute")
        } else if let Some(path) = attributes::get_field_with(field) {
            let path =
                TokenStream::from_str(&path).expect("invalid path given for the with attribute");

            quote! { #path::deserialize_lite }
        } else {
            quote! {
                <#ty as serde_lite::Deserialize>::deserialize
            }
        };

        let skip = attributes::has_flag(&field.attrs, "skip")
            || attributes::has_flag(&field.attrs, "skip_deserializing");

        if skip {
            deserialize.extend(quote! {
                let #name: #ty = Default::default();
            });
        } else if attributes::has_flag(&field.attrs, "flatten") {
            deserialize.extend(quote! {
                let #name = match #deserializer(__val) {
                    Ok(v) => Some(v),
                    Err(serde_lite::Error::NamedFieldErrors(errors)) => {
                        __field_errors.append(errors);
                        None
                    }
                    Err(err) => return Err(err),
                };
            });
        } else if attributes::has_flag(&field.attrs, "default") {
            deserialize.extend(quote! {
                let #name = __obj
                    .get(#lname)
                    .map(#deserializer)
                    .unwrap_or_else(|| Ok(Default::default()))
                    .map_err(|err| __field_errors.push(serde_lite::NamedFieldError::new_static(#lname, err)))
                    .ok();
            });
        } else if let Some(func) = attributes::get_field_default(field) {
            let func: TokenStream = func.parse().unwrap();

            deserialize.extend(quote! {
                let #name = __obj
                    .get(#lname)
                    .map(#deserializer)
                    .unwrap_or_else(|| Ok(#func()))
                    .map_err(|err| __field_errors.push(serde_lite::NamedFieldError::new_static(#lname, err)))
                    .ok();
            });
        } else {
            deserialize.extend(quote! {
                let #name = __obj
                    .get(#lname)
                    .map(#deserializer)
                    .unwrap_or_else(|| Err(serde_lite::Error::MissingField))
                    .map_err(|err| __field_errors.push(serde_lite::NamedFieldError::new_static(#lname, err)))
                    .ok();
            });
        }

        if skip {
            constructor.extend(quote! {
                #name,
            });
        } else {
            constructor.extend(quote! {
                #name: unsafe { #name.unwrap_unchecked() },
            });
        }
    }

    if !fields.named.is_empty() {
        deserialize.extend(quote! {
            if !__field_errors.is_empty() {
                return Err(serde_lite::Error::NamedFieldErrors(__field_errors));
            }
        });
    }

    (deserialize, constructor)
}

// StreamDeserialize implementation functions

/// Expand StreamDeserialize as delegation.
fn expand_stream_delegation(delegated_type: &str) -> TokenStream {
    let ty: TokenStream = delegated_type.parse().unwrap();

    quote! {
        <#ty as serde_lite::StreamDeserialize>::deserialize_from_tokens(__tokenizer).map(|v| Self::from(v))
    }
}

/// Expand StreamDeserialize for a given struct.
fn expand_stream_for_struct(fields: Fields) -> TokenStream {
    match fields {
        Fields::Named(fields) => expand_stream_struct_named_fields(fields),
        Fields::Unnamed(fields) => expand_stream_struct_unnamed_fields(fields),
        Fields::Unit => quote! {
            Ok(Self)
        },
    }
}

/// Expand StreamDeserialize for a given enum.
fn expand_stream_for_enum(data: DataEnum, attrs: &[Attribute]) -> TokenStream {
    if data.variants.is_empty() {
        panic!("enum with no variants cannot be deserialized")
    }

    // For now, only support externally tagged enums in streaming
    if attributes::get_enum_tag(attrs).is_some() {
        panic!("internally tagged enums are not yet supported for streaming deserialization")
    }

    expand_stream_externally_tagged_enum(data)
}

/// Expand StreamDeserialize for named struct fields.
fn expand_stream_struct_named_fields(fields: FieldsNamed) -> TokenStream {
    let (deserialize, constructor) = stream_deserialize_named_fields(&fields);

    quote! {
        #deserialize

        Ok(Self {
            #constructor
        })
    }
}

/// Expand StreamDeserialize for unnamed struct fields.
fn expand_stream_struct_unnamed_fields(fields: FieldsUnnamed) -> TokenStream {
    let (deserialize, constructor) = stream_deserialize_unnamed_fields(&fields);

    quote! {
        #deserialize

        Ok(Self(#constructor))
    }
}

/// Expand StreamDeserialize for an externally tagged enum.
fn expand_stream_externally_tagged_enum(data: DataEnum) -> TokenStream {
    let mut plain = TokenStream::new();
    let mut with_content = TokenStream::new();

    for (index, variant) in data.variants.into_iter().enumerate() {
        let sname = attributes::get_variant_name(&variant);
        let lname = Literal::string(&sname);
        let constructor_with_content = stream_construct_enum_variant_with_content(&variant);
        let constructor_without_content = stream_construct_enum_variant_without_content(&variant);

        plain.extend(quote! {
            #lname => { #constructor_without_content }
        });

        if index == 0 {
            with_content.extend(quote! {
                if let Some(__content) = __obj.get(#lname) {
                    #constructor_with_content
                }
            })
        } else {
            with_content.extend(quote! {
                else if let Some(__content) = __obj.get(#lname) {
                    #constructor_with_content
                }
            })
        }
    }

    quote! {
        match __tokenizer.next_token().map_err(|e| e.into())? {
            serde_lite::Token::String(__variant) => {
                match __variant.as_str() {
                    #plain
                    _ => Err(serde_lite::Error::UnknownEnumVariant),
                }
            }
            serde_lite::Token::ObjectStart => {
                let __key = __tokenizer.expect_key().map_err(|e| e.into())?;
                match __key.as_str() {
                    #plain
                    _ => Err(serde_lite::Error::UnknownEnumVariant),
                }
            }
            __token => Err(serde_lite::Error::invalid_value(format!("expected enum variant, found {}", __token))),
        }
    }
}

/// Generate code for constructing a given enum variant with content for streaming.
fn stream_construct_enum_variant_with_content(variant: &Variant) -> TokenStream {
    match &variant.fields {
        Fields::Named(fields) => stream_construct_struct_enum_variant(variant, fields),
        Fields::Unnamed(fields) => stream_construct_tuple_enum_variant(variant, fields),
        Fields::Unit => stream_construct_unit_enum_variant(variant),
    }
}

/// Generate code for constructing a given enum variant without content for streaming.
fn stream_construct_enum_variant_without_content(variant: &Variant) -> TokenStream {
    match &variant.fields {
        Fields::Named(fields) if fields.named.is_empty() => {
            stream_construct_struct_enum_variant(variant, fields)
        }
        Fields::Unnamed(fields) if fields.unnamed.is_empty() => {
            stream_construct_tuple_enum_variant(variant, fields)
        }
        Fields::Unit => stream_construct_unit_enum_variant(variant),
        _ => {
            quote! {
                Err(serde_lite::Error::MissingEnumVariantContent)
            }
        }
    }
}

/// Generate code for constructing a given struct-like enum variant for streaming.
fn stream_construct_struct_enum_variant(variant: &Variant, fields: &FieldsNamed) -> TokenStream {
    let (deserialize, constructor) = stream_deserialize_named_fields(fields);
    let ident = &variant.ident;

    quote! {
        #deserialize

        Ok(Self::#ident {
            #constructor
        })
    }
}

/// Generate code for constructing a given tuple-like enum variant for streaming.
fn stream_construct_tuple_enum_variant(variant: &Variant, fields: &FieldsUnnamed) -> TokenStream {
    let (deserialize, constructor) = stream_deserialize_unnamed_fields(fields);
    let ident = &variant.ident;

    quote! {
        #deserialize

        Ok(Self::#ident(#constructor))
    }
}

/// Generate code for constructing a given unit enum variant for streaming.
fn stream_construct_unit_enum_variant(variant: &Variant) -> TokenStream {
    let ident = &variant.ident;

    quote! {
        Ok(Self::#ident)
    }
}

/// Generate code for deserializing given named fields for streaming.
fn stream_deserialize_named_fields(fields: &FieldsNamed) -> (TokenStream, TokenStream) {
    let mut deserialize = TokenStream::new();
    let mut constructor = TokenStream::new();

    if !fields.named.is_empty() {
        deserialize.extend(quote! {
            // Expect object start
            match __tokenizer.next_token().map_err(|e| e.into())? {
                serde_lite::Token::ObjectStart => {}
                __token => return Err(serde_lite::Error::invalid_value(format!("expected object, found {}", __token))),
            }
        });

        // Generate field variables
        for field in &fields.named {
            let name = field.ident.as_ref().unwrap();
            let ty = &field.ty;
            let skip = attributes::has_flag(&field.attrs, "skip")
                || attributes::has_flag(&field.attrs, "skip_deserializing");

            if skip {
                deserialize.extend(quote! {
                    let #name: #ty = Default::default();
                });
            } else {
                deserialize.extend(quote! {
                    let mut #name: Option<#ty> = None;
                });
            }
        }

        // Build field matching arms
        let mut field_arms = TokenStream::new();
        for field in &fields.named {
            let name = field.ident.as_ref().unwrap();
            let ty = &field.ty;
            let sname = attributes::get_field_name(field);
            let lname = Literal::string(&sname);
            let skip = attributes::has_flag(&field.attrs, "skip")
                || attributes::has_flag(&field.attrs, "skip_deserializing");

            if !skip {
                field_arms.extend(quote! {
                    #lname => {
                        #name = Some(<#ty as serde_lite::StreamDeserialize>::deserialize_from_tokens(__tokenizer)?);
                    }
                });
            }
        }

        deserialize.extend(quote! {
            // Read object fields
            loop {
                match __tokenizer.next_token().map_err(|e| e.into())? {
                    serde_lite::Token::ObjectEnd => break,
                    serde_lite::Token::String(__key) => {
                        match __key.as_str() {
                            #field_arms
                            _ => {
                                // Skip unknown field
                                serde_lite::skip_value(__tokenizer)?;
                            }
                        }
                    }
                    __token => return Err(serde_lite::Error::invalid_value(format!("expected object key, found {}", __token))),
                }
            }
        });
    }

    // Generate constructor
    for field in &fields.named {
        let name = field.ident.as_ref().unwrap();
        let skip = attributes::has_flag(&field.attrs, "skip")
            || attributes::has_flag(&field.attrs, "skip_deserializing");

        if skip {
            constructor.extend(quote! {
                #name,
            });
        } else if attributes::has_flag(&field.attrs, "default") {
            constructor.extend(quote! {
                #name: #name.unwrap_or_default(),
            });
        } else {
            constructor.extend(quote! {
                #name: #name.ok_or(serde_lite::Error::MissingField)?,
            });
        }
    }

    (deserialize, constructor)
}

/// Generate code for deserializing given unnamed fields for streaming.
fn stream_deserialize_unnamed_fields(fields: &FieldsUnnamed) -> (TokenStream, TokenStream) {
    match fields.unnamed.len() {
        0 => stream_deserialize_unnamed_fields_0(),
        1 => stream_deserialize_unnamed_fields_1(fields),
        _ => stream_deserialize_unnamed_fields_n(fields),
    }
}

/// Generate code for deserializing given unnamed fields where the actual
/// number of fields is zero (e.g. zero-length tuple struct) for streaming.
fn stream_deserialize_unnamed_fields_0() -> (TokenStream, TokenStream) {
    let deserialize = TokenStream::new();
    let constructor = TokenStream::new();

    (deserialize, constructor)
}

/// Generate code for deserializing given unnamed fields where the actual
/// number of fields is one (e.g. single-element tuple struct) for streaming.
fn stream_deserialize_unnamed_fields_1(fields: &FieldsUnnamed) -> (TokenStream, TokenStream) {
    let mut deserialize = TokenStream::new();
    let mut constructor = TokenStream::new();

    let field = &fields.unnamed[0];
    let ty = &field.ty;
    let name = Ident::new("f0", Span::call_site());

    deserialize.extend(quote! {
        let #name = <#ty as serde_lite::StreamDeserialize>::deserialize_from_tokens(__tokenizer)?;
    });

    constructor.extend(quote! {
        #name,
    });

    (deserialize, constructor)
}

/// Generate code for deserializing given unnamed fields where the actual
/// number of fields is greater than one (e.g. multiple-element tuple struct) for streaming.
fn stream_deserialize_unnamed_fields_n(fields: &FieldsUnnamed) -> (TokenStream, TokenStream) {
    let mut deserialize = TokenStream::new();
    let mut constructor = TokenStream::new();

    deserialize.extend(quote! {
        match __tokenizer.next_token().map_err(|e| e.into())? {
            serde_lite::Token::ArrayStart => {}
            __token => return Err(serde_lite::Error::invalid_value(format!("expected array, found {}", __token))),
        }
    });

    for (index, field) in fields.unnamed.iter().enumerate() {
        let ty = &field.ty;
        let sname = format!("f{}", index);
        let name = Ident::new(&sname, Span::call_site());

        deserialize.extend(quote! {
            let #name = <#ty as serde_lite::StreamDeserialize>::deserialize_from_tokens(__tokenizer)?;
        });

        constructor.extend(quote! {
            #name,
        });
    }

    deserialize.extend(quote! {
        match __tokenizer.next_token().map_err(|e| e.into())? {
            serde_lite::Token::ArrayEnd => {}
            __token => return Err(serde_lite::Error::invalid_value(format!("expected array end, found {}", __token))),
        }
    });

    (deserialize, constructor)
}

/// Generate code for deserializing given unnamed fields.
fn deserialize_unnamed_fields(fields: &FieldsUnnamed) -> (TokenStream, TokenStream) {
    match fields.unnamed.len() {
        0 => deserialize_unnamed_fields_0(),
        1 => deserialize_unnamed_fields_1(fields),
        _ => deserialize_unnamed_fields_n(fields),
    }
}

/// Generate code for deserializing given unnamed fields where the actual
/// number of fields is zero (e.g. zero-length tuple struct).
fn deserialize_unnamed_fields_0() -> (TokenStream, TokenStream) {
    let deserialize = TokenStream::new();
    let constructor = TokenStream::new();

    (deserialize, constructor)
}

/// Generate code for deserializing given unnamed fields where the actual
/// number of fields is one (e.g. single-element tuple struct).
fn deserialize_unnamed_fields_1(fields: &FieldsUnnamed) -> (TokenStream, TokenStream) {
    let mut deserialize = TokenStream::new();
    let mut constructor = TokenStream::new();

    let field = &fields.unnamed[0];
    let ty = &field.ty;
    let name = Ident::new("f0", Span::call_site());

    deserialize.extend(quote! {
        let #name = <#ty as serde_lite::Deserialize>::deserialize(__val)?;
    });

    constructor.extend(quote! {
        #name,
    });

    (deserialize, constructor)
}

/// Generate code for deserializing given unnamed fields where the actual
/// number of fields is greater than one (e.g. multiple-element tuple struct).
fn deserialize_unnamed_fields_n(fields: &FieldsUnnamed) -> (TokenStream, TokenStream) {
    let mut deserialize = TokenStream::new();
    let mut constructor = TokenStream::new();

    let len = Literal::usize_unsuffixed(fields.unnamed.len());

    deserialize.extend(quote! {
        let __arr = __val
            .as_array()
            .ok_or_else(|| serde_lite::Error::invalid_value_static("array"))?;

        if __arr.len() < #len {
            return Err(serde_lite::Error::invalid_value_static(concat!("array of length ", #len)));
        }

        let mut __field_errors = serde_lite::ErrorList::new();
    });

    for (index, field) in fields.unnamed.iter().enumerate() {
        let ty = &field.ty;
        let sname = format!("f{}", index);
        let name = Ident::new(&sname, Span::call_site());
        let lindex = Literal::usize_unsuffixed(index);

        deserialize.extend(quote! {
            let #name = <#ty as serde_lite::Deserialize>::deserialize(&__arr[#lindex])
                .map_err(|err| __field_errors.push(serde_lite::UnnamedFieldError::new(#lindex, err)))
                .ok();
        });

        constructor.extend(quote! {
            unsafe { #name.unwrap_unchecked() },
        });
    }

    deserialize.extend(quote! {
        if !__field_errors.is_empty() {
            return Err(serde_lite::Error::UnnamedFieldErrors(__field_errors));
        }
    });

    (deserialize, constructor)
}
