using SpaInspectorReader;
using SpaTestUtils;

namespace SpaFileReader.Tests;

public class CompatibilityTests
{
    [Fact]
    public void EveryReadApiLeavesInputUnchangedAndRepeatedResultsMatch()
    {
        var bytes = File.ReadAllBytes(TestFile("test1.spa"));
        var snapshot = bytes.ToArray();
        var first = SpaFile.ReadYUnitAsFloatArray(bytes);
        Assert.Equal(first, SpaFile.ReadYUnitAsFloatArray(bytes));
        var doubles = SpaFile.ReadYUnitAsDoubleArray(bytes);
        Assert.Equal(first.Select(value => (double)value), doubles);
        _ = SpaFile.ReadInterferogramAsSpanFloat(bytes);
        _ = SpaFile.ReadBackgroundInterogramAsSpanFloat(bytes);
        var raw = SpaFile.ReadYUnitAsSpanByte(bytes);
        raw[0] ^= 0xFF;
        Assert.Equal(snapshot, bytes);
    }

    [Fact]
    public void VoltsPrimaryIsForwardAndInterferogramsAgreeBetweenReaders()
    {
        var bytes = SpaFixture.Spectrum(1, 2, 22, 0, 3, 1, 2, 3, 4);
        Assert.Equal(new float[] { 1, 2, 3, 4 }, SpaFile.ReadYUnitAsFloatArray(bytes));
        var fixture = File.ReadAllBytes(TestFile("test1.spa"));
        var full = SpaParser.Parse(fixture);
        Assert.Equal(full.UnitInterferogram, SpaFile.ReadInterferogramAsSpanFloat(fixture).ToArray());
        Assert.Equal(full.BackgroundInterferogram, SpaFile.ReadBackgroundInterogramAsSpanFloat(fixture).ToArray());
    }

    [Fact]
    public void WriterLengthErrorsDoNotPartiallyModifyDestination()
    {
        var bytes = File.ReadAllBytes(TestFile("test1.spa"));
        var copy = bytes.ToArray();
        Assert.Throws<ArgumentException>(() => SpaFile.WriteYUnitAsSpanByte(bytes, new byte[3]));
        Assert.Equal(copy, bytes);
        Assert.Throws<ArgumentException>(() => SpaFile.WriteYUnitAsSpanFloat(bytes, new float[3]));
        Assert.Equal(copy, bytes);
    }

    [Fact]
    public void FloatWriterAcceptsReaderOrderWithoutModifyingReplacement()
    {
        var bytes = SpaFixture.Spectrum();
        var replacement = new float[] { 10, 20, 30, 40 };
        SpaFile.WriteYUnitAsSpanFloat(bytes, replacement);
        Assert.Equal(new float[] { 10, 20, 30, 40 }, replacement);
        Assert.Equal(replacement, SpaFile.ReadYUnitAsFloatArray(bytes));
        Assert.Equal(SpaFixture.Floats(40, 30, 20, 10), SpaFile.ReadYUnitAsSpanByte(bytes).ToArray());
    }

    [Fact]
    public void InvalidAndMisalignedFilesAreRejectedByLegacyEntryPoints()
    {
        Assert.Throws<InvalidDataException>(() => SpaFile.ReadYUnitAsFloatArray([]));
        var bytes = SpaFixture.Build(new(2, SpaFixture.Header(points: 1)), new(3, [1, 2, 3]));
        Assert.Throws<InvalidDataException>(() => SpaFile.ReadYUnitAsFloatArray(bytes));
        Assert.Throws<InvalidDataException>(() => SpaFile.ReadInterferogramAsSpanFloat(bytes));
    }

    [Fact]
    public void GroupAccessorKeepsLegacyUnpairedDataAndDoesNotMutateIt()
    {
        var bytes = SpaFixture.Build(new(3, SpaFixture.Floats(1, 2, 3, 4)),
            new(3, SpaFixture.Floats(5, 6), 9));
        var snapshot = bytes.ToArray();
        var groups = SpgFile.ReadYUnitAsFloatArray(bytes).ToArray();
        Assert.Equal(new float[] { 4, 3, 2, 1 }, groups[0]);
        Assert.Equal(new float[] { 6, 5 }, groups[1]);
        Assert.Equal(snapshot, bytes);
    }

    [Fact]
    public void MultiPrimaryAccessorDoesNotMutateItsInput()
    {
        var bytes = SpaFixture.Build(new(2, SpaFixture.Header()), new(3, SpaFixture.Floats(1, 2, 3, 4)),
            new(2, SpaFixture.Header(), 2), new(3, SpaFixture.Floats(5, 6, 7, 8), 2));
        var snapshot = bytes.ToArray();
        var first = SpgFile.ReadYUnitAsFloatArray(bytes).ToArray();
        var second = SpgFile.ReadYUnitAsFloatArray(bytes).ToArray();
        Assert.Equal(2, first.Length);
        Assert.Equal(first[0], second[0]);
        Assert.Equal(first[1], second[1]);
        Assert.Equal(snapshot, bytes);
    }
}
