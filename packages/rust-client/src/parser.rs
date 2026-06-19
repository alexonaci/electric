//! Postgres value parser: converts Electric's text-encoded column values into
//! typed [`serde_json::Value`]s.
//!
//! Electric sends every column value as a JSON string (or `null`).  The
//! [`Parser`] reads the [`Schema`] from the `electric-schema` response header
//! and converts each string to the appropriate JSON type.
//!
//! Mirrors the behaviour of `packages/typescript-client/src/parser.ts`.

use std::collections::HashMap;
use std::sync::Arc;

use serde_json::Value;

use crate::error::ElectricError;
use crate::types::{ColumnInfo, Row, Schema};

// ── ParseFn ───────────────────────────────────────────────────────────────────

/// A function that converts a non-null Postgres text value to a JSON value.
pub type ParseFn = Arc<dyn Fn(&str) -> Value + Send + Sync>;

// ── Default parsers (match TypeScript defaultParser) ─────────────────────────

fn parse_int(v: &str) -> Value {
    // int2 / int4
    match v.parse::<i64>() {
        Ok(n) => Value::Number(n.into()),
        Err(_) => Value::String(v.to_owned()),
    }
}

fn parse_int8(v: &str) -> Value {
    // int8 / bigint — keep as string to avoid precision loss (JS BigInt pattern)
    Value::String(v.to_owned())
}

fn parse_float(v: &str) -> Value {
    match v.parse::<f64>() {
        Ok(f) => serde_json::Number::from_f64(f)
            .map(Value::Number)
            .unwrap_or_else(|| Value::String(v.to_owned())),
        Err(_) => Value::String(v.to_owned()),
    }
}

fn parse_bool(v: &str) -> Value {
    Value::Bool(v == "true" || v == "t")
}

fn parse_json(v: &str) -> Value {
    serde_json::from_str(v).unwrap_or_else(|_| Value::String(v.to_owned()))
}

// ── Postgres array parser ─────────────────────────────────────────────────────

/// Parse a Postgres array literal `{elem,elem,...}` (potentially nested) into a
/// JSON array.  Handles quoted strings, escapes, and `NULL` elements.
///
/// Ported from the PGlite implementation in `packages/typescript-client/src/parser.ts`.
pub fn pg_array_parse(text: &str, elem_parse: impl Fn(Option<&str>) -> Value) -> Value {
    let chars: Vec<char> = text.chars().collect();
    let mut pos = 0usize;

    fn parse_array(
        chars: &[char],
        pos: &mut usize,
        elem_parse: &impl Fn(Option<&str>) -> Value,
    ) -> Value {
        // Skip opening '{'
        if chars.get(*pos) == Some(&'{') {
            *pos += 1;
        }

        let mut result = Vec::new();

        while *pos < chars.len() {
            match chars[*pos] {
                '}' => {
                    *pos += 1; // consume '}'
                    break;
                }
                ',' => {
                    *pos += 1; // skip separator
                }
                '{' => {
                    // Nested array
                    result.push(parse_array(chars, pos, elem_parse));
                }
                '"' => {
                    // Quoted string element
                    *pos += 1; // skip opening '"'
                    let mut s = String::new();
                    while *pos < chars.len() {
                        match chars[*pos] {
                            '\\' => {
                                *pos += 1;
                                if let Some(&c) = chars.get(*pos) {
                                    s.push(c);
                                    *pos += 1;
                                }
                            }
                            '"' => {
                                *pos += 1; // consume closing '"'
                                break;
                            }
                            c => {
                                s.push(c);
                                *pos += 1;
                            }
                        }
                    }
                    result.push(elem_parse(Some(&s)));
                }
                _ => {
                    // Unquoted element (number, bool, NULL, etc.)
                    let start = *pos;
                    while *pos < chars.len()
                        && chars[*pos] != ','
                        && chars[*pos] != '}'
                        && chars[*pos] != '{'
                    {
                        *pos += 1;
                    }
                    let token: String = chars[start..*pos].iter().collect();
                    if token == "NULL" {
                        result.push(elem_parse(None));
                    } else {
                        result.push(elem_parse(Some(&token)));
                    }
                }
            }
        }

        Value::Array(result)
    }

    parse_array(&chars, &mut pos, &elem_parse)
}

// ── Parser ────────────────────────────────────────────────────────────────────

/// Converts raw Electric row values (strings / null) to typed JSON values.
///
/// Built-in defaults cover the most common Postgres types.  Custom parsers can
/// be registered with [`Parser::with`] and will take precedence over defaults.
///
/// ```rust
/// use electric_client::Parser;
///
/// let parser = Parser::new()
///     .with("timestamptz", |v| serde_json::Value::String(v.to_string()));
/// ```
#[derive(Clone, Default)]
pub struct Parser {
    custom: HashMap<String, ParseFn>,
}

impl Parser {
    /// Create a `Parser` that uses only the built-in default type conversions.
    pub fn new() -> Self {
        Self {
            custom: HashMap::new(),
        }
    }

    /// Register a custom parse function for a Postgres type name.
    ///
    /// The function receives the raw non-null string value and should return a
    /// `serde_json::Value`.  For null values the parser automatically returns
    /// `Value::Null` (or errors if the column is NOT NULL).
    pub fn with<F>(mut self, pg_type: impl Into<String>, f: F) -> Self
    where
        F: Fn(&str) -> Value + Send + Sync + 'static,
    {
        self.custom.insert(pg_type.into(), Arc::new(f));
        self
    }

    /// Parse a single value given its Postgres type and nullability.
    ///
    /// - `value`: `None` = SQL NULL, `Some(s)` = text-encoded non-null value.
    /// - `column_info`: metadata from the schema header.
    /// - `column_name`: used in the error message for NOT NULL violations.
    pub fn parse_value(
        &self,
        value: Option<&str>,
        column_info: &ColumnInfo,
        column_name: &str,
    ) -> Result<Value, ElectricError> {
        match value {
            None => {
                if column_info.not_null {
                    Err(ElectricError::ParserNullValue {
                        column: column_name.to_owned(),
                    })
                } else {
                    Ok(Value::Null)
                }
            }
            Some(v) => {
                // Array type?
                if column_info.dimensions > 0 {
                    let elem_ci = ColumnInfo {
                        pg_type: column_info.pg_type.clone(),
                        dimensions: 0,
                        not_null: false,
                        ..Default::default()
                    };
                    let result = pg_array_parse(v, |elem| {
                        // Ignore parse errors inside arrays; fall back to null/string
                        self.parse_scalar(elem, &elem_ci).unwrap_or(Value::Null)
                    });
                    Ok(result)
                } else {
                    self.parse_scalar(Some(v), column_info)
                }
            }
        }
    }

    /// Parse a single scalar (non-array) value.
    fn parse_scalar(
        &self,
        value: Option<&str>,
        column_info: &ColumnInfo,
    ) -> Result<Value, ElectricError> {
        let v = match value {
            None => return Ok(Value::Null),
            Some(v) => v,
        };

        // Custom parser has priority
        if let Some(f) = self.custom.get(&column_info.pg_type) {
            return Ok(f(v));
        }

        // Built-in defaults
        let result = match column_info.pg_type.as_str() {
            "int2" | "int4" => parse_int(v),
            "int8" => parse_int8(v),
            "float4" | "float8" | "numeric" => parse_float(v),
            "bool" => parse_bool(v),
            "json" | "jsonb" => parse_json(v),
            _ => Value::String(v.to_owned()),
        };

        Ok(result)
    }

    /// Apply this parser to every field of a raw row, using the given schema.
    ///
    /// Fields not present in `schema` are passed through unchanged.
    pub fn parse_row(&self, raw: &Row, schema: &Schema) -> Result<Row, ElectricError> {
        let mut out = Row::new();
        for (col, raw_val) in raw {
            let parsed = if let Some(ci) = schema.get(col) {
                // Electric sends non-null column values as JSON strings
                let str_val = match raw_val {
                    Value::String(s) => Some(s.as_str()),
                    Value::Null => None,
                    // Unexpected type (e.g. already-parsed JSON subobject for jsonb) –
                    // keep as-is
                    other => {
                        out.insert(col.clone(), other.clone());
                        continue;
                    }
                };
                self.parse_value(str_val, ci, col)?
            } else {
                // No schema info: pass through as-is
                raw_val.clone()
            };
            out.insert(col.clone(), parsed);
        }
        Ok(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::ColumnInfo;

    fn ci(pg_type: &str) -> ColumnInfo {
        ColumnInfo {
            pg_type: pg_type.to_string(),
            ..Default::default()
        }
    }

    fn ci_not_null(pg_type: &str) -> ColumnInfo {
        ColumnInfo {
            pg_type: pg_type.to_string(),
            not_null: true,
            ..Default::default()
        }
    }

    fn ci_array(pg_type: &str) -> ColumnInfo {
        ColumnInfo {
            pg_type: pg_type.to_string(),
            dimensions: 1,
            ..Default::default()
        }
    }

    fn p() -> Parser {
        Parser::new()
    }

    // ── Scalar types ──────────────────────────────────────────────────────────

    #[test]
    fn parses_int4() {
        assert_eq!(
            p().parse_value(Some("42"), &ci("int4"), "n").unwrap(),
            serde_json::json!(42i64)
        );
    }

    #[test]
    fn parses_negative_int4() {
        assert_eq!(
            p().parse_value(Some("-7"), &ci("int4"), "n").unwrap(),
            serde_json::json!(-7i64)
        );
    }

    #[test]
    fn parses_int2() {
        assert_eq!(
            p().parse_value(Some("100"), &ci("int2"), "n").unwrap(),
            serde_json::json!(100i64)
        );
    }

    #[test]
    fn parses_int8_as_string() {
        // int8 must stay as a string to avoid i64 overflow / JS BigInt compat
        let v = p()
            .parse_value(Some("9223372036854775807"), &ci("int8"), "n")
            .unwrap();
        assert_eq!(v, serde_json::json!("9223372036854775807"));
    }

    #[test]
    fn parses_float4() {
        let v = p().parse_value(Some("3.14"), &ci("float4"), "n").unwrap();
        assert!(matches!(v, Value::Number(_)));
    }

    #[test]
    fn parses_float8() {
        let v = p()
            .parse_value(Some("2.718281828459045"), &ci("float8"), "n")
            .unwrap();
        assert!(matches!(v, Value::Number(_)));
    }

    #[test]
    fn parses_bool_true_t() {
        assert_eq!(
            p().parse_value(Some("t"), &ci("bool"), "n").unwrap(),
            Value::Bool(true)
        );
    }

    #[test]
    fn parses_bool_true_word() {
        assert_eq!(
            p().parse_value(Some("true"), &ci("bool"), "n").unwrap(),
            Value::Bool(true)
        );
    }

    #[test]
    fn parses_bool_false() {
        assert_eq!(
            p().parse_value(Some("false"), &ci("bool"), "n").unwrap(),
            Value::Bool(false)
        );
    }

    #[test]
    fn parses_bool_f() {
        assert_eq!(
            p().parse_value(Some("f"), &ci("bool"), "n").unwrap(),
            Value::Bool(false)
        );
    }

    #[test]
    fn parses_text_identity() {
        let v = p()
            .parse_value(Some("hello world"), &ci("text"), "n")
            .unwrap();
        assert_eq!(v, serde_json::json!("hello world"));
    }

    #[test]
    fn parses_json_object() {
        let v = p()
            .parse_value(Some(r#"{"a":1}"#), &ci("json"), "n")
            .unwrap();
        assert_eq!(v, serde_json::json!({"a": 1}));
    }

    #[test]
    fn parses_jsonb_array() {
        let v = p().parse_value(Some("[1,2,3]"), &ci("jsonb"), "n").unwrap();
        assert_eq!(v, serde_json::json!([1, 2, 3]));
    }

    // ── Null handling ─────────────────────────────────────────────────────────

    #[test]
    fn null_on_nullable_column_returns_null() {
        let v = p().parse_value(None, &ci("int4"), "col").unwrap();
        assert_eq!(v, Value::Null);
    }

    #[test]
    fn null_on_not_null_column_errors() {
        let err = p()
            .parse_value(None, &ci_not_null("int4"), "col")
            .unwrap_err();
        assert!(matches!(err, ElectricError::ParserNullValue { .. }));
    }

    // ── Array types ───────────────────────────────────────────────────────────

    #[test]
    fn parses_int_array() {
        let v = p()
            .parse_value(Some("{1,2,3}"), &ci_array("int4"), "n")
            .unwrap();
        assert_eq!(v, serde_json::json!([1, 2, 3]));
    }

    #[test]
    fn parses_text_array() {
        let v = p()
            .parse_value(Some(r#"{"hello","world"}"#), &ci_array("text"), "n")
            .unwrap();
        assert_eq!(v, serde_json::json!(["hello", "world"]));
    }

    #[test]
    fn parses_array_with_null() {
        let v = p()
            .parse_value(Some("{1,NULL,3}"), &ci_array("int4"), "n")
            .unwrap();
        assert_eq!(v, serde_json::json!([1, null, 3]));
    }

    #[test]
    fn parses_bool_array() {
        let v = p()
            .parse_value(Some("{t,f,t}"), &ci_array("bool"), "n")
            .unwrap();
        assert_eq!(v, serde_json::json!([true, false, true]));
    }

    // ── Custom parsers ────────────────────────────────────────────────────────

    #[test]
    fn custom_parser_overrides_default() {
        let parser = Parser::new().with("int4", |v| serde_json::json!(format!("custom:{}", v)));
        let v = parser.parse_value(Some("42"), &ci("int4"), "n").unwrap();
        assert_eq!(v, serde_json::json!("custom:42"));
    }

    // ── parse_row ─────────────────────────────────────────────────────────────

    #[test]
    fn parse_row_applies_schema() {
        let mut schema = Schema::new();
        schema.insert("id".to_string(), ci("int4"));
        schema.insert("active".to_string(), ci("bool"));

        let mut raw = Row::new();
        raw.insert("id".to_string(), Value::String("5".to_string()));
        raw.insert("active".to_string(), Value::String("t".to_string()));

        let parsed = Parser::new().parse_row(&raw, &schema).unwrap();
        assert_eq!(parsed["id"], serde_json::json!(5i64));
        assert_eq!(parsed["active"], Value::Bool(true));
    }

    #[test]
    fn parse_row_passes_through_unknown_column() {
        let schema = Schema::new(); // empty schema
        let mut raw = Row::new();
        raw.insert(
            "unknown_col".to_string(),
            Value::String("raw_value".to_string()),
        );

        let parsed = Parser::new().parse_row(&raw, &schema).unwrap();
        assert_eq!(
            parsed["unknown_col"],
            Value::String("raw_value".to_string())
        );
    }
}
