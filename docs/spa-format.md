# SPA reader contract

All integers and float32 values below are little-endian. Offsets are byte offsets. Bounds, directory capacity, float alignment, and point counts are validated before decoding. Input bytes are never modified.

This contract combines the existing [SpaInspector](https://github.com/specshell/SpaInspector), public samples, and static tracing of OMNIC 9.2 native loaders and consumers using [REA](https://github.com/morluto/rea). The vendor installer is publicly hosted by [Caltech MMRC](https://mmrc.caltech.edu/FTIR/Software/). The analyzed `omnic_ap.dll` SHA-256 is `13b2df7cf9e542f30790ace25c7714f975500c8b2bf5e4392661fc6a5652eb32`. Vendor code was inspected statically; it was not executed to establish numerical equivalence. No vendor binaries or newly acquired samples are redistributed here.

## Container and lookup

| Fixed-header offset | Type | Meaning |
| --- | --- | --- |
| 0 | 21 bytes | `Spectral Data File\r\n\0`, including the NUL |
| 30 | 256 bytes | Title, decoded through the first NUL |
| 288 | uint16 | File type code |
| 290 | uint16 | Revision code; it is not a four-byte field |
| 292 | uint16 | Reserved directory allocation in 256-byte units |
| 294 | uint16 | Count of directory records |
| 296 | uint32 | Seconds since 1899-12-31 UTC |
| 304 | records | Counted 16-byte directory |

Each record contains `type:uint16` at +0, `offset:uint32` at +2, `length:uint32` at +6, `secondarySelector:uint16` at +10, and `tertiarySelector:uint32` at +12. Lookup uses the first exact `(type, secondary, tertiary)` match. All duplicates remain in `sections`; only the first matching header is decoded. The reserved directory must fit the file and contain the counted records. Nonempty sections must follow that reserved area.

The native allocation bound is 10,000 units. File type 1 is the supported classification established by the samples. Other classifications produce a warning, not a promise of SPG/series compatibility.

## Numerical sections

| Header type | Data type | Decoded series |
| --- | --- | --- |
| 2 | 3 | Spectrum |
| 100 | 102 | Saved sample interferogram |
| 101 | 103 | Saved background interferogram |
| 379 | 381 | Auxiliary interferogram, physical role unverified |
| 380 | 382 | Auxiliary interferogram, physical role unverified |

Header and data selectors must match exactly. Spectrum headers require the first 24 bytes: storage mode +0, count +4, X-unit code +8, Y-unit code +12, first stored X float32 +16, last stored X +20. Optional fields are read only when present.

For storage mode 1, normalized endpoints retain their stored order. Other modes swap the endpoints, matching the native fallback branch. In the ordinary clear file-object loading path, Y code 22 retains stored sample order; other spectrum Y codes reverse a copied array. A special native runtime object type 58 has another copy branch. That runtime type is not the fixed-header file type and is not reproduced by this reader.

Interferogram headers contain count +0 and optional scan count +4. Their samples retain stored order, X is the sample index from zero, and units are points/V. A positive laser-frequency float32 at +20 is exposed when present. Static reprocessing consumers also use +16 as a center/ZPD index; it remains available in the raw header rather than a validated typed field. The native short-header fill path does not establish every optional field's meaning.

Axes use double-precision interpolation between float32 endpoints. Zero-point series have no coordinates; one-point series uses the first endpoint. Non-finite sample values remain intact and are omitted from plotted paths. Non-finite endpoints are rejected.

## Metadata and confidence

| Section | Offset | Type | Export field | Evidence boundary |
| --- | --- | --- | --- | --- |
| 2 | 36 | uint32 | `scanCount` | Existing reader field |
| 2 | 52 | uint32 | `backgroundScanCount` | Existing reader field |
| 2 | 56 | float32 | `backgroundGain` | Native labelled converter |
| 2 | 68 | uint32 | `collectionDurationCentiseconds` | Native labelled conversion to seconds by division by 100 |
| 2 | 72 | float32 | `signalStrength` | Historical API label; physical meaning unresolved |
| 2 | 80 | float32 | `laserFrequencyCmInverse` | Native labelled converter and axis-adjustment consumer |
| 2 | 84 | float32 | `sampleSpacing` | Native label established; units unresolved |
| 2 | 96 | float32 | `ramanLaserFrequencyCmInverse` | Native labelled converter |
| 106 | 44 | float32 | `gain` | Native label: sample gain |
| 106 | 48 | float32 | `opticalVelocity` | Historical API label; physical meaning and units unresolved |
| 384 | 0 | uint32 | `processingAlgorithmCode` | Native ratio/background-processing switch; enumeration unresolved |

Matching selectors select sections 106 and 384. Fields absent from short headers are null. The second word of section 384 is copied to an internal field, but its consumers and meaning are unresolved. The remaining tail is not interpreted. Native fallback creates a zeroed 128-byte block when section 384 is absent; the reader preserves absence instead of inventing file contents.

Known X labels: 1/32 cm⁻¹, 2 points, 3 nm, 4 µm. Known Y labels: 11/16 %, 12 log(1/R), 15 single beam, 17 absorbance, 20 Kubelka-Munk, 22 V, 26 photoacoustic, 31 Raman intensity. Unmapped codes retain their full uint32 value and a `code N` label.

## Type 130 objects

Type 130 is a generic serialized-object container. Its secondary selector identifies an instance, not a fixed experiment/status/ATR role. The native serializer (`omnic_ap.dll` entry 0x20055e15) writes:

| Payload offset | Type | Meaning |
| --- | --- | --- |
| 0 | uint16 | Payload class ID |
| 2 | uint32 | Declared serialized body length |
| 6 | bytes | Body |
| 6 + body length | bytes | Remaining attachments/alignment bytes |

The tertiary selector's upper 16 bits cache the class when nonzero. Its lower 16 bits may cache the first body word, depending on the writer flag. These are exported as `cachedClassId` and `cachedBodyWord`; the latter is not a proven version number. A historical native getter falls back to the low word of body length when the cache is zero, so interpreting that API as a normal version field would be misleading.

The class cache takes precedence for identity. A cache/payload disagreement suppresses attribute interpretation and produces a warning. Invalid envelopes produce a warning and retain their original section. Trailing bytes are preserved in full: they can contain substantial nonzero attachments, not merely padding.

| Class | Role | Decoding boundary |
| --- | --- | --- |
| 121 | Experiment | Native 50-field fixed layout; selected bounded strings exposed |
| 158 | Instrument status candidate | Role inferred from sample text; body retained without guessed framing |
| 159 | Advanced ATR | Native seven-float layout; applied marker and four correction controls verified |
| 166 | Instrument specification | Native uint16 + float32 layout; physical meanings unknown |
| Other | `class_N` | Complete raw body and attachments retained |

Class 121 attributes use payload-relative offsets: ID 2 at 10, width 80, short experiment path; ID 3 at 90, width 64, inferred experiment title; ID 4 at 154, width 255, inferred description; ID 7 at 413, width 100, unnamed; ID 49 at 671, width 260, long experiment path. The native path synchronization consumers establish IDs 2/49. IDs 3/4 are labelled from sample contents, not a proven native semantic consumer. Only fields entirely inside the declared body are decoded.

Class 159 exposes seven float32 attributes at offsets 6 + 4×ID. IDs 0..4 are `applied`, `sampleRefractiveIndex`, `crystalRefractiveIndex`, `numberOfBounces`, and `angleOfIncidenceDegrees`, established by the native correction writer and controlled OMNIC saves. IDs 5/6 are retained unnamed; the observed writer sets them to zero. Class 166 exposes unnamed ID 0 as uint16 at 6 and ID 1 as float32 at 8. A value near 15,798 in class 166 alone does not prove that it is laser frequency.

## Text and export

Titles, history (type 27), and decoded object strings stop at their first NUL within their bounds. Auto uses valid UTF-8, otherwise Windows-1252 with a warning. Explicit UTF-8 is strict; Windows-1252 and Latin-1 can be selected. The file does not establish the original producer code page, so all original bytes are retained.

JSON schema version 1 is camelCase. Raw byte arrays use base64, including `sourceBytes`, section payloads, headers/settings, object bodies and attachments. Non-finite float values use `"NaN"`, `"Infinity"`, or `"-Infinity"`; original NaN payload bits remain in raw bytes. JSON preserves decoded values but is not a SPA writer. CSV exports all supported series with selectors and unit codes. Plots may select intervals or reverse their view without changing either document export.
