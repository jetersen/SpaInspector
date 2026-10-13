use serde::Serialize;

/// Encoding used for legacy title and history strings. Auto preserves valid
/// UTF-8 and falls back to Windows-1252, recording the choice in the document.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum TextEncoding {
    #[default]
    Auto,
    Utf8,
    Windows1252,
    Latin1,
}

impl TextEncoding {
    pub fn label(self) -> &'static str {
        match self {
            Self::Auto => "auto",
            Self::Utf8 => "utf-8",
            Self::Windows1252 => "windows-1252",
            Self::Latin1 => "latin1",
        }
    }
}

#[derive(Clone, Copy, Debug, Default)]
pub struct ReadOptions {
    pub text_encoding: TextEncoding,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Document {
    pub schema_version: u32,
    pub file_type_code: u16,
    pub revision_code: u16,
    pub directory_allocation_units: u16,
    pub title: String,
    #[serde(serialize_with = "crate::export::serialize_bytes")]
    pub title_bytes: Vec<u8>,
    pub title_encoding: String,
    pub history: String,
    #[serde(serialize_with = "crate::export::serialize_bytes")]
    pub history_bytes: Vec<u8>,
    pub history_encoding: String,
    pub timestamp_seconds: u32,
    pub timestamp_utc: String,
    pub text_encoding: String,
    #[serde(serialize_with = "crate::export::serialize_bytes")]
    pub source_bytes: Vec<u8>,
    pub sections: Vec<Section>,
    pub embedded_objects: Vec<EmbeddedObject>,
    pub series: Vec<Series>,
    pub warnings: Vec<String>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Section {
    #[serde(rename = "type")]
    pub type_code: u16,
    pub offset: u32,
    pub length: u32,
    pub secondary_selector: u16,
    pub tertiary_selector: u32,
    #[serde(serialize_with = "crate::export::serialize_bytes")]
    pub payload: Vec<u8>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Series {
    pub kind: String,
    pub name: String,
    pub secondary_selector: u16,
    pub tertiary_selector: u32,
    pub metadata: Metadata,
    pub x: Vec<f64>,
    #[serde(serialize_with = "crate::export::serialize_samples")]
    pub y: Vec<f32>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Metadata {
    pub storage_mode: u32,
    pub point_count: u32,
    pub x_unit_code: u32,
    pub y_unit_code: u32,
    pub x_unit: String,
    pub y_unit: String,
    pub stored_first_x: f32,
    pub stored_last_x: f32,
    pub first_x: f32,
    pub last_x: f32,
    pub scan_count: Option<u32>,
    pub background_scan_count: Option<u32>,
    pub collection_duration_centiseconds: Option<u32>,
    #[serde(serialize_with = "crate::export::serialize_optional_float")]
    pub signal_strength: Option<f32>,
    #[serde(serialize_with = "crate::export::serialize_optional_float")]
    pub gain: Option<f32>,
    #[serde(serialize_with = "crate::export::serialize_optional_float")]
    pub optical_velocity: Option<f32>,
    #[serde(serialize_with = "crate::export::serialize_optional_float")]
    pub laser_frequency_cm_inverse: Option<f32>,
    #[serde(serialize_with = "crate::export::serialize_optional_float")]
    pub background_gain: Option<f32>,
    #[serde(serialize_with = "crate::export::serialize_optional_float")]
    pub sample_spacing: Option<f32>,
    #[serde(serialize_with = "crate::export::serialize_optional_float")]
    pub raman_laser_frequency_cm_inverse: Option<f32>,
    pub processing_algorithm_code: Option<u32>,
    #[serde(serialize_with = "crate::export::serialize_bytes")]
    pub raw_header: Vec<u8>,
    #[serde(serialize_with = "crate::export::serialize_bytes")]
    pub raw_settings: Vec<u8>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EmbeddedObject {
    pub section_index: usize,
    pub class_id: u16,
    pub payload_class_id: u16,
    pub cached_class_id: Option<u16>,
    pub cached_body_word: Option<u16>,
    pub secondary_selector: u16,
    pub tertiary_selector: u32,
    pub declared_body_length: u32,
    pub kind: String,
    #[serde(serialize_with = "crate::export::serialize_bytes")]
    pub body: Vec<u8>,
    #[serde(serialize_with = "crate::export::serialize_bytes")]
    pub trailing_bytes: Vec<u8>,
    pub attributes: Vec<ObjectAttribute>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ObjectAttribute {
    pub id: u16,
    pub name: Option<String>,
    pub offset: u32,
    #[serde(rename = "type")]
    pub value_type: String,
    pub value: serde_json::Value,
}
