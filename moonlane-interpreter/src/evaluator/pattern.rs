use std::collections::HashMap;

use crate::ast::{Literal, Pattern};

use super::Value;

pub(super) fn match_pattern(pattern: &Pattern, value: &Value, out: &mut HashMap<String, Value>) -> bool {
    match pattern {
        Pattern::Wildcard(_) => true,

        Pattern::None(_) => matches!(value, Value::Perhaps(None)),

        Pattern::Literal(lit, _) => match (lit, value) {
            (Literal::Int(a),   Value::Int(b))          => a == b,
            (Literal::Float(a), Value::Float(b))        => a == b,
            (Literal::Bool(a),  Value::Bool(b))         => a == b,
            (Literal::Str(a),   Value::Str(b))          => a == b,
            (Literal::Unit,     Value::Unit)             => true,
            (Literal::None,     Value::Perhaps(None))   => true,
            _ => false,
        },

        Pattern::Binding(name, _) => {
            out.insert(name.clone(), value.clone());
            true
        }

        Pattern::Tuple(sub_patterns, _) => match value {
            Value::Tuple(elems) if elems.len() == sub_patterns.len() => {
                sub_patterns.iter().zip(elems.iter())
                    .all(|(p, v)| match_pattern(p, v, out))
            }
            _ => false,
        },

        Pattern::EnumVariant { path, fields, .. } => {
            let type_name   = if path.len() >= 2 { path[path.len() - 2].as_str() } else { "" };
            let variant_name = path.last().map(String::as_str).unwrap_or("");
            match (type_name, variant_name, value) {
                ("Perhaps", "Some", Value::Perhaps(Some(v))) => {
                    if let Some(field_name) = fields.first() {
                        out.insert(field_name.clone(), *v.clone());
                    }
                    true
                }
                ("Perhaps", "None", Value::Perhaps(None)) => true,
                ("Result", "Ok", Value::Result(Ok(v))) => {
                    if let Some(field_name) = fields.first() {
                        out.insert(field_name.clone(), *v.clone());
                    }
                    true
                }
                ("Result", "Err", Value::Result(Err(e))) => {
                    if let Some(field_name) = fields.first() {
                        out.insert(field_name.clone(), *e.clone());
                    }
                    true
                }
                (_, variant_name, Value::Enum { variant, fields: enum_fields, .. }) if variant == variant_name => {
                    for field_name in fields {
                        match enum_fields.get(field_name) {
                            Some(v) => { out.insert(field_name.clone(), v.clone()); }
                            None    => return false,
                        }
                    }
                    true
                }
                _ => false,
            }
        }
    }
}
