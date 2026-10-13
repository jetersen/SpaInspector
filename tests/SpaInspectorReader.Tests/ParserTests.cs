using System.Buffers.Binary;
using System.Globalization;
using System.Text;
using System.Text.Json;
using SpaTestUtils;

namespace SpaInspectorReader.Tests;

public class ParserTests
{
    [Theory]
    [InlineData("test1.spa", 3)]
    [InlineData("test2.spa", 2)]
    public void ExistingFixturesPreserveSourceAndForwardInterferograms(string file, int seriesCount)
    {
        var bytes = File.ReadAllBytes(TestFile(file));
        var copy = bytes.ToArray();
        var spa = SpaParser.Parse(bytes);
        Assert.Equal(seriesCount, spa.Series.Count);
        Assert.Equal(copy, bytes);
        Assert.Equal(copy, spa.SourceBytes);
        Assert.Equal(6846u, spa.Spectrum!.Metadata.PointCount);
        Assert.Equal(700.033447265625, spa.Spectrum.X[0]);
        Assert.Equal(4000.1220703125, spa.Spectrum.X[^1]);
        var sample = spa.SampleInterferogram!;
        Assert.Equal(12200, sample.Y.Length);
        Assert.Equal(0d, sample.X[0]);
        Assert.Equal(12199d, sample.X[^1]);
        if (file == "test1.spa")
        {
            Assert.Equal(-1.9441359E-05f, sample.Y[0]);
            Assert.Equal(-0.0009917123f, sample.Y[^1]);
            Assert.Equal(0.00012780126f, spa.BackgroundSignal!.Y[0]);
        }
        else Assert.Null(spa.BackgroundSignal);
    }

    [Fact]
    public void StandaloneInterferogramUsesForwardValuesAndAscendingIndexAxis()
    {
        var spa = SpaParser.Parse(SpaFixture.Spectrum(1, 2, 22, 0, 3, 10, 20, 30, 40));
        Assert.Equal(new float[] { 10, 20, 30, 40 }, spa.Spectrum!.Y);
        Assert.Equal(new double[] { 0, 1, 2, 3 }, spa.Spectrum.X);
        Assert.Equal(22u, spa.Headers.YUnitCode);
        Assert.Equal(1, spa.Headers.DataSpacing);
    }

    [Fact]
    public void CountedDirectoryUsesFullTypeAndSelectorsAndKeepsUnknownRecords()
    {
        var bytes = SpaFixture.Build(new(2, SpaFixture.Header()), new(0, []),
            new(259, [99]), new(3, SpaFixture.Floats(1, 2, 3, 4), 1, uint.MaxValue),
            new(2, SpaFixture.Header(), 1, uint.MaxValue), new(3, SpaFixture.Floats(10, 20, 30, 40)),
            new(384, [0x80, 0xFF]), new(3, SpaFixture.Floats(50, 60, 70, 80)));
        var directory = SpaDirectory.Parse(bytes);
        Assert.Equal(8, directory.Entries.Count);
        Assert.Null(directory.Find(128));
        Assert.Equal(new byte[] { 0x80, 0xFF }, directory.Find(384)!.Payload);
        Assert.Equal(SpaFixture.Floats(10, 20, 30, 40), directory.Find(3)!.Payload);
        Assert.Equal(SpaFixture.Floats(1, 2, 3, 4), directory.Find(3, 1, uint.MaxValue)!.Payload);
        var spa = SpaParser.Parse(bytes);
        Assert.Equal(2, spa.Series.Count);
        Assert.Contains(spa.Warnings, warning => warning.Contains("Duplicate section triple"));
        Assert.Equal(bytes, spa.SourceBytes);
        Assert.Equal(new byte[] { 99 }, spa.Sections.Single(section => section.Type == 259).Payload);
    }

    [Theory]
    [InlineData(288, 58)]
    [InlineData(290, 521)]
    public void IndependentHeaderFieldsAreReadAtTheirOwnOffsets(int offset, ushort value)
    {
        var bytes = SpaFixture.Spectrum();
        BinaryPrimitives.WriteUInt16LittleEndian(bytes.AsSpan(offset, 2), value);
        var spa = SpaParser.Parse(bytes);
        Assert.Equal(1, spa.DirectoryAllocationUnits);
        if (offset == 288)
        {
            Assert.Equal(value, spa.FileTypeCode);
            Assert.Contains(spa.Warnings, warning => warning.Contains("unvalidated container classification"));
        }
        else Assert.Equal(value, spa.RevisionCode);
    }

    [Fact]
    public void LaserFrequencyUsesVerifiedPrimaryOffset80AndRemainsNullWhenAbsent()
    {
        var header = SpaFixture.Header();
        BinaryPrimitives.WriteSingleLittleEndian(header.AsSpan(80, 4), 15800.25f);
        BinaryPrimitives.WriteSingleLittleEndian(header.AsSpan(84, 4), 1234.5f);
        var spa = SpaParser.Parse(SpaFixture.Build(new(2, header), new(3, SpaFixture.Floats(1, 2, 3, 4))));
        Assert.Equal(15800.25f, spa.Spectrum!.Metadata.LaserFrequencyCmInverse);
        using var json = JsonDocument.Parse(spa.ToJson());
        Assert.Equal(15800.25f, json.RootElement.GetProperty("series")[0].GetProperty("metadata").GetProperty("laserFrequencyCmInverse").GetSingle());
        var shortHeader = header[..80];
        var shortSpa = SpaParser.Parse(SpaFixture.Build(new(2, shortHeader), new(3, SpaFixture.Floats(1, 2, 3, 4))));
        Assert.Null(shortSpa.Spectrum!.Metadata.LaserFrequencyCmInverse);
    }

    [Theory]
    [InlineData(11670.25f, 11670.25f)]
    [InlineData(0f, null)]
    [InlineData(-1f, null)]
    public void InterferogramLaserFrequencyUsesPositiveOffset20Only(float storedFrequency, float? expected)
    {
        var header = SpaFixture.InterferogramHeader(2);
        BinaryPrimitives.WriteSingleLittleEndian(header.AsSpan(16, 4), 9876.5f);
        BinaryPrimitives.WriteSingleLittleEndian(header.AsSpan(20, 4), storedFrequency);
        var spa = SpaParser.Parse(SpaFixture.Build(new(2, SpaFixture.Header()), new(3, SpaFixture.Floats(1, 2, 3, 4)), new(100, header), new(102, SpaFixture.Floats(8, 9))));
        Assert.Equal(expected, spa.SampleInterferogram!.Metadata.LaserFrequencyCmInverse);
        var truncated = header[..20];
        var shortSpa = SpaParser.Parse(SpaFixture.Build(new(2, SpaFixture.Header()), new(3, SpaFixture.Floats(1, 2, 3, 4)), new(100, truncated), new(102, SpaFixture.Floats(8, 9))));
        Assert.Null(shortSpa.SampleInterferogram!.Metadata.LaserFrequencyCmInverse);
    }

    [Fact]
    public void UnobservedStorageCodeUsesRecoveredNativeFallbackAndRemainsVisible()
    {
        var spa = SpaParser.Parse(SpaFixture.Spectrum(mode: 99));
        Assert.Equal(99u, spa.Spectrum!.Metadata.StorageMode);
        Assert.Equal(new float[] { 4, 3, 2, 1 }, spa.Spectrum.Y);
        Assert.Equal(new double[] { 1, 2, 3, 4 }, spa.Spectrum.X);
        Assert.Contains(spa.Warnings, warning => warning.Contains("native float32 fallback"));
    }

    [Fact]
    public void LegacyFourByteInterferogramHeaderUsesOnlyEstablishedPointCount()
    {
        var count = new byte[4]; BinaryPrimitives.WriteUInt32LittleEndian(count, 2);
        var bytes = SpaFixture.Build(new(2, SpaFixture.Header()), new(3, SpaFixture.Floats(1, 2, 3, 4)), new(100, count), new(102, SpaFixture.Floats(8, 9)));
        var signal = SpaParser.Parse(bytes).SampleInterferogram!;
        Assert.Equal(new float[] { 8, 9 }, signal.Y);
        Assert.Null(signal.Metadata.ScanCount);
        Assert.Equal(count, signal.Metadata.RawHeader);
    }

    [Fact]
    public void UnitCodesRemainFullWidthAndUnknownUnitsKeepTheirCodes()
    {
        var spa = SpaParser.Parse(SpaFixture.Spectrum(xCode: 257, yCode: 273));
        Assert.Equal(257u, spa.Spectrum!.Metadata.XUnitCode);
        Assert.Equal(273u, spa.Spectrum.Metadata.YUnitCode);
        Assert.Equal("code 257", spa.Spectrum.Metadata.XUnit);
        Assert.Equal("code 273", spa.Spectrum.Metadata.YUnit);
    }

    [Fact]
    public void LegacyTextFallbackPreservesBytesLinesAndExplicitEncodingChoice()
    {
        var history = new byte[] { (byte)'A', 0x80, 0xE9, 13, 10, 32, 32, 0, 0xFF };
        var bytes = SpaFixture.Build(new(2, SpaFixture.Header()), new(3, SpaFixture.Floats(1, 2, 3, 4)), new(27, history));
        var automatic = SpaParser.Parse(bytes);
        Assert.Equal("A€é\r\n  ", automatic.History);
        Assert.Equal(history[..^2], automatic.HistoryBytes);
        Assert.Equal("windows-1252", automatic.HistoryEncoding);
        Assert.Contains(automatic.Warnings, warning => warning.Contains("producer code page is unverified"));
        Assert.Equal(history, automatic.Sections.Single(section => section.Type == 27).Payload);
        var latin = SpaParser.Parse(bytes, new() { TextEncoding = SpaTextEncoding.Latin1 });
        Assert.Equal("A\u0080é\r\n  ", latin.History);
        Assert.Throws<InvalidDataException>(() => SpaParser.Parse(bytes, new() { TextEncoding = SpaTextEncoding.Utf8 }));
    }

    [Fact]
    public void ValidUtf8WinsAutomaticDetectionAndTitleReadIsBounded()
    {
        var bytes = SpaFixture.Spectrum();
        bytes.AsSpan(30, 256).Fill((byte)'A');
        var spa = SpaParser.Parse(bytes);
        Assert.Equal(256, spa.TitleBytes.Length);
        Assert.Equal(new string('A', 256), spa.Title);
        var utf8 = Encoding.UTF8.GetBytes("é€\n");
        bytes = SpaFixture.Build(new(2, SpaFixture.Header()), new(3, SpaFixture.Floats(1, 2, 3, 4)), new(27, utf8));
        Assert.Equal("é€\n", SpaParser.Parse(bytes).History);
        Assert.Equal("utf-8", SpaParser.Parse(bytes).HistoryEncoding);
    }

    [Theory]
    [InlineData(0)]
    [InlineData(1)]
    public void ZeroAndOnePointSignalsAvoidDivisionByZero(int count)
    {
        var bytes = SpaFixture.Build(new(2, SpaFixture.Header(points: (uint)count)), new(3, SpaFixture.Floats(Enumerable.Repeat(7f, count).ToArray())));
        var spa = SpaParser.Parse(bytes);
        Assert.Equal(count, spa.Spectrum!.X.Length);
        Assert.Equal(count, spa.Spectrum.Y.Length);
        Assert.Equal(0, spa.Headers.DataSpacing);
        if (count == 1) Assert.Equal(1d, spa.Spectrum.X[0]);
    }

    [Fact]
    public void AssociatedAndAuxiliaryInterferogramsUseAllSelectorsAndForwardOrder()
    {
        var bytes = SpaFixture.Build(new(2, SpaFixture.Header()), new(3, SpaFixture.Floats(1, 2, 3, 4)),
            new(100, SpaFixture.InterferogramHeader(2), 9, 17), new(102, SpaFixture.Floats(8, 9), 9, 17),
            new(379, SpaFixture.InterferogramHeader(2)), new(381, SpaFixture.Floats(12, 13)));
        var spa = SpaParser.Parse(bytes);
        var sample = spa.Series.Single(series => series.Kind == "sample_interferogram");
        Assert.Equal(new float[] { 8, 9 }, sample.Y);
        Assert.Equal(9, sample.SecondarySelector);
        Assert.Equal(17u, sample.TertiarySelector);
        Assert.Equal(7u, sample.Metadata.ScanCount);
        Assert.Null(sample.Metadata.SignalStrength);
        Assert.Empty(sample.Metadata.RawSettings);
        Assert.Equal(new float[] { 12, 13 }, spa.Series.Single(series => series.Kind == "auxiliary_interferogram_381").Y);
        Assert.Contains(spa.Warnings, warning => warning.Contains("role is unverified"));
    }

    [Fact]
    public void UnpairedInterferogramDataIsRejected()
    {
        var bytes = SpaFixture.Build(new(2, SpaFixture.Header()), new(3, SpaFixture.Floats(1, 2, 3, 4)), new(102, SpaFixture.Floats(8, 9)));
        Assert.Throws<InvalidDataException>(() => SpaParser.Parse(bytes));
    }

    [Fact]
    public void StreamAdapterLeavesCallerOpenUnlessExplicitlyOwned()
    {
        using var stream = new MemoryStream(SpaFixture.Spectrum());
        _ = stream.ReadSpa();
        Assert.True(stream.CanRead);
        _ = stream.ReadSpa(leaveOpen: false);
        Assert.False(stream.CanRead);
    }

    [Fact]
    public void JsonAndCsvPreserveNonFiniteValuesAndIndependentUnitCodes()
    {
        var spa = SpaParser.Parse(SpaFixture.Spectrum(values: [float.NaN, float.PositiveInfinity, float.NegativeInfinity, 0.25f]));
        using var json = JsonDocument.Parse(spa.ToJson());
        var root = json.RootElement;
        Assert.Equal(1, root.GetProperty("schemaVersion").GetInt32());
        Assert.Equal(spa.SourceBytes, root.GetProperty("sourceBytes").GetBytesFromBase64());
        Assert.Equal("-Infinity", root.GetProperty("series")[0].GetProperty("y")[1].GetString());
        Assert.Equal("NaN", root.GetProperty("series")[0].GetProperty("y")[3].GetString());
        Assert.False(root.TryGetProperty("headers", out _));
        var previous = CultureInfo.CurrentCulture;
        try
        {
            CultureInfo.CurrentCulture = CultureInfo.GetCultureInfo("da-DK");
            var csv = spa.ToCsv();
            Assert.StartsWith("series,secondary_selector,tertiary_selector,index,x,y,x_unit_code,y_unit_code\n", csv);
            Assert.Contains("spectrum,0,0,0,1,0.25,1,17\n", csv);
            Assert.Contains(",NaN,", csv);
        }
        finally { CultureInfo.CurrentCulture = previous; }
    }

    [Fact]
    public void RejectsInvalidMagicAllocationRangeAlignmentAndPointCounts()
    {
        Assert.Throws<InvalidDataException>(() => SpaParser.Parse([]));
        var bytes = SpaFixture.Spectrum(); bytes[0] = 0;
        Assert.Throws<InvalidDataException>(() => SpaParser.Parse(bytes));
        bytes = SpaFixture.Spectrum(); bytes[20] = 1;
        Assert.Throws<InvalidDataException>(() => SpaParser.Parse(bytes));
        bytes = SpaFixture.Spectrum(); BinaryPrimitives.WriteUInt16LittleEndian(bytes.AsSpan(292, 2), 10001);
        Assert.Throws<InvalidDataException>(() => SpaParser.Parse(bytes));
        bytes = SpaFixture.Spectrum(); BinaryPrimitives.WriteUInt32LittleEndian(bytes.AsSpan(306, 4), uint.MaxValue);
        Assert.Throws<InvalidDataException>(() => SpaParser.Parse(bytes));
        bytes = SpaFixture.Spectrum(); BinaryPrimitives.WriteUInt32LittleEndian(bytes.AsSpan(306, 4), 0);
        Assert.Throws<InvalidDataException>(() => SpaParser.Parse(bytes));
        bytes = SpaFixture.Spectrum(); BinaryPrimitives.WriteUInt32LittleEndian(bytes.AsSpan(326, 4), 15);
        Assert.Throws<InvalidDataException>(() => SpaParser.Parse(bytes));
        bytes = SpaFixture.Build(new(2, SpaFixture.Header(points: 5)), new(3, SpaFixture.Floats(1, 2, 3, 4)));
        Assert.Throws<InvalidDataException>(() => SpaParser.Parse(bytes));
        bytes = SpaFixture.Build(new(2, SpaFixture.Header()), new(3, SpaFixture.Floats(1, 2, 3, 4)), new(100, SpaFixture.InterferogramHeader(3)), new(102, SpaFixture.Floats(1, 2)));
        Assert.Throws<InvalidDataException>(() => SpaParser.Parse(bytes));
    }
}
