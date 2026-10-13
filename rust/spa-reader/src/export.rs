use base64::{Engine, engine::general_purpose::STANDARD};
use serde::{Serialize, Serializer, ser::SerializeSeq};

use crate::Document;

impl Document {
    /// Export every decoded series. Numeric values use locale-independent,
    /// round-trip formatting. Unknown fields remain in the JSON export.
    pub fn to_csv(&self) -> String {
        use std::fmt::Write;
        let mut csv = String::from(
            "series,secondary_selector,tertiary_selector,index,x,y,x_unit_code,y_unit_code\n",
        );
        for series in &self.series {
            for (index, (x, y)) in series.x.iter().zip(&series.y).enumerate() {
                // The kind comes from the parser, never from untrusted titles.
                writeln!(
                    csv,
                    "{},{},{},{},{},{},{},{}",
                    series.kind,
                    series.secondary_selector,
                    series.tertiary_selector,
                    index,
                    x,
                    csv_float(*y),
                    series.metadata.x_unit_code,
                    series.metadata.y_unit_code
                )
                .expect("writing to a String cannot fail");
            }
        }
        csv
    }

    /// JSON includes original file bytes and section payloads as base64. NaN
    /// and infinities use named strings; exact IEEE-754 bits remain in payloads.
    pub fn to_json(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string_pretty(self)
    }
}

fn csv_float(value: f32) -> String {
    if value == f32::INFINITY {
        "Infinity".into()
    } else if value == f32::NEG_INFINITY {
        "-Infinity".into()
    } else {
        value.to_string()
    }
}

pub(crate) fn serialize_bytes<S: Serializer>(
    bytes: &[u8],
    serializer: S,
) -> Result<S::Ok, S::Error> {
    serializer.serialize_str(&STANDARD.encode(bytes))
}

struct JsonFloat(f32);

impl Serialize for JsonFloat {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        if self.0.is_nan() {
            serializer.serialize_str("NaN")
        } else if self.0 == f32::INFINITY {
            serializer.serialize_str("Infinity")
        } else if self.0 == f32::NEG_INFINITY {
            serializer.serialize_str("-Infinity")
        } else {
            serializer.serialize_f32(self.0)
        }
    }
}

pub(crate) fn serialize_samples<S: Serializer>(
    values: &[f32],
    serializer: S,
) -> Result<S::Ok, S::Error> {
    let mut sequence = serializer.serialize_seq(Some(values.len()))?;
    for value in values {
        sequence.serialize_element(&JsonFloat(*value))?;
    }
    sequence.end()
}

pub(crate) fn serialize_optional_float<S: Serializer>(
    value: &Option<f32>,
    serializer: S,
) -> Result<S::Ok, S::Error> {
    match value {
        Some(value) => serializer.serialize_some(&JsonFloat(*value)),
        None => serializer.serialize_none(),
    }
}
