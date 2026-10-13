use std::{collections::HashSet, fmt};

use crate::{
    Document, EmbeddedObject, Metadata, ObjectAttribute, ReadOptions, Section, Series, TextEncoding,
};

const HEADER_SIZE: usize = 0x130;
const MAGIC: &[u8] = b"Spectral Data File\r\n\0";

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Error {
    pub kind: &'static str,
    pub message: String,
    pub offset: Option<usize>,
}

impl Error {
    fn new(kind: &'static str, message: impl Into<String>, offset: Option<usize>) -> Self {
        Self {
            kind,
            message: message.into(),
            offset,
        }
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.message)?;
        if let Some(offset) = self.offset {
            write!(f, " (offset 0x{offset:x})")?;
        }
        Ok(())
    }
}

impl std::error::Error for Error {}

/// Parse a clear SPA container. The input is never changed. Decoded samples
/// follow native OMNIC ordering, while original bytes remain available exactly.
pub fn parse(bytes: &[u8], options: &ReadOptions) -> Result<Document, Error> {
    if bytes.len() < HEADER_SIZE {
        return Err(Error::new(
            "truncated_header",
            "SPA header requires 304 bytes",
            None,
        ));
    }
    if !bytes.starts_with(MAGIC) {
        return Err(Error::new(
            "invalid_magic",
            "Not an OMNIC Spectral Data File",
            Some(0),
        ));
    }
    let file_type_code = u16_at(bytes, 0x120);
    let revision_code = u16_at(bytes, 0x122);
    let directory_allocation_units = u16_at(bytes, 0x124);
    let count = usize::from(u16_at(bytes, 0x126));
    if directory_allocation_units > 10_000 {
        return Err(Error::new(
            "invalid_directory",
            "SPA directory allocation exceeds OMNIC's limit",
            Some(0x124),
        ));
    }
    let capacity = usize::from(directory_allocation_units) * 16;
    if count > capacity {
        return Err(Error::new(
            "invalid_directory",
            "Section count exceeds the allocated directory",
            Some(0x126),
        ));
    }
    let directory_end = HEADER_SIZE + usize::from(directory_allocation_units) * 256;
    if directory_end > bytes.len() {
        return Err(Error::new(
            "truncated_directory",
            "Allocated SPA directory extends beyond the file",
            Some(HEADER_SIZE),
        ));
    }
    let mut warnings = Vec::new();
    if file_type_code != 1 {
        warnings.push(format!(
            "Container code {file_type_code} is not verified as a single-spectrum SPA file."
        ));
    }
    let mut sections = Vec::with_capacity(count);
    let mut keys = HashSet::new();
    for index in 0..count {
        let position = HEADER_SIZE + index * 16;
        let type_code = u16_at(bytes, position);
        let offset = u32_at(bytes, position + 2);
        let length = u32_at(bytes, position + 6);
        let secondary_selector = u16_at(bytes, position + 10);
        let tertiary_selector = u32_at(bytes, position + 12);
        let start = offset as usize;
        let size = length as usize;
        let end = start.checked_add(size).ok_or_else(|| {
            Error::new(
                "invalid_range",
                "Section range overflows",
                Some(position + 2),
            )
        })?;
        if end > bytes.len() || (size != 0 && start < directory_end) {
            return Err(Error::new(
                "invalid_range",
                format!(
                    "Section {type_code} overlaps the header/directory or extends beyond the file"
                ),
                Some(position + 2),
            ));
        }
        if !keys.insert((type_code, secondary_selector, tertiary_selector)) {
            warnings.push(format!("Duplicate section ({type_code}, {secondary_selector}, {tertiary_selector}); decoding uses its first occurrence."));
        }
        sections.push(Section {
            type_code,
            offset,
            length,
            secondary_selector,
            tertiary_selector,
            payload: bytes[start..end].to_vec(),
        });
    }

    let title_bytes = c_string(&bytes[30..0x11e]).to_vec();
    let (title, title_encoding) =
        decode_text(&title_bytes, options.text_encoding, &mut warnings, "title")?;
    let history_bytes =
        find(&sections, 27, 0, 0).map_or_else(Vec::new, |s| c_string(&s.payload).to_vec());
    let (history, history_encoding) = decode_text(
        &history_bytes,
        options.text_encoding,
        &mut warnings,
        "history",
    )?;
    let timestamp_seconds = u32_at(bytes, 0x128);
    let timestamp_utc = timestamp(timestamp_seconds);

    let mut series = Vec::new();
    let mut groups = HashSet::new();
    for header in sections.iter().filter(|section| section.type_code == 2) {
        let secondary = header.secondary_selector;
        let tertiary = header.tertiary_selector;
        if !groups.insert((secondary, tertiary)) {
            continue;
        }
        let data = find(&sections, 3, secondary, tertiary).ok_or_else(|| {
            Error::new(
                "missing_section",
                format!("Spectrum header ({secondary}, {tertiary}) has no matching data section 3"),
                Some(header.offset as usize),
            )
        })?;
        let settings = find(&sections, 106, secondary, tertiary);
        let processing = find(&sections, 384, secondary, tertiary);
        let metadata = spectrum_metadata(header, settings, processing)?;
        let mut y = samples(data, metadata.point_count)?;
        // AP20_DataRead_FILE copies forward for volts (22), otherwise backward
        // for the ordinary clear SPA file-object path. No input span is reversed.
        if metadata.y_unit_code != 22 {
            y.reverse();
        }
        warn_samples(&y, &mut warnings);
        series.push(Series {
            kind: "spectrum".into(),
            name: title.clone(),
            secondary_selector: secondary,
            tertiary_selector: tertiary,
            x: axis(metadata.first_x, metadata.last_x, y.len()),
            y,
            metadata,
        });
    }
    for data in sections.iter().filter(|s| s.type_code == 3) {
        if find(
            &sections,
            2,
            data.secondary_selector,
            data.tertiary_selector,
        )
        .is_none()
        {
            return Err(Error::new(
                "missing_section",
                "Spectrum data has no matching section 2 header",
                Some(data.offset as usize),
            ));
        }
    }

    for (header_type, data_type, kind, name) in [
        (100, 102, "sample_interferogram", "Sample interferogram"),
        (
            101,
            103,
            "background_interferogram",
            "Background interferogram",
        ),
        (
            379,
            381,
            "auxiliary_interferogram_381",
            "Auxiliary interferogram 381",
        ),
        (
            380,
            382,
            "auxiliary_interferogram_382",
            "Auxiliary interferogram 382",
        ),
    ] {
        let mut seen = HashSet::new();
        for data in sections.iter().filter(|s| s.type_code == data_type) {
            let secondary = data.secondary_selector;
            let tertiary = data.tertiary_selector;
            if !seen.insert((secondary, tertiary)) {
                continue;
            }
            let header = find(&sections, header_type, secondary, tertiary).ok_or_else(|| {
                Error::new(
                    "missing_section",
                    format!("Interferogram {data_type} has no matching header {header_type}"),
                    Some(data.offset as usize),
                )
            })?;
            if header.payload.len() < 4 {
                return Err(Error::new(
                    "truncated_metadata",
                    "Interferogram header requires a point count",
                    Some(header.offset as usize),
                ));
            }
            let point_count = u32_at(&header.payload, 0);
            let y = samples(data, point_count)?;
            warn_samples(&y, &mut warnings);
            let last = if point_count == 0 {
                0.0
            } else {
                (point_count - 1) as f32
            };
            let metadata = Metadata {
                storage_mode: 1,
                point_count,
                x_unit_code: 2,
                y_unit_code: 22,
                x_unit: "points".into(),
                y_unit: "V".into(),
                stored_first_x: 0.0,
                stored_last_x: last,
                first_x: 0.0,
                last_x: last,
                scan_count: optional_u32(&header.payload, 4),
                background_scan_count: None,
                signal_strength: None,
                gain: None,
                optical_velocity: None,
                laser_frequency_cm_inverse: optional_f32(&header.payload, 20)
                    .filter(|value| *value > 0.0),
                background_gain: None,
                collection_duration_centiseconds: None,
                sample_spacing: None,
                raman_laser_frequency_cm_inverse: None,
                processing_algorithm_code: None,
                raw_header: header.payload.clone(),
                raw_settings: Vec::new(),
            };
            series.push(Series {
                kind: kind.into(),
                name: name.into(),
                secondary_selector: secondary,
                tertiary_selector: tertiary,
                x: (0..y.len()).map(|index| index as f64).collect(),
                y,
                metadata,
            });
        }
        for header in sections.iter().filter(|s| s.type_code == header_type) {
            if find(
                &sections,
                data_type,
                header.secondary_selector,
                header.tertiary_selector,
            )
            .is_none()
            {
                warnings.push(format!(
                    "Interferogram header {header_type} has no matching data section {data_type}."
                ));
            }
        }
    }
    if series.is_empty() {
        warnings.push(
            "No supported spectrum or interferogram was found; original data is retained in JSON."
                .into(),
        );
    }
    if sections
        .iter()
        .any(|s| !matches!(s.type_code, 2 | 3 | 27 | 100 | 101 | 102 | 103 | 106))
    {
        warnings.push("Some sections remain undecoded. Their selectors and original payloads are retained in JSON.".into());
    }
    let embedded_objects = decode_objects(&sections, options.text_encoding, &mut warnings)?;
    Ok(Document {
        schema_version: 1,
        file_type_code,
        revision_code,
        directory_allocation_units,
        title,
        title_bytes,
        title_encoding,
        history,
        history_bytes,
        history_encoding,
        timestamp_seconds,
        timestamp_utc,
        text_encoding: options.text_encoding.label().into(),
        source_bytes: bytes.to_vec(),
        sections,
        embedded_objects,
        series,
        warnings,
    })
}

fn spectrum_metadata(
    header: &Section,
    settings: Option<&Section>,
    processing: Option<&Section>,
) -> Result<Metadata, Error> {
    let data = &header.payload;
    if data.len() < 24 {
        return Err(Error::new(
            "truncated_metadata",
            "Spectrum header requires at least 24 bytes",
            Some(header.offset as usize),
        ));
    }
    let storage_mode = u32_at(data, 0);
    let point_count = u32_at(data, 4);
    let x_unit_code = u32_at(data, 8);
    let y_unit_code = u32_at(data, 12);
    let stored_first_x = f32_at(data, 16);
    let stored_last_x = f32_at(data, 20);
    if !stored_first_x.is_finite() || !stored_last_x.is_finite() {
        return Err(Error::new(
            "invalid_axis",
            "Spectrum endpoints must be finite",
            Some(header.offset as usize + 16),
        ));
    }
    let (first_x, last_x) = if storage_mode == 1 {
        (stored_first_x, stored_last_x)
    } else {
        (stored_last_x, stored_first_x)
    };
    let settings_bytes = settings.map_or(&[][..], |section| section.payload.as_slice());
    Ok(Metadata {
        storage_mode,
        point_count,
        x_unit_code,
        y_unit_code,
        x_unit: x_unit(x_unit_code),
        y_unit: y_unit(y_unit_code),
        stored_first_x,
        stored_last_x,
        first_x,
        last_x,
        scan_count: optional_u32(data, 36),
        background_scan_count: optional_u32(data, 52),
        signal_strength: optional_f32(data, 72),
        gain: optional_f32(settings_bytes, 44),
        optical_velocity: optional_f32(settings_bytes, 48),
        laser_frequency_cm_inverse: optional_f32(data, 80),
        background_gain: optional_f32(data, 56),
        collection_duration_centiseconds: optional_u32(data, 68),
        sample_spacing: optional_f32(data, 84),
        raman_laser_frequency_cm_inverse: optional_f32(data, 96),
        processing_algorithm_code: processing.and_then(|section| optional_u32(&section.payload, 0)),
        raw_header: data.clone(),
        raw_settings: settings_bytes.to_vec(),
    })
}

fn decode_objects(
    sections: &[Section],
    encoding: TextEncoding,
    warnings: &mut Vec<String>,
) -> Result<Vec<EmbeddedObject>, Error> {
    let mut objects = Vec::new();
    for (section_index, section) in sections
        .iter()
        .enumerate()
        .filter(|(_, section)| section.type_code == 130)
    {
        let payload = &section.payload;
        if payload.len() < 6 {
            warnings.push(format!("Embedded object section {section_index} has an incomplete envelope; its raw data is retained."));
            continue;
        }
        let payload_class_id = u16_at(payload, 0);
        let declared_body_length = u32_at(payload, 2);
        let Some(end) = 6usize
            .checked_add(declared_body_length as usize)
            .filter(|end| *end <= payload.len())
        else {
            warnings.push(format!("Embedded object section {section_index} has an out-of-range body length; its raw data is retained."));
            continue;
        };
        let cached_class_id = ((section.tertiary_selector >> 16) as u16 != 0)
            .then_some((section.tertiary_selector >> 16) as u16);
        let cached_body_word =
            ((section.tertiary_selector as u16) != 0).then_some(section.tertiary_selector as u16);
        let class_id = cached_class_id.unwrap_or(payload_class_id);
        let kind = match class_id {
            121 => "experiment".into(),
            158 => "instrument_status".into(),
            159 => "advanced_atr".into(),
            166 => "instrument_specification".into(),
            _ => format!("class_{class_id}"),
        };
        let mut attributes = Vec::new();
        if class_id != payload_class_id {
            warnings.push(format!("Embedded object section {section_index} has disagreeing cached and payload class IDs; attributes are not interpreted."));
        } else if class_id == 121 {
            for (id, offset, length, name) in [
                (2, 10, 80, Some("shortExperimentPath")),
                (3, 90, 64, Some("experimentTitle")),
                (4, 154, 255, Some("description")),
                (7, 413, 100, None),
                (49, 671, 260, Some("longExperimentPath")),
            ] {
                if offset + length > end {
                    continue;
                }
                let text = decode_text(
                    c_string(&payload[offset..offset + length]),
                    encoding,
                    warnings,
                    "experiment attribute",
                )?
                .0;
                attributes.push(ObjectAttribute {
                    id,
                    name: name.map(str::to_string),
                    offset: offset as u32,
                    value_type: "string".into(),
                    value: serde_json::Value::String(text),
                });
            }
        } else if class_id == 159 {
            for id in 0..7 {
                let offset = 6 + usize::from(id) * 4;
                if offset + 4 > end {
                    continue;
                }
                attributes.push(ObjectAttribute {
                    id,
                    name: [
                        Some("applied"),
                        Some("sampleRefractiveIndex"),
                        Some("crystalRefractiveIndex"),
                        Some("numberOfBounces"),
                        Some("angleOfIncidenceDegrees"),
                        None,
                        None,
                    ][usize::from(id)]
                    .map(str::to_string),
                    offset: offset as u32,
                    value_type: "float32".into(),
                    value: float_value(f32_at(payload, offset)),
                });
            }
        } else if class_id == 166 {
            if end >= 8 {
                attributes.push(ObjectAttribute {
                    id: 0,
                    name: None,
                    offset: 6,
                    value_type: "uint16".into(),
                    value: u16_at(payload, 6).into(),
                });
            }
            if end >= 12 {
                attributes.push(ObjectAttribute {
                    id: 1,
                    name: None,
                    offset: 8,
                    value_type: "float32".into(),
                    value: float_value(f32_at(payload, 8)),
                });
            }
        }
        objects.push(EmbeddedObject {
            section_index,
            class_id,
            payload_class_id,
            cached_class_id,
            cached_body_word,
            secondary_selector: section.secondary_selector,
            tertiary_selector: section.tertiary_selector,
            declared_body_length,
            kind,
            body: payload[6..end].to_vec(),
            trailing_bytes: payload[end..].to_vec(),
            attributes,
        });
    }
    Ok(objects)
}

fn float_value(value: f32) -> serde_json::Value {
    if value.is_nan() {
        "NaN".into()
    } else if value == f32::INFINITY {
        "Infinity".into()
    } else if value == f32::NEG_INFINITY {
        "-Infinity".into()
    } else {
        serde_json::Value::from(value)
    }
}

fn samples(section: &Section, expected_count: u32) -> Result<Vec<f32>, Error> {
    if !section.payload.len().is_multiple_of(4) {
        return Err(Error::new(
            "invalid_samples",
            format!(
                "Section {} has an incomplete float32 sample",
                section.type_code
            ),
            Some(section.offset as usize),
        ));
    }
    if section.payload.len() / 4 != expected_count as usize {
        return Err(Error::new(
            "point_count_mismatch",
            format!(
                "Section {} has {} samples but its header declares {expected_count}",
                section.type_code,
                section.payload.len() / 4
            ),
            Some(section.offset as usize),
        ));
    }
    Ok(section
        .payload
        .as_chunks::<4>()
        .0
        .iter()
        .map(|chunk| f32::from_le_bytes(*chunk))
        .collect())
}

fn axis(first: f32, last: f32, count: usize) -> Vec<f64> {
    match count {
        0 => Vec::new(),
        1 => vec![f64::from(first)],
        _ => {
            let first = f64::from(first);
            let last = f64::from(last);
            let step = (last - first) / (count - 1) as f64;
            (0..count)
                .map(|index| {
                    if index + 1 == count {
                        last
                    } else {
                        first + index as f64 * step
                    }
                })
                .collect()
        }
    }
}

fn warn_samples(samples: &[f32], warnings: &mut Vec<String>) {
    let count = samples.iter().filter(|value| !value.is_finite()).count();
    if count != 0 {
        warnings.push(format!("Series contains {count} non-finite samples; plots omit them and JSON uses named floating-point strings."));
    }
}

fn find(sections: &[Section], type_code: u16, secondary: u16, tertiary: u32) -> Option<&Section> {
    sections.iter().find(|s| {
        s.type_code == type_code
            && s.secondary_selector == secondary
            && s.tertiary_selector == tertiary
    })
}

fn c_string(bytes: &[u8]) -> &[u8] {
    &bytes[..bytes
        .iter()
        .position(|byte| *byte == 0)
        .unwrap_or(bytes.len())]
}

fn decode_text(
    bytes: &[u8],
    encoding: TextEncoding,
    warnings: &mut Vec<String>,
    field: &str,
) -> Result<(String, String), Error> {
    if encoding == TextEncoding::Auto || encoding == TextEncoding::Utf8 {
        match std::str::from_utf8(bytes) {
            Ok(text) => return Ok((text.into(), "utf-8".into())),
            Err(_) if encoding == TextEncoding::Utf8 => return Err(Error::new("invalid_text", format!("{field} is not valid UTF-8; select a legacy text encoding"), None)),
            Err(_) => warnings.push(format!("{field} is not UTF-8; decoded as Windows-1252. The producer's code page is not recorded here; raw bytes are retained.")),
        }
    }
    if encoding == TextEncoding::Latin1 {
        return Ok((
            bytes.iter().map(|byte| char::from(*byte)).collect(),
            "latin1".into(),
        ));
    }
    let (text, had_errors) = encoding_rs::WINDOWS_1252.decode_without_bom_handling(bytes);
    if had_errors {
        warnings.push(format!(
            "{field} contains bytes not representable in Windows-1252; raw bytes are retained."
        ));
    }
    Ok((text.into_owned(), "windows-1252".into()))
}

fn timestamp(seconds: u32) -> String {
    // The epoch used by existing readers and the fixtures is 1899-12-31 UTC.
    let epoch = time::Date::from_calendar_date(1899, time::Month::December, 31)
        .expect("fixed date")
        .midnight()
        .assume_utc();
    let date = epoch + time::Duration::seconds(i64::from(seconds));
    format!(
        "{:04}-{:02}-{:02}T{:02}:{:02}:{:02}Z",
        date.year(),
        u8::from(date.month()),
        date.day(),
        date.hour(),
        date.minute(),
        date.second()
    )
}

fn u16_at(bytes: &[u8], offset: usize) -> u16 {
    u16::from_le_bytes(
        bytes[offset..offset + 2]
            .try_into()
            .expect("range checked by caller"),
    )
}

fn u32_at(bytes: &[u8], offset: usize) -> u32 {
    u32::from_le_bytes(
        bytes[offset..offset + 4]
            .try_into()
            .expect("range checked by caller"),
    )
}

fn f32_at(bytes: &[u8], offset: usize) -> f32 {
    f32::from_bits(u32_at(bytes, offset))
}
fn optional_u32(bytes: &[u8], offset: usize) -> Option<u32> {
    (offset + 4 <= bytes.len()).then(|| u32_at(bytes, offset))
}
fn optional_f32(bytes: &[u8], offset: usize) -> Option<f32> {
    optional_u32(bytes, offset).map(f32::from_bits)
}

fn x_unit(code: u32) -> String {
    match code {
        1 | 32 => "cm^-1".into(),
        2 => "points".into(),
        3 => "nm".into(),
        4 => "um".into(),
        _ => format!("code {code}"),
    }
}

fn y_unit(code: u32) -> String {
    match code {
        11 | 16 => "%".into(),
        12 => "log(1/R)".into(),
        15 => "single beam".into(),
        17 => "absorbance".into(),
        20 => "Kubelka-Munk".into(),
        22 => "V".into(),
        26 => "photoacoustic".into(),
        31 => "Raman intensity".into(),
        _ => format!("code {code}"),
    }
}
