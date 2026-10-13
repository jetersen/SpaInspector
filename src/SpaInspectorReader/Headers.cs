namespace SpaInspectorReader;

/// <summary>Compatibility view of the first primary series, with normalized endpoints.</summary>
public record Headers
{
    public uint UnitSize { get; init; }
    public uint StorageMode { get; init; }
    public uint XUnitCode { get; init; }
    public uint YUnitCode { get; init; }
    public string XUnits { get; init; } = "";
    public string XTitle { get; init; } = "";
    public string YUnit { get; init; } = "";
    public string YUnitTitle { get; init; } = "";
    public float FirstX { get; init; }
    public float LastX { get; init; }
    public uint NumberOfScan { get; init; }
    public uint NumberOfBackgroundScan { get; init; }
    public float DataSpacing => UnitSize < 2 ? 0 : (LastX - FirstX) / (UnitSize - 1);
    public float SignalStrength { get; set; }
    public float Gain { get; set; }
    public float OpticalVelocity { get; set; }

    internal static Headers FromMetadata(SpaMetadata? metadata) => metadata is null ? new() : new()
    {
        UnitSize = metadata.PointCount, StorageMode = metadata.StorageMode,
        XUnitCode = metadata.XUnitCode, YUnitCode = metadata.YUnitCode,
        XUnits = metadata.XUnit, XTitle = SpaUnits.XTitle(metadata.XUnitCode),
        YUnit = metadata.YUnit, YUnitTitle = SpaUnits.YTitle(metadata.YUnitCode),
        FirstX = metadata.FirstX, LastX = metadata.LastX,
        NumberOfScan = metadata.ScanCount ?? 0, NumberOfBackgroundScan = metadata.BackgroundScanCount ?? 0,
        SignalStrength = metadata.SignalStrength ?? 0, Gain = metadata.Gain ?? 0, OpticalVelocity = metadata.OpticalVelocity ?? 0
    };
}

public static class SpaUnits
{
    public static string XUnit(uint code) => code switch { 1 or 32 => "cm^-1", 2 => "points", 3 => "nm", 4 => "um", _ => $"code {code}" };
    public static string XTitle(uint code) => code switch { 1 => "Wavenumbers", 2 => "Data points", 3 or 4 => "Wavelengths", 32 => "Raman shift", _ => $"X unit {code}" };
    public static string YUnit(uint code) => code switch { 11 or 16 => "%", 17 => "absorbance", 20 => "Kubelka-Munk", 22 => "V", 12 => "log(1/R)", 15 => "single beam", 26 => "photoacoustic", 31 => "Raman intensity", _ => $"code {code}" };
    public static string YTitle(uint code) => code switch { 11 => "Reflectance", 12 => "Log(1/R)", 15 => "Single beam", 16 => "Transmittance", 17 => "Absorbance", 20 => "Kubelka-Munk", 22 => "Volts", 26 => "Photoacoustic", 31 => "Raman intensity", _ => $"Intensity (unit {code})" };
}
