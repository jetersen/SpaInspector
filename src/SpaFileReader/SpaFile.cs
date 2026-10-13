using System.Buffers.Binary;
using SpaInspectorReader;

namespace SpaFileReader;

/// <summary>Compatibility helpers. Reads return copies; only explicit writes modify the destination.</summary>
[PublicAPI]
public static class SpaFile
{
    public static Span<byte> ReadYUnitAsSpanByte(byte[] bytes) => ReadYUnitAsSpanByte(bytes.AsSpan());
    public static Span<byte> ReadYUnitAsSpanByte(ref Span<byte> bytes) => ReadYUnitAsSpanByte(bytes);
    private static Span<byte> ReadYUnitAsSpanByte(Span<byte> bytes)
    {
        _ = SpaParser.Parse(bytes);
        return SpaDirectory.Parse(bytes).Find(3)?.Payload.ToArray() ?? Array.Empty<byte>();
    }

    public static Span<float> ReadYUnitAsSpanFloat(byte[] bytes) => ReadYUnitAsFloatArray(bytes);
    public static Span<float> ReadYUnitAsSpanFloat(ref Span<byte> bytes) => PrimaryValues(bytes);
    public static Span<float> ReadYUnitAsSpanFloat(string file) => ReadYUnitAsFloatArray(file);
    public static float[] ReadYUnitAsFloatArray(byte[] bytes) => PrimaryValues(bytes);
    public static float[] ReadYUnitAsFloatArray(string file) => ReadYUnitAsFloatArray(File.ReadAllBytes(file));
    public static async Task<float[]> ReadYUnitAsFloatArrayAsync(string file) => ReadYUnitAsFloatArray(await File.ReadAllBytesAsync(file));

    public static Span<double> ReadYUnitAsSpanDouble(byte[] bytes) => ReadYUnitAsDoubleArray(bytes);
    public static Span<double> ReadYUnitAsSpanDouble(ref Span<byte> bytes) => PrimaryValues(bytes).Select(value => (double)value).ToArray();
    public static Span<double> ReadYUnitAsSpanDouble(string file) => ReadYUnitAsDoubleArray(file);
    public static double[] ReadYUnitAsDoubleArray(byte[] bytes) => PrimaryValues(bytes).Select(value => (double)value).ToArray();
    public static double[] ReadYUnitAsDoubleArray(ref Span<byte> bytes) => PrimaryValues(bytes).Select(value => (double)value).ToArray();
    public static double[] ReadYUnitAsDoubleArray(string file) => ReadYUnitAsDoubleArray(File.ReadAllBytes(file));
    public static async Task<double[]> ReadYUnitAsDoubleArrayAsync(string file) => ReadYUnitAsDoubleArray(await File.ReadAllBytesAsync(file));

    public static Span<float> ReadInterferogramAsSpanFloat(byte[] bytes) => InterferogramValues(bytes, "sample_interferogram");
    public static Span<float> ReadInterferogramAsSpanFloat(ref Span<byte> bytes) => InterferogramValues(bytes, "sample_interferogram");
    public static Span<float> ReadBackgroundInterogramAsSpanFloat(byte[] bytes) => InterferogramValues(bytes, "background_interferogram");
    public static Span<float> ReadBackgroundInterogramAsSpanFloat(ref Span<byte> bytes) => InterferogramValues(bytes, "background_interferogram");
    public static Span<byte> ReadInterferogramAsSpanByte(ref Span<byte> bytes) => RawInterferogram(bytes, 102);
    public static Span<byte> ReadBackgroundInterferogramAsSpanByte(ref Span<byte> bytes) => RawInterferogram(bytes, 103);

    private static float[] PrimaryValues(ReadOnlySpan<byte> bytes) => SpaParser.Parse(bytes).Series
        .FirstOrDefault(series => series.Kind == "spectrum" && series.SecondarySelector == 0 && series.TertiarySelector == 0)?.Y.ToArray() ?? Array.Empty<float>();

    private static float[] InterferogramValues(ReadOnlySpan<byte> bytes, string kind) => SpaParser.Parse(bytes).Series
        .FirstOrDefault(series => series.Kind == kind && series.SecondarySelector == 0 && series.TertiarySelector == 0)?.Y.ToArray() ?? Array.Empty<float>();

    private static byte[] RawInterferogram(ReadOnlySpan<byte> bytes, ushort type)
    {
        _ = SpaParser.Parse(bytes);
        return SpaDirectory.Parse(bytes).Find(type)?.Payload.ToArray() ?? Array.Empty<byte>();
    }

    /// <summary>Writes bytes in their original stored order, requiring the exact payload length.</summary>
    public static Span<byte> WriteYUnitAsSpanByte(Span<byte> bytes, Span<byte> yUnits)
    {
        _ = SpaParser.Parse(bytes);
        var section = SpaDirectory.Parse(bytes).Find(3) ?? throw new InvalidDataException("SPA has no default-selector primary data section.");
        if (yUnits.Length != section.Length)
            throw new ArgumentException($"Replacement payload must have exactly {section.Length} bytes.", nameof(yUnits));
        yUnits.CopyTo(bytes.Slice((int)section.Offset, (int)section.Length));
        return bytes;
    }

    /// <summary>Writes values in the same normalized order returned by the float reader.</summary>
    public static Span<byte> WriteYUnitAsSpanFloat(Span<byte> bytes, Span<float> yUnit)
    {
        var document = SpaParser.Parse(bytes);
        var primary = document.Series.FirstOrDefault(series => series.Kind == "spectrum" && series.SecondarySelector == 0 && series.TertiarySelector == 0)
            ?? throw new InvalidDataException("SPA has no default-selector primary data series.");
        if (yUnit.Length != primary.Y.Length)
            throw new ArgumentException($"Replacement signal must have exactly {primary.Y.Length} values.", nameof(yUnit));
        var stored = yUnit.ToArray();
        if (primary.Metadata.YUnitCode != 22) Array.Reverse(stored);
        var encoded = new byte[checked(stored.Length * 4)];
        for (var index = 0; index < stored.Length; index++)
            BinaryPrimitives.WriteSingleLittleEndian(encoded.AsSpan(index * 4, 4), stored[index]);
        return WriteYUnitAsSpanByte(bytes, encoded);
    }

    public static byte ReadByteAt(this ref Span<byte> data, int offset)
    {
        CheckRange(data, offset, 1);
        return data[offset];
    }
    public static short ReadInt16At(this ref Span<byte> data, int offset)
    {
        CheckRange(data, offset, 2);
        return BinaryPrimitives.ReadInt16LittleEndian(data.Slice(offset, 2));
    }
    public static int ReadInt32At(this ref Span<byte> data, int offset)
    {
        CheckRange(data, offset, 4);
        return BinaryPrimitives.ReadInt32LittleEndian(data.Slice(offset, 4));
    }
    private static void CheckRange(ReadOnlySpan<byte> data, int offset, int width)
    {
        if (offset < 0 || offset > data.Length - width)
            throw new ArgumentOutOfRangeException(nameof(offset), "Read extends outside the supplied data.");
    }
}
