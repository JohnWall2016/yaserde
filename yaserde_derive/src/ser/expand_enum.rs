use crate::common::{Field, YaSerdeAttribute, YaSerdeField};
use crate::ser::{implement_serializer::implement_serializer, label::build_label_name};
use proc_macro2::TokenStream;
use syn::DataEnum;
use syn::Fields;
use syn::Ident;

pub fn serialize(
  data_enum: &DataEnum,
  name: &Ident,
  root: &str,
  root_attributes: &YaSerdeAttribute,
) -> TokenStream {
  let inner_enum_inspector = inner_enum_inspector(data_enum, name, root_attributes);

  implement_serializer(
    name,
    root,
    root_attributes,
    quote!(),
    quote!(match self {
      #inner_enum_inspector
    }),
  )
}

fn inner_enum_inspector(
  data_enum: &DataEnum,
  name: &Ident,
  root_attributes: &YaSerdeAttribute,
) -> TokenStream {
  data_enum
    .variants
    .iter()
    .map(|variant| {
      let variant_attrs = YaSerdeAttribute::parse(&variant.attrs);

      let label = &variant.ident;
      let label_name = build_label_name(&label, &variant_attrs, &root_attributes.default_namespace);

      match variant.fields {
        Fields::Unit => Some(quote! {
          &#name::#label => {
            let mut struct_start_event = XmlEvent::start_element(#label_name);
            writer.write(struct_start_event).map_err(|e| e.to_string())?;
            let struct_end_event = XmlEvent::end_element();
            writer.write(struct_end_event).map_err(|e| e.to_string())?;
          }
        }),
        Fields::Named(ref fields) => {
          let map_nonattr_field = |field: YaSerdeField| {
            let field_label = field.label();

            if field.is_text_content() {
              return Some(quote!(
                let data_event = XmlEvent::characters(&self.#field_label);
                writer.write(data_event).map_err(|e| e.to_string())?;
              ));
            }

            let field_label_name = field.renamed_label(root_attributes);

            match field.get_type() {
              Field::FieldString
              | Field::FieldBool
              | Field::FieldU8
              | Field::FieldI8
              | Field::FieldU16
              | Field::FieldI16
              | Field::FieldU32
              | Field::FieldI32
              | Field::FieldF32
              | Field::FieldU64
              | Field::FieldI64
              | Field::FieldF64
              | Field::FieldUSize => Some({
                quote! {
                  match self {
                    &#name::#label{ref #field_label, ..} => {
                      let struct_start_event = XmlEvent::start_element(#field_label_name);
                      writer.write(struct_start_event).map_err(|e| e.to_string())?;

                      let string_value = #field_label.to_string();
                      let data_event = XmlEvent::characters(&string_value);
                      writer.write(data_event).map_err(|e| e.to_string())?;

                      let struct_end_event = XmlEvent::end_element();
                      writer.write(struct_end_event).map_err(|e| e.to_string())?;
                    },
                    _ => {},
                  }
                }
              }),
              Field::FieldStruct { .. } => Some(quote! {
                match self {
                  &#name::#label{ref #field_label, ..} => {
                    writer.set_start_event_name(Some(#field_label_name.to_string()));
                    writer.set_skip_start_end(false);
                    #field_label.serialize(writer)?;
                  },
                  _ => {}
                }
              }),
              Field::FieldVec { .. } => Some(quote! {
                match self {
                  &#name::#label{ref #field_label, ..} => {
                    for item in #field_label {
                      writer.set_start_event_name(Some(#field_label_name.to_string()));
                      writer.set_skip_start_end(false);
                      item.serialize(writer)?;
                    }
                  },
                  _ => {}
                }
              }),
              Field::FieldOption { .. } => None,
            }
          };

          let map_attr_field = |field: YaSerdeField| {
            let field_label = field.label();
            let field_label_name = field.renamed_label(root_attributes);
            match field.get_type() {
              Field::FieldString => {
                Some(quote! {
                  match self {
                    &#name::#label{ref #field_label, ..} => {
                      struct_start_event = struct_start_event.attr(#field_label_name, &#field_label);
                    },
                    _ => {},
                  }
                })
              }
              _ => None,
            }
          };

          let collect_tokens = |vec: Vec<Option<TokenStream>>| -> TokenStream {
            vec.into_iter().filter_map(|x| x).collect()
          };

          let mut fields_tokens: Vec<Option<TokenStream>> = vec![];
          let mut append_attrs: Vec<Option<TokenStream>> = vec![];

          fields
            .named
            .iter()
            .map(|field| YaSerdeField::new(field.clone()))
            .for_each(|field| {
              if field.is_attribute() {
                append_attrs.push(map_attr_field(field))
              } else {
                fields_tokens.push(map_nonattr_field(field));
              }
            });

          let append_attrs = collect_tokens(append_attrs);
          let fields_tokens = collect_tokens(fields_tokens);

          if append_attrs.is_empty() {
            Some(quote! {
              &#name::#label{..} => {
                #fields_tokens
              }
            })
          } else {
            Some(quote! {
              &#name::#label{..} => {
                let mut struct_start_event = XmlEvent::start_element(#label_name);
                #append_attrs

                writer.write(struct_start_event).map_err(|e| e.to_string())?;

                #fields_tokens

                let struct_end_event = XmlEvent::end_element();
                writer.write(struct_end_event).map_err(|e| e.to_string())?;
              }
            })
          }
        }
        Fields::Unnamed(ref fields) => {
          let enum_fields: TokenStream = fields
            .unnamed
            .iter()
            .map(|field| YaSerdeField::new(field.clone()))
            .filter(|field| !field.is_attribute())
            .map(|field| {
              let write_element = |action: &TokenStream| {
                quote! {
                  let struct_start_event = XmlEvent::start_element(#label_name);
                  writer.write(struct_start_event).map_err(|e| e.to_string())?;

                  #action

                  let struct_end_event = XmlEvent::end_element();
                  writer.write(struct_end_event).map_err(|e| e.to_string())?;
                }
              };

              let write_string_chars = quote! {
                let data_event = XmlEvent::characters(item);
                writer.write(data_event).map_err(|e| e.to_string())?;
              };

              let write_simple_type = write_element(&quote! {
                let s = item.to_string();
                let data_event = XmlEvent::characters(&s);
                writer.write(data_event).map_err(|e| e.to_string())?;
              });

              let serialize = quote! {
                writer.set_start_event_name(None);
                writer.set_skip_start_end(true);
                item.serialize(writer)?;
              };

              let write_sub_type = |data_type| {
                write_element(match data_type {
                  Field::FieldString => &write_string_chars,
                  _ => &serialize,
                })
              };

              let match_field = |write: &TokenStream| {
                quote! {
                  match self {
                    &#name::#label(ref item) => {
                      #write
                    },
                    _ => {},
                  }
                }
              };

              match field.get_type() {
                Field::FieldOption { data_type } => {
                  let write = write_sub_type(*data_type);

                  Some(match_field(&quote! {
                    if let Some(item) = item {
                      #write
                    }
                  }))
                }
                Field::FieldVec { data_type } => {
                  let write = write_sub_type(*data_type);

                  Some(match_field(&quote! {
                    for item in item {
                      #write
                    }
                  }))
                }
                Field::FieldStruct { .. } => Some(write_element(&match_field(&serialize))),
                Field::FieldString => Some(match_field(&write_element(&write_string_chars))),
                _simple_type => Some(match_field(&write_simple_type)),
              }
            })
            .filter_map(|x| x)
            .collect();

          Some(quote! {
            &#name::#label{..} => {
              #enum_fields
            }
          })
        }
      }
    })
    .filter_map(|x| x)
    .collect()
}
