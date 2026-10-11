//! Schema pieces the published documents share.

use serde_json::Value;
use utoipa::PartialSchema;
use utoipa::openapi::schema::{Object, ObjectBuilder, SchemaType, Type};
use utoipa::openapi::{RefOr, Schema};

/// An open enumeration (conventions §10): a string with `known` as its examples and no closed `enum`.
pub fn open_enumeration(description: &str, known: impl IntoIterator<Item = Value>) -> Object {
    ObjectBuilder::new()
        .schema_type(SchemaType::new(Type::String))
        .description(Some(description))
        .examples(known)
        .build()
}

/// The values `T`'s derived schema lists in its closed `enum`.
#[must_use]
pub fn enum_values<T: PartialSchema>() -> Vec<Value> {
    match T::schema() {
        RefOr::T(Schema::Object(object)) => object.enum_values.unwrap_or_default(),
        _ => Vec::new(),
    }
}
