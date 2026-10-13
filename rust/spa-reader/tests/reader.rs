use base64::{Engine, engine::general_purpose::STANDARD};
use spa_reader::{ReadOptions, TextEncoding, parse};

fn put16(bytes: &mut [u8], position: usize, value: u16) {
    bytes[position..position + 2].copy_from_slice(&value.to_le_bytes());
}
fn put32(bytes: &mut [u8], position: usize, value: u32) {
    bytes[position..position + 4].copy_from_slice(&value.to_le_bytes());
}
fn float_bytes(values: &[f32]) -> Vec<u8> {
    values
        .iter()
        .flat_map(|value| value.to_le_bytes())
        .collect()
}
fn metadata(mode: u32, count: u32, x_code: u32, y_code: u32, first: f32, last: f32) -> Vec<u8> {
    let mut header = vec![0; 140];
    for (offset, value) in [
        (0, mode),
        (4, count),
        (8, x_code),
        (12, y_code),
        (16, first.to_bits()),
        (20, last.to_bits()),
    ] {
        put32(&mut header, offset, value);
    }
    header
}
fn container(sections: Vec<(u16, u16, u32, Vec<u8>)>) -> Vec<u8> {
    let units = sections.len().div_ceil(16).max(1);
    let mut bytes = vec![0; 304 + units * 256];
    let magic = b"Spectral Data File\r\n\0";
    bytes[..magic.len()].copy_from_slice(magic);
    bytes[30..35].copy_from_slice(b"Probe");
    put16(&mut bytes, 0x120, 1);
    put16(&mut bytes, 0x122, 521);
    put16(&mut bytes, 0x124, units as u16);
    put16(&mut bytes, 0x126, sections.len() as u16);
    for (index, (kind, secondary, tertiary, payload)) in sections.into_iter().enumerate() {
        let offset = bytes.len() as u32;
        let p = 304 + index * 16;
        put16(&mut bytes, p, kind);
        put32(&mut bytes, p + 2, offset);
        put32(&mut bytes, p + 6, payload.len() as u32);
        put16(&mut bytes, p + 10, secondary);
        put32(&mut bytes, p + 12, tertiary);
        bytes.extend(payload);
    }
    bytes
}
fn simple(mode: u32, y_code: u32, values: &[f32]) -> Vec<u8> {
    container(vec![
        (
            2,
            0,
            0,
            metadata(mode, values.len() as u32, 1, y_code, 4000.0, 700.0),
        ),
        (3, 0, 0, float_bytes(values)),
    ])
}

#[test]
fn repository_fixture_is_unchanged_and_reads_repeatably() {
    let bytes = include_bytes!("../../../tests/TestFiles/test1.spa");
    let first = parse(bytes, &ReadOptions::default()).unwrap();
    let second = parse(bytes, &ReadOptions::default()).unwrap();
    assert_eq!(first.source_bytes, bytes);
    assert_eq!(first.series[0].y, second.series[0].y);
    assert_eq!(first.series[0].y.len(), 6846);
    assert_eq!(first.series[0].y[0], 0.03549831);
    assert_eq!(*first.series[0].y.last().unwrap(), 0.009505022);
    assert_eq!(first.timestamp_utc, "2021-05-06T11:13:32Z");
    assert_eq!(first.sections[1].type_code, 384);
}

#[test]
fn sample_and_background_interferograms_preserve_stored_order() {
    let bytes = include_bytes!("../../../tests/TestFiles/test1.spa");
    let document = parse(bytes, &ReadOptions::default()).unwrap();
    let sample = document
        .series
        .iter()
        .find(|series| series.kind == "sample_interferogram")
        .unwrap();
    let background = document
        .series
        .iter()
        .find(|series| series.kind == "background_interferogram")
        .unwrap();
    assert_eq!(sample.y[0], -1.9441359E-05);
    assert_eq!(sample.y[sample.y.len() - 1], -0.0009917123);
    assert_eq!(background.y[0], 0.00012780126);
    assert_eq!(background.y[background.y.len() - 1], -0.00088237826);
    assert_eq!(sample.x[0], 0.0);
    assert_eq!(sample.x[sample.x.len() - 1], 12199.0);
}

#[test]
fn ordinary_spectra_reverse_but_volts_are_forward_and_axes_follow_storage_mode() {
    let spectrum = parse(&simple(3, 17, &[10.0, 20.0]), &ReadOptions::default()).unwrap();
    assert_eq!(spectrum.series[0].y, [20.0, 10.0]);
    assert_eq!(spectrum.series[0].x, [700.0, 4000.0]);
    let volts = parse(&simple(1, 22, &[10.0, 20.0]), &ReadOptions::default()).unwrap();
    assert_eq!(volts.series[0].y, [10.0, 20.0]);
    assert_eq!(volts.series[0].x, [4000.0, 700.0]);
}

#[test]
fn integer_codes_are_not_truncated_and_unknown_sections_are_preserved() {
    let bytes = container(vec![
        (2, 7, 1234, metadata(3, 1, 513, 529, 20.0, 10.0)),
        (3, 7, 1234, float_bytes(&[42.0])),
        (384, 9, 456, vec![1, 2, 3, 4, 5]),
    ]);
    let document = parse(&bytes, &ReadOptions::default()).unwrap();
    assert_eq!(document.series[0].metadata.x_unit_code, 513);
    assert_eq!(document.series[0].metadata.y_unit_code, 529);
    assert_eq!(document.series[0].secondary_selector, 7);
    assert_eq!(document.series[0].tertiary_selector, 1234);
    assert_eq!(document.sections[2].payload, [1, 2, 3, 4, 5]);
    let json: serde_json::Value = serde_json::from_str(&document.to_json().unwrap()).unwrap();
    assert_eq!(
        STANDARD
            .decode(json["sourceBytes"].as_str().unwrap())
            .unwrap(),
        bytes
    );
    assert_eq!(json["sections"][2]["type"], 384);
    assert_eq!(
        STANDARD
            .decode(json["sections"][2]["payload"].as_str().unwrap())
            .unwrap(),
        [1, 2, 3, 4, 5]
    );
}

#[test]
fn duplicate_lookup_uses_first_exact_triple_without_losing_entries() {
    let bytes = container(vec![
        (2, 2, 9, metadata(1, 1, 2, 22, 0.0, 0.0)),
        (3, 2, 9, float_bytes(&[10.0])),
        (3, 2, 9, float_bytes(&[20.0])),
        (2, 3, 9, metadata(1, 1, 2, 22, 0.0, 0.0)),
        (3, 3, 9, float_bytes(&[30.0])),
    ]);
    let document = parse(&bytes, &ReadOptions::default()).unwrap();
    assert_eq!(document.sections.len(), 5);
    assert_eq!(document.series.len(), 2);
    assert_eq!(document.series[0].y, [10.0]);
    assert_eq!(document.series[1].y, [30.0]);
    assert!(
        document
            .warnings
            .iter()
            .any(|warning| warning.contains("Duplicate"))
    );
}

#[test]
fn legacy_history_is_bounded_raw_and_user_selectable() {
    let bytes = container(vec![
        (2, 0, 0, metadata(1, 1, 2, 22, 0.0, 0.0)),
        (3, 0, 0, float_bytes(&[1.0])),
        (27, 0, 0, vec![b'c', b'a', b'f', 0xe9, 0, 0xff]),
    ]);
    let auto = parse(&bytes, &ReadOptions::default()).unwrap();
    assert_eq!(auto.history, "café");
    assert_eq!(auto.history_bytes, [b'c', b'a', b'f', 0xe9]);
    assert_eq!(auto.sections[2].payload, [b'c', b'a', b'f', 0xe9, 0, 0xff]);
    assert_eq!(auto.history_encoding, "windows-1252");
    assert!(
        parse(
            &bytes,
            &ReadOptions {
                text_encoding: TextEncoding::Utf8
            }
        )
        .is_err()
    );
    assert_eq!(
        parse(
            &bytes,
            &ReadOptions {
                text_encoding: TextEncoding::Latin1
            }
        )
        .unwrap()
        .history,
        "café"
    );
}

#[test]
fn malformed_files_return_errors_instead_of_partial_samples_or_panics() {
    assert_eq!(
        parse(&[], &ReadOptions::default()).unwrap_err().kind,
        "truncated_header"
    );
    let mut bytes = simple(3, 17, &[1.0, 2.0]);
    bytes[0] = b'X';
    assert_eq!(
        parse(&bytes, &ReadOptions::default()).unwrap_err().kind,
        "invalid_magic"
    );
    let mut bytes = simple(3, 17, &[1.0, 2.0]);
    bytes[20] = b'X';
    assert_eq!(
        parse(&bytes, &ReadOptions::default()).unwrap_err().kind,
        "invalid_magic"
    );
    let mut bytes = simple(3, 17, &[1.0, 2.0]);
    put32(&mut bytes, 304 + 16 + 6, 7);
    assert_eq!(
        parse(&bytes, &ReadOptions::default()).unwrap_err().kind,
        "invalid_samples"
    );
    let mut bytes = simple(3, 17, &[1.0, 2.0]);
    put32(&mut bytes, 304 + 16 + 2, u32::MAX);
    assert_eq!(
        parse(&bytes, &ReadOptions::default()).unwrap_err().kind,
        "invalid_range"
    );
    let mut bytes = simple(3, 17, &[1.0, 2.0]);
    put16(&mut bytes, 0x126, 17);
    assert_eq!(
        parse(&bytes, &ReadOptions::default()).unwrap_err().kind,
        "invalid_directory"
    );
    let mut bytes = simple(3, 17, &[1.0, 2.0]);
    let metadata_position = u32::from_le_bytes(bytes[306..310].try_into().unwrap()) as usize;
    put32(&mut bytes, metadata_position + 4, 3);
    assert_eq!(
        parse(&bytes, &ReadOptions::default()).unwrap_err().kind,
        "point_count_mismatch"
    );
}

#[test]
fn zero_and_single_point_axes_and_nonfinite_exports_are_defined() {
    assert!(
        parse(&simple(1, 22, &[]), &ReadOptions::default())
            .unwrap()
            .series[0]
            .x
            .is_empty()
    );
    let document = parse(&simple(1, 22, &[f32::NAN]), &ReadOptions::default()).unwrap();
    assert_eq!(document.series[0].x, [4000.0]);
    let json: serde_json::Value = serde_json::from_str(&document.to_json().unwrap()).unwrap();
    assert_eq!(json["series"][0]["y"][0], "NaN");
    assert!(document.to_csv().contains(",4000,NaN,1,22\n"));
    let document = parse(
        &simple(1, 22, &[f32::INFINITY, f32::NEG_INFINITY]),
        &ReadOptions::default(),
    )
    .unwrap();
    assert!(document.to_csv().contains(",Infinity,1,22\n"));
    assert!(document.to_csv().contains(",-Infinity,1,22\n"));
}

#[test]
fn laser_frequency_uses_its_proven_header_field() {
    let mut header = metadata(1, 1, 2, 22, 0.0, 0.0);
    put32(&mut header, 80, 15798.0_f32.to_bits());
    put32(&mut header, 84, 123.0_f32.to_bits());
    let bytes = container(vec![(2, 0, 0, header), (3, 0, 0, float_bytes(&[1.0]))]);
    let document = parse(&bytes, &ReadOptions::default()).unwrap();
    assert_eq!(
        document.series[0].metadata.laser_frequency_cm_inverse,
        Some(15798.0)
    );
}

#[test]
fn external_corpus_when_requested() {
    let Some(root) = std::env::var_os("SPA_CORPUS_DIR") else {
        return;
    };
    let mut queue = vec![std::path::PathBuf::from(root)];
    let mut count = 0;
    while let Some(path) = queue.pop() {
        for entry in std::fs::read_dir(path).unwrap() {
            let entry = entry.unwrap();
            if entry.file_type().unwrap().is_dir() {
                queue.push(entry.path());
            } else if entry
                .path()
                .extension()
                .is_some_and(|extension| extension.to_string_lossy().eq_ignore_ascii_case("spa"))
            {
                let bytes = std::fs::read(entry.path()).unwrap();
                let document = parse(&bytes, &ReadOptions::default())
                    .unwrap_or_else(|error| panic!("{}: {error}", entry.path().display()));
                assert_eq!(document.source_bytes, bytes);
                for series in &document.series {
                    assert_eq!(series.x.len(), series.y.len());
                    assert_eq!(series.y.len(), series.metadata.point_count as usize);
                }
                document.to_json().unwrap();
                count += 1;
            }
        }
    }
    assert!(count > 0, "SPA_CORPUS_DIR did not contain any SPA files");
    eprintln!("Validated {count} external SPA files");
}

fn object_payload(class: u16, body: &[u8], trailing: &[u8]) -> Vec<u8> {
    let mut payload = vec![0; 6];
    put16(&mut payload, 0, class);
    put32(&mut payload, 2, body.len() as u32);
    payload.extend_from_slice(body);
    payload.extend_from_slice(trailing);
    payload
}

#[test]
fn typed_metadata_does_not_confuse_neighboring_fields_or_selectors() {
    let mut header = metadata(1, 1, 2, 22, 0.0, 0.0);
    put32(&mut header, 56, 4.0_f32.to_bits());
    put32(&mut header, 68, 12345);
    put32(&mut header, 80, 15798.0_f32.to_bits());
    put32(&mut header, 84, 2.0_f32.to_bits());
    put32(&mut header, 96, 18797.0_f32.to_bits());
    let bytes = container(vec![
        (2, 7, 8, header),
        (3, 7, 8, float_bytes(&[1.0])),
        (384, 0, 0, 99u32.to_le_bytes().to_vec()),
        (384, 7, 8, 1001u32.to_le_bytes().to_vec()),
    ]);
    let document = parse(&bytes, &ReadOptions::default()).unwrap();
    let m = &document.series[0].metadata;
    assert_eq!(m.background_gain, Some(4.0));
    assert_eq!(m.collection_duration_centiseconds, Some(12345));
    assert_eq!(m.laser_frequency_cm_inverse, Some(15798.0));
    assert_eq!(m.sample_spacing, Some(2.0));
    assert_eq!(m.raman_laser_frequency_cm_inverse, Some(18797.0));
    assert_eq!(m.processing_algorithm_code, Some(1001));
}

#[test]
fn object_envelope_preserves_attachments_and_names_only_verified_atr_fields() {
    let body = float_bytes(&[1.0, 1.36, 2.4, 1.0, 45.0, f32::NAN, f32::INFINITY]);
    let attachment = [0xAB; 336];
    let bytes = container(vec![(
        130,
        19,
        (159u32 << 16) | 42,
        object_payload(159, &body, &attachment),
    )]);
    let document = parse(&bytes, &ReadOptions::default()).unwrap();
    let object = &document.embedded_objects[0];
    assert_eq!(object.secondary_selector, 19);
    assert_eq!(object.cached_class_id, Some(159));
    assert_eq!(object.cached_body_word, Some(42));
    assert_eq!(object.body, body);
    assert_eq!(object.trailing_bytes, attachment);
    assert_eq!(object.attributes.len(), 7);
    assert_eq!(
        object
            .attributes
            .iter()
            .map(|a| a.name.as_deref())
            .collect::<Vec<_>>(),
        vec![
            Some("applied"),
            Some("sampleRefractiveIndex"),
            Some("crystalRefractiveIndex"),
            Some("numberOfBounces"),
            Some("angleOfIncidenceDegrees"),
            None,
            None
        ]
    );
    assert_eq!(object.attributes[5].value, "NaN");
    assert_eq!(object.attributes[6].value, "Infinity");
    assert_eq!(object.attributes[6].offset, 30);
}

#[test]
fn object_cache_disagreement_and_invalid_lengths_keep_raw_data() {
    let mut invalid = object_payload(121, &[], &[1, 2]);
    put32(&mut invalid, 2, u32::MAX);
    let bytes = container(vec![
        (
            130,
            0,
            158u32 << 16,
            object_payload(159, &float_bytes(&[1.0; 7]), &[9, 8]),
        ),
        (130, 1, 0, invalid.clone()),
        (130, 2, 0, vec![1, 2, 3]),
    ]);
    let document = parse(&bytes, &ReadOptions::default()).unwrap();
    assert_eq!(document.embedded_objects.len(), 1);
    assert_eq!(document.embedded_objects[0].class_id, 158);
    assert!(document.embedded_objects[0].attributes.is_empty());
    assert_eq!(document.sections[1].payload, invalid);
    assert!(document.warnings.len() >= 3);
}

#[test]
fn experiment_strings_are_bounded_by_declared_body_not_attachment() {
    let mut body = vec![0; 148];
    body[4..13].copy_from_slice(b"short.exp");
    body[84..93].copy_from_slice(b"New title");
    let mut attachment = vec![0; 300];
    attachment[..11].copy_from_slice(b"Not a field");
    let document = parse(
        &container(vec![(130, 4, 0, object_payload(121, &body, &attachment))]),
        &ReadOptions::default(),
    )
    .unwrap();
    let object = &document.embedded_objects[0];
    assert_eq!(object.attributes.len(), 2);
    assert_eq!(object.attributes[0].value, "short.exp");
    assert_eq!(object.attributes[1].value, "New title");
    assert_eq!(object.trailing_bytes, attachment);
}
