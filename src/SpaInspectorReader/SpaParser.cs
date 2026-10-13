using System.Buffers.Binary;
using System.Globalization;
using System.Text;

namespace SpaInspectorReader;

public static class SpaParser
{
    private static readonly DateTime Epoch = new(1899, 12, 31, 0, 0, 0, DateTimeKind.Utc);
    private static readonly UTF8Encoding StrictUtf8 = new(false, true);

    public static Spa Parse(ReadOnlySpan<byte> bytes, SpaReadOptions? options = null)
    {
        options ??= new();
        var directory = SpaDirectory.Parse(bytes);
        var warnings = new List<string>();
        if (directory.FileTypeCode != 1)
            warnings.Add($"File type code {directory.FileTypeCode} has an unvalidated container classification; only SPA type 1 ordinate semantics are implemented.");
        foreach (var duplicate in directory.Entries.GroupBy(section => (section.Type, section.SecondarySelector, section.TertiarySelector)).Where(group => group.Count() > 1))
            warnings.Add($"Duplicate section triple ({duplicate.Key.Type}, {duplicate.Key.SecondarySelector}, {duplicate.Key.TertiarySelector}); exact lookup uses the first record, and all records are preserved.");

        var titleBytes = TextBytes(directory.OriginalBytes.AsSpan(30, 256));
        var historySection = directory.Find(27);
        var historyBytes = historySection is null ? Array.Empty<byte>() : TextBytes(historySection.Payload);
        var title = DecodeText(titleBytes, options.TextEncoding, "Title", warnings);
        var history = DecodeText(historyBytes, options.TextEncoding, "History", warnings);
        var series = new List<SpaSeries>();
        foreach (var header in directory.Entries.Where(section => section.Type == 2)
                     .GroupBy(section => (section.SecondarySelector, section.TertiarySelector)).Select(group => group.First()))
        {
            var data = directory.Find(3, header.SecondarySelector, header.TertiarySelector)
                ?? throw new InvalidDataException($"SPA primary header section {header.Index} has no matching type 3 data.");
            var settings = directory.Find(106, header.SecondarySelector, header.TertiarySelector);
            var metadata = ReadPrimaryMetadata(header, settings, directory.Find(384, header.SecondarySelector, header.TertiarySelector));
            if (metadata.StorageMode is < 1 or > 3) warnings.Add($"Storage mode {metadata.StorageMode} is absent from the observed corpus; the recovered native float32 fallback is used, and the numeric code is preserved.");
            var values = ReadCountedFloats(data, metadata.PointCount);
            if (metadata.YUnitCode != 22) Array.Reverse(values);
            AddNumericWarnings(metadata, values, warnings);
            series.Add(new SpaSeries
            {
                Kind = "spectrum", Name = title.Text,
                SecondarySelector = header.SecondarySelector, TertiarySelector = header.TertiarySelector,
                Metadata = metadata, X = Coordinates(metadata.FirstX, metadata.LastX, values.Length), Y = values
            });
        }
        foreach (var data in directory.Entries.Where(section => section.Type == 3))
            if (directory.Find(2, data.SecondarySelector, data.TertiarySelector) is null)
                throw new InvalidDataException($"SPA primary data section {data.Index} has no matching type 2 header.");

        AddInterferograms(directory, 102, 100, "sample_interferogram", "Sample interferogram", series, warnings);
        AddInterferograms(directory, 103, 101, "background_interferogram", "Background interferogram", series, warnings);
        AddInterferograms(directory, 381, 379, "auxiliary_interferogram_381", "Auxiliary interferogram 381", series, warnings);
        AddInterferograms(directory, 382, 380, "auxiliary_interferogram_382", "Auxiliary interferogram 382", series, warnings);

        if (series.Count == 0) warnings.Add("No supported spectrum or interferogram was found; original data is retained in JSON.");
        var embeddedObjects = SpaEmbeddedObjectDecoder.Decode(directory.Entries, options.TextEncoding, warnings);
        var timestamp = Epoch.AddSeconds(directory.RawTimestamp);
        return new Spa
        {
            FileTypeCode = directory.FileTypeCode, RevisionCode = directory.RevisionCandidate,
            DirectoryAllocationUnits = directory.DirectoryAllocationUnits,
            Title = title.Text, TitleBytes = titleBytes, TitleEncoding = title.Encoding,
            History = history.Text, HistoryBytes = historyBytes, HistoryEncoding = history.Encoding,
            TimestampSeconds = directory.RawTimestamp,
            TimestampUtc = timestamp.ToString("yyyy-MM-dd'T'HH:mm:ss'Z'", CultureInfo.InvariantCulture),
            DateTime = timestamp, TextEncoding = EncodingName(options.TextEncoding),
            SourceBytes = directory.OriginalBytes, Sections = directory.Entries,
            Series = series.AsReadOnly(), EmbeddedObjects = embeddedObjects, Warnings = warnings.AsReadOnly()
        };
    }

    private static SpaMetadata ReadPrimaryMetadata(SpaSection header, SpaSection? settings, SpaSection? processing)
    {
        var bytes = header.Payload;
        if (bytes.Length < 24)
            throw new InvalidDataException($"SPA header section {header.Index} is shorter than 24 bytes.");
        var mode = UInt32(bytes, 0);
        var xcode = UInt32(bytes, 8);
        var ycode = UInt32(bytes, 12);
        var storedFirst = Float32(bytes, 16);
        var storedLast = Float32(bytes, 20);
        if (!float.IsFinite(storedFirst) || !float.IsFinite(storedLast))
            throw new InvalidDataException($"SPA header section {header.Index} contains non-finite X endpoints.");
        return new SpaMetadata
        {
            StorageMode = mode, PointCount = UInt32(bytes, 4), XUnitCode = xcode, YUnitCode = ycode,
            XUnit = SpaUnits.XUnit(xcode), YUnit = SpaUnits.YUnit(ycode),
            StoredFirstX = storedFirst, StoredLastX = storedLast,
            FirstX = mode == 1 ? storedFirst : storedLast, LastX = mode == 1 ? storedLast : storedFirst,
            ScanCount = OptionalUInt32(bytes, 36), BackgroundScanCount = OptionalUInt32(bytes, 52),
            SignalStrength = OptionalFloat32(bytes, 72),
            LaserFrequencyCmInverse = OptionalFloat32(bytes, 80),
            BackgroundGain = OptionalFloat32(bytes, 56),
            CollectionDurationCentiseconds = OptionalUInt32(bytes, 68),
            SampleSpacing = OptionalFloat32(bytes, 84),
            RamanLaserFrequencyCmInverse = OptionalFloat32(bytes, 96),
            ProcessingAlgorithmCode = processing is null ? null : OptionalUInt32(processing.Payload, 0),
            Gain = settings is null ? null : OptionalFloat32(settings.Payload, 44),
            OpticalVelocity = settings is null ? null : OptionalFloat32(settings.Payload, 48),
            RawHeader = bytes.ToArray(), RawSettings = settings?.Payload.ToArray() ?? Array.Empty<byte>()
        };
    }

    private static void AddInterferograms(SpaDirectory directory, ushort dataType, ushort headerType,
        string kind, string name, List<SpaSeries> series, List<string> warnings)
    {
        foreach (var data in directory.Entries.Where(section => section.Type == dataType)
                     .GroupBy(section => (section.SecondarySelector, section.TertiarySelector)).Select(group => group.First()))
        {
            var header = directory.Find(headerType, data.SecondarySelector, data.TertiarySelector)
                ?? throw new InvalidDataException($"Interferogram data section {data.Index} has no matching type {headerType} metadata.");
            if (header.Payload.Length < 4)
                throw new InvalidDataException($"Interferogram header section {header.Index} does not contain a complete point count.");
            var count = UInt32(header.Payload, 0);
            var values = ReadCountedFloats(data, count);
            var last = values.Length == 0 ? 0 : values.Length - 1;
            var metadata = new SpaMetadata
            {
                StorageMode = 1, PointCount = count, XUnitCode = 2, YUnitCode = 22, XUnit = "points", YUnit = "V",
                StoredFirstX = 0, StoredLastX = last, FirstX = 0, LastX = last,
                ScanCount = OptionalUInt32(header.Payload, 4),
                LaserFrequencyCmInverse = OptionalPositiveFloat32(header.Payload, 20),
                RawHeader = header.Payload.ToArray(),
                RawSettings = Array.Empty<byte>()
            };
            if (dataType is 381 or 382) warnings.Add($"Type {dataType} interferogram role is unverified; its stored order and bytes are preserved.");
            AddNumericWarnings(metadata, values, warnings);
            series.Add(new SpaSeries
            {
                Kind = kind, Name = name, SecondarySelector = data.SecondarySelector, TertiarySelector = data.TertiarySelector,
                Metadata = metadata, X = Coordinates(0, last, values.Length), Y = values
            });
        }
        foreach (var header in directory.Entries.Where(section => section.Type == headerType))
            if (directory.Find(dataType, header.SecondarySelector, header.TertiarySelector) is null)
                warnings.Add($"Interferogram header section {header.Index} has no matching type {dataType} data; its bytes are preserved.");
    }

    private static float[] ReadCountedFloats(SpaSection data, uint expectedCount)
    {
        if (data.Length % 4 != 0)
            throw new InvalidDataException($"SPA data section {data.Index} has an incomplete float32 value.");
        if (data.Length / 4 != expectedCount)
            throw new InvalidDataException($"SPA data section {data.Index} contains {data.Length / 4} float32 values, but its header declares {expectedCount}.");
        return SpaDirectory.ReadFloat32(data);
    }

    private static double[] Coordinates(double first, double last, int count)
    {
        var result = new double[count];
        if (count == 0) return result;
        result[0] = first;
        if (count == 1) return result;
        var spacing = (last - first) / (count - 1);
        for (var index = 1; index < count - 1; index++) result[index] = first + index * spacing;
        result[^1] = last;
        return result;
    }

    private static void AddNumericWarnings(SpaMetadata metadata, float[] values, List<string> warnings)
    {
        if (!float.IsFinite(metadata.FirstX) || !float.IsFinite(metadata.LastX))
            warnings.Add("A signal contains non-finite X endpoints; JSON exports use named values and plotting must omit non-finite coordinates.");
        if (values.Any(value => !float.IsFinite(value)))
            warnings.Add("A signal contains non-finite ordinates; JSON exports use NaN/Infinity strings without losing these values.");
        if (metadata.PointCount == 1 && metadata.FirstX != metadata.LastX)
            warnings.Add("A one-point signal declares different endpoints; its sole X coordinate uses the normalized first endpoint.");
    }

    private static byte[] TextBytes(ReadOnlySpan<byte> bytes)
    {
        var terminator = bytes.IndexOf((byte)0);
        return (terminator < 0 ? bytes : bytes[..terminator]).ToArray();
    }

    internal static (string Text, string Encoding) DecodeText(byte[] bytes, SpaTextEncoding requested, string field, List<string> warnings)
    {
        try
        {
            if (requested is SpaTextEncoding.Auto or SpaTextEncoding.Utf8)
            {
                try { return (StrictUtf8.GetString(bytes), "utf-8"); }
                catch (DecoderFallbackException) when (requested == SpaTextEncoding.Auto)
                {
                    warnings.Add($"{field} is not valid UTF-8; Windows-1252 was selected as a fallback. The producer code page is unverified; choose a text encoding if needed.");
                }
            }
            if (requested == SpaTextEncoding.Latin1) return (Encoding.Latin1.GetString(bytes), "latin1");
            Encoding.RegisterProvider(CodePagesEncodingProvider.Instance);
            return (Encoding.GetEncoding(1252).GetString(bytes), "windows-1252");
        }
        catch (DecoderFallbackException exception)
        {
            throw new InvalidDataException($"{field} cannot be decoded as the requested UTF-8 encoding.", exception);
        }
    }

    internal static string EncodingName(SpaTextEncoding encoding) => encoding switch
    {
        SpaTextEncoding.Auto => "auto", SpaTextEncoding.Utf8 => "utf-8", SpaTextEncoding.Windows1252 => "windows-1252", SpaTextEncoding.Latin1 => "latin1",
        _ => throw new ArgumentOutOfRangeException(nameof(encoding))
    };

    private static uint UInt32(byte[] bytes, int offset) => BinaryPrimitives.ReadUInt32LittleEndian(bytes.AsSpan(offset, 4));
    private static float Float32(byte[] bytes, int offset) => BinaryPrimitives.ReadSingleLittleEndian(bytes.AsSpan(offset, 4));
    private static uint? OptionalUInt32(byte[] bytes, int offset) => bytes.Length - offset >= 4 ? UInt32(bytes, offset) : null;
    private static float? OptionalPositiveFloat32(byte[] bytes, int offset)
    {
        var value = OptionalFloat32(bytes, offset);
        return value > 0 ? value : null;
    }

    private static float? OptionalFloat32(byte[] bytes, int offset) => bytes.Length - offset >= 4 ? Float32(bytes, offset) : null;
}
