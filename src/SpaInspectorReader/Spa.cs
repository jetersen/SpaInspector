using System.Text.Json.Serialization;

namespace SpaInspectorReader;

public enum SpaTextEncoding { Auto, Utf8, Windows1252, Latin1 }

public sealed record SpaReadOptions
{
    public SpaTextEncoding TextEncoding { get; init; } = SpaTextEncoding.Auto;
}

public sealed record SpaMetadata
{
    public uint StorageMode { get; init; }
    public uint PointCount { get; init; }
    public uint XUnitCode { get; init; }
    public uint YUnitCode { get; init; }
    public string XUnit { get; init; } = "";
    public string YUnit { get; init; } = "";
    public float StoredFirstX { get; init; }
    public float StoredLastX { get; init; }
    public float FirstX { get; init; }
    public float LastX { get; init; }
    public uint? ScanCount { get; init; }
    public uint? BackgroundScanCount { get; init; }
    public uint? CollectionDurationCentiseconds { get; init; }
    public float? SignalStrength { get; init; }
    public float? Gain { get; init; }
    public float? OpticalVelocity { get; init; }
    public float? LaserFrequencyCmInverse { get; init; }
    public float? BackgroundGain { get; init; }
    public float? SampleSpacing { get; init; }
    public float? RamanLaserFrequencyCmInverse { get; init; }
    public uint? ProcessingAlgorithmCode { get; init; }
    public byte[] RawHeader { get; init; } = Array.Empty<byte>();
    public byte[] RawSettings { get; init; } = Array.Empty<byte>();
}

public sealed record SpaSeries
{
    public string Kind { get; init; } = "spectrum";
    public string Name { get; init; } = "";
    public ushort SecondarySelector { get; init; }
    public uint TertiarySelector { get; init; }
    public SpaMetadata Metadata { get; init; } = new();
    public double[] X { get; init; } = Array.Empty<double>();
    public float[] Y { get; init; } = Array.Empty<float>();
}

/// <summary>Decoded signals alongside the complete original container and raw metadata.</summary>
public class Spa
{
    public int SchemaVersion { get; init; } = 1;
    public ushort FileTypeCode { get; init; }
    public ushort RevisionCode { get; init; }
    public ushort DirectoryAllocationUnits { get; init; }
    public string Title { get; set; } = "";
    public byte[] TitleBytes { get; init; } = Array.Empty<byte>();
    public string TitleEncoding { get; init; } = "utf-8";
    public string History { get; set; } = "";
    public byte[] HistoryBytes { get; init; } = Array.Empty<byte>();
    public string HistoryEncoding { get; init; } = "utf-8";
    public uint TimestampSeconds { get; init; }
    public string TimestampUtc { get; init; } = "";
    public string TextEncoding { get; init; } = "auto";
    public byte[] SourceBytes { get; init; } = Array.Empty<byte>();
    public IReadOnlyList<SpaSection> Sections { get; init; } = Array.Empty<SpaSection>();
    public IReadOnlyList<SpaSeries> Series { get; init; } = Array.Empty<SpaSeries>();
    public IReadOnlyList<SpaEmbeddedObject> EmbeddedObjects { get; init; } = Array.Empty<SpaEmbeddedObject>();
    public IReadOnlyList<string> Warnings { get; init; } = Array.Empty<string>();

    [JsonIgnore] public SpaSeries? Spectrum => Series.FirstOrDefault(series => series.Kind == "spectrum");
    [JsonIgnore] public SpaSeries? SampleInterferogram => Series.FirstOrDefault(series => series.Kind == "sample_interferogram");
    [JsonIgnore] public SpaSeries? BackgroundSignal => Series.FirstOrDefault(series => series.Kind == "background_interferogram");

    private Headers? _headers;
    private float[]? _unitIntensities;
    private float[]? _unitInterferogram;
    private float[]? _backgroundInterferogram;
    [JsonIgnore] public DateTime DateTime { get; set; }
    [JsonIgnore] public Headers Headers { get => _headers ?? Headers.FromMetadata(Spectrum?.Metadata); set => _headers = value; }
    [JsonIgnore] public float[] UnitIntensities { get => _unitIntensities ?? Spectrum?.Y ?? Array.Empty<float>(); set => _unitIntensities = value; }
    [JsonIgnore] public float[] UnitInterferogram { get => _unitInterferogram ?? SampleInterferogram?.Y ?? Array.Empty<float>(); set => _unitInterferogram = value; }
    [JsonIgnore] public float[] BackgroundInterferogram { get => _backgroundInterferogram ?? BackgroundSignal?.Y ?? Array.Empty<float>(); set => _backgroundInterferogram = value; }

    public string ToCsv() => SpaExport.ToCsv(this);
    public string ToJson() => SpaExport.ToJson(this);
}
